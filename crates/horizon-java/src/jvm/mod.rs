//! The embedded JVM (cargo feature `jvm`; docs/java-mod-runtime.md): started through JNI's
//! invocation API from the player's own Java (`crate::locate`), never bundled.
//!
//! [`start`] defines the layer's own classes (`java/`: Mixin's callback classes, the Java API
//! layer with vanilla's names, Fabric API's facade) in the system class loader and registers the
//! natives they call ([`natives`]). [`Layer::load`] loads the mods' jars with one class loader,
//! binds each supported injection to its hook point, and runs the mods' entry points;
//! [`Layer::install`] makes [`crate::hooks::run`] call them.
//!
//! What a handler gets: `this` is the target object when the Java API layer has the mixin's
//! target class (the mixin class is rebased onto it, [`crate::classfile::rebase`], and one
//! instance stands for each target: the server, a level, an entity, a block), else one instance
//! of the mixin class; the target method's arguments as the hook point's [`Site`] has them (the
//! API layer's objects; zero for primitives); and a `CallbackInfo` (or `CallbackInfoReturnable`
//! with the return value) reused per handler and thread.
mod natives;

pub use natives::take_registrations;

use crate::{
    classfile,
    fabric::{self, Entrypoint, Event},
    hooks::{self, Hook, Host, Obj, Side, Site, Slot},
};
use jni::{
    InitArgsBuilder, JNIEnv, JNIVersion, JavaVM, NativeMethod,
    objects::{
        GlobalRef, JClass, JFieldID, JMethodID, JObject, JObjectArray, JStaticMethodID, JValue,
    },
    signature::{Primitive, ReturnType},
    sys::jvalue,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

include!(concat!(env!("OUT_DIR"), "/classes.rs"));

const LAYER: &str = "dev/rustyminecraft/java/Layer";
const OBJECTS: &str = "dev/rustyminecraft/java/Objects";
const GAME_EVENT: &str = "dev/rustyminecraft/java/fabric/GameEvent";
const LOADER: &str = "dev/rustyminecraft/java/fabric/Loader";
const INFO: &str = "org/spongepowered/asm/mixin/injection/callback/CallbackInfo";
const RETURNABLE: &str = "org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable";
const RESULT: &str = "net/minecraft/world/InteractionResult";

static VM: OnceLock<JavaVM> = OnceLock::new();
/// `Layer`, defined in the system class loader at start.
static LAYER_CLASS: OnceLock<GlobalRef> = OnceLock::new();

/// Whether the Java API layer has this class (internal name), to rebase a mixin onto.
pub fn api_has(class: &str) -> bool {
    !class.starts_with("dev/rustyminecraft/") && CLASSES.iter().any(|(n, _)| *n == class)
}

/// The API class a mixin of `target` is rebased onto: the target itself, or for
/// `BlockBehaviour` (every one of which is a `Block` in vanilla) `Block`, so `this` casts to a
/// block as in vanilla.
fn rebase_onto(target: &str) -> &str {
    match target {
        "net/minecraft/world/level/block/state/BlockBehaviour" => {
            "net/minecraft/world/level/block/Block"
        }
        t => t,
    }
}

/// The Java API layer's classes (internal names): `net/minecraft/...`, `net/fabricmc/...` and
/// Mixin's callback classes, with their class files.
pub fn api_classes() -> impl Iterator<Item = (&'static str, &'static [u8])> {
    CLASSES
        .iter()
        .copied()
        .filter(|(n, _)| !n.starts_with("dev/rustyminecraft/"))
}

/// A supported injection to attach: its hook point and its handler in a mod jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub hook: Hook,
    pub jar: PathBuf,
    /// The mixin class, internal name: `dev/example/tickcounter/mixin/TickMixin`.
    pub class: String,
    /// The class it mixes into, internal name: `net/minecraft/server/MinecraftServer`.
    pub target: String,
    /// The handler and its descriptor.
    pub member: String,
    pub descriptor: String,
    pub is_static: bool,
    pub cancellable: bool,
    /// The vanilla method it injects into (`CallbackInfo.getId`).
    pub method: String,
    /// A `@ModifyReturnValue`: the handler takes the return value and returns the new one,
    /// instead of taking a `CallbackInfo`.
    pub modifies: bool,
    /// The mixin config's list it is in: `client` or `server` only (None: both).
    pub side: Option<Side>,
}

/// What the layer loads: the mods' injections and entry points.
#[derive(Debug, Clone, Default)]
pub struct Mods {
    pub bindings: Vec<Binding>,
    pub entrypoints: Vec<Entrypoint>,
    /// The ids of the mods that are on (`FabricLoader.isModLoaded`).
    pub mod_ids: Vec<String>,
    /// The game folder (`FabricLoader.getGameDir`).
    pub game_dir: PathBuf,
    /// A dedicated server (its entry points are `main` and `server`); else a game with a window.
    pub dedicated: bool,
}

/// A handler's parameter, by its descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Param {
    Info,
    Returnable,
    Bool,
    Int,
    Long,
    Float,
    Double,
    Object,
}

/// A descriptor's parameters (`(Ljava/util/function/BooleanSupplier;L…/CallbackInfo;)V`).
fn params(descriptor: &str) -> Option<Vec<Param>> {
    let inner = descriptor.strip_prefix('(')?.split(')').next()?;
    let mut out = Vec::new();
    let mut chars = inner.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        out.push(match c {
            'Z' => Param::Bool,
            'B' | 'C' | 'S' | 'I' => Param::Int,
            'J' => Param::Long,
            'F' => Param::Float,
            'D' => Param::Double,
            'L' => {
                let end = inner[i..].find(';')? + i;
                let name = &inner[i + 1..end];
                while chars.peek().is_some_and(|(j, _)| *j <= end) {
                    chars.next();
                }
                match name {
                    INFO => Param::Info,
                    RETURNABLE => Param::Returnable,
                    _ => Param::Object,
                }
            }
            '[' => {
                while chars.peek().is_some_and(|(_, c)| *c == '[') {
                    chars.next();
                }
                if let Some((j, 'L')) = chars.next() {
                    let end = inner[j..].find(';')? + j;
                    while chars.peek().is_some_and(|(k, _)| *k <= end) {
                        chars.next();
                    }
                }
                Param::Object
            }
            _ => return None,
        });
    }
    Some(out)
}

/// The value a hook point carries, as Java sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Void,
    /// A `float` (`java.lang.Float` in a `CallbackInfoReturnable`).
    Float,
    /// A `boolean`.
    Bool,
    /// Whether something took the click, as an `InteractionResult`.
    Result,
    /// A value the layer can't hand over yet (a list of item stacks).
    Other,
}

fn kind(hook: Hook) -> Kind {
    match hook {
        Hook::GetDestroyProgressHead | Hook::GetDestroyProgressReturn => Kind::Float,
        Hook::InteractOnHead | Hook::InteractOnReturn => Kind::Result,
        Hook::GetDropsHead | Hook::GetDropsReturn => Kind::Other,
        _ => Kind::Void,
    }
}

/// The handle an API object standing for a target is bound with (`rm$bind`).
fn handle(this: Option<&Obj>) -> i64 {
    match this {
        Some(Obj::Entity(id) | Obj::Level(id)) => i64::from(*id),
        Some(Obj::Block(state) | Obj::BlockState(state)) => i64::from(*state),
        _ => 0,
    }
}

/// Starts the JVM from the library at `libjvm`, once per process (a second call returns the
/// first JVM), and defines the layer's own classes.
pub fn start(libjvm: &Path) -> Result<&'static JavaVM, String> {
    if let Some(vm) = VM.get() {
        return Ok(vm);
    }
    // -Xrs: the game keeps its own signals (Ctrl+C, termination); the JVM installs only what it
    // needs to run. Headless: no mod opens a window of its own.
    let args = InitArgsBuilder::new()
        .version(JNIVersion::V8)
        .option("-Xrs")
        .option("-Djava.awt.headless=true")
        .build()
        .map_err(|e| format!("JVM options: {e}"))?;
    let vm = JavaVM::with_libjvm(args, || Ok(libjvm.to_path_buf()))
        .map_err(|e| format!("{}: {e}", libjvm.display()))?;
    let vm = VM.get_or_init(|| vm);
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| e.to_string())?;
    let layer = define_layer(&mut env).map_err(|e| describe(&mut env, e))?;
    let _ = LAYER_CLASS.set(layer);
    Ok(vm)
}

/// The JVM [`start`] started, if any.
pub fn vm() -> Option<&'static JavaVM> {
    VM.get()
}

/// The layer's classes in an order the JVM can define them in: each after its superclass and
/// interfaces.
fn defining_order() -> Vec<(&'static str, &'static [u8])> {
    let ours: HashSet<&str> = CLASSES.iter().map(|(n, _)| *n).collect();
    let mut pending: Vec<(&'static str, &'static [u8])> = CLASSES.to_vec();
    let mut done: HashSet<String> = HashSet::new();
    let mut order = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let ready = pending.iter().position(|(_, bytes)| {
            classfile::names(bytes).is_none_or(|(_, sup, interfaces)| {
                sup.iter()
                    .chain(&interfaces)
                    .all(|n| !ours.contains(n.as_str()) || done.contains(n))
            })
        });
        // A cycle can't happen in valid classes; define the rest as they come.
        let next = pending.remove(ready.unwrap_or(0));
        done.insert(next.0.to_owned());
        order.push(next);
    }
    order
}

/// Defines `java/`'s classes in the system class loader and registers the natives.
fn define_layer(env: &mut JNIEnv) -> jni::errors::Result<GlobalRef> {
    let system = env
        .call_static_method(
            "java/lang/ClassLoader",
            "getSystemClassLoader",
            "()Ljava/lang/ClassLoader;",
            &[],
        )?
        .l()?;
    for (name, bytes) in defining_order() {
        env.define_class(name, &system, bytes)?;
    }
    let layer = env.find_class(LAYER)?;
    env.register_native_methods(
        &layer,
        &[NativeMethod {
            name: "rust".into(),
            sig: "()V".into(),
            fn_ptr: rust as *mut c_void,
        }],
    )?;
    natives::register(env)?;
    env.new_global_ref(layer)
}

/// `Layer.rust`: a call from Java into Rust that does nothing, for the measurement.
extern "system" fn rust(_env: JNIEnv, _class: JClass) {}

/// A JNI error as text, with the Java exception (cleared) when there is one.
fn describe(env: &mut JNIEnv, e: jni::errors::Error) -> String {
    if env.exception_check().unwrap_or(false) {
        let thrown = env.exception_occurred().ok();
        let _ = env.exception_clear();
        if let Some(t) = thrown
            && let Ok(text) = env.call_method(&t, "toString", "()Ljava/lang/String;", &[])
            && let Ok(obj) = text.l()
            && let Ok(s) = env.get_string(&obj.into())
        {
            return s.into();
        }
    }
    e.to_string()
}

/// One handler, ready to call.
struct Bound {
    /// Its index among all handlers: its `CallbackInfo` per thread.
    index: usize,
    /// Its class's index: the instances standing for targets, per thread.
    class_index: usize,
    /// `Class.member`, for the log.
    name: String,
    /// The vanilla method's name as a Java string, made once (`CallbackInfo.getId`).
    method_name: GlobalRef,
    class: GlobalRef,
    /// The class stands for its target (rebased onto an API class): `rm$bind(long)`.
    binds: Option<JMethodID>,
    /// The one instance of a class that isn't rebased.
    instance: Option<GlobalRef>,
    method: Option<JMethodID>,
    static_method: Option<JStaticMethodID>,
    params: Vec<Param>,
    returnable: bool,
    /// A `@ModifyReturnValue` handler's value type.
    modifies: Option<Kind>,
    cancellable: bool,
    side: Option<Side>,
    /// A Java exception was logged for it already.
    warned: AtomicBool,
}

/// Method and field ids the calls use, looked up once.
struct Ids {
    info: (GlobalRef, JMethodID),
    returnable: (GlobalRef, JMethodID),
    cancelled: JFieldID,
    return_value: JFieldID,
    objects: GlobalRef,
    level: JStaticMethodID,
    entity: JStaticMethodID,
    block_pos: JStaticMethodID,
    block_state: JStaticMethodID,
    block: JStaticMethodID,
    stack: JStaticMethodID,
    hand: JStaticMethodID,
    result: JStaticMethodID,
    consumed: JStaticMethodID,
    server: GlobalRef,
    always: GlobalRef,
    float: (GlobalRef, JStaticMethodID, JMethodID),
    boolean: (GlobalRef, JStaticMethodID, JMethodID),
    game_event: GlobalRef,
    fire: JStaticMethodID,
}

thread_local! {
    /// Each handler's `CallbackInfo` on this thread, by handler index.
    static INFOS: RefCell<Vec<Option<GlobalRef>>> = const { RefCell::new(Vec::new()) };
    /// The instances standing for targets on this thread, by (class index, handle).
    static TARGETS: RefCell<HashMap<(usize, i64), GlobalRef>> = RefCell::new(HashMap::new());
}

/// How many targets a mixin class keeps instances for per thread (its own fields live in them);
/// past it they start over.
const MAX_TARGETS: usize = 4096;

/// The mods' class loader and their bound handlers, per hook point.
pub struct Layer {
    vm: &'static JavaVM,
    bound: Vec<Vec<Bound>>,
    ids: Ids,
    /// The mixin classes, by internal name.
    classes: BTreeMap<String, GlobalRef>,
    /// A Fabric API event's listener threw: logged once per event.
    event_warned: [AtomicBool; Event::ALL.len()],
    /// Keeps the mods' classes loaded.
    loader: GlobalRef,
}

fn ids(env: &mut JNIEnv) -> jni::errors::Result<Ids> {
    let info_class = env.find_class(INFO)?;
    let returnable_class = env.find_class(RETURNABLE)?;
    let objects = env.find_class(OBJECTS)?;
    let obj =
        |env: &mut JNIEnv, name: &str, sig: &str| env.get_static_method_id(OBJECTS, name, sig);
    let server = env
        .call_static_method(
            &objects,
            "server",
            "()Lnet/minecraft/server/MinecraftServer;",
            &[],
        )?
        .l()?;
    let always = env
        .get_static_field(&objects, "ALWAYS", "Ljava/util/function/BooleanSupplier;")?
        .l()?;
    let float_class = env.find_class("java/lang/Float")?;
    let boolean_class = env.find_class("java/lang/Boolean")?;
    let game_event = env.find_class(GAME_EVENT)?;
    Ok(Ids {
        info: (
            env.new_global_ref(&info_class)?,
            env.get_method_id(&info_class, "<init>", "(Ljava/lang/String;Z)V")?,
        ),
        returnable: (
            env.new_global_ref(&returnable_class)?,
            env.get_method_id(
                &returnable_class,
                "<init>",
                "(Ljava/lang/String;ZLjava/lang/Object;)V",
            )?,
        ),
        cancelled: env.get_field_id(&info_class, "cancelled", "Z")?,
        return_value: env.get_field_id(&returnable_class, "returnValue", "Ljava/lang/Object;")?,
        level: obj(env, "level", "(I)Lnet/minecraft/world/level/Level;")?,
        entity: obj(env, "entity", "(I)Lnet/minecraft/world/entity/Entity;")?,
        block_pos: obj(env, "blockPos", "(III)Lnet/minecraft/core/BlockPos;")?,
        block_state: obj(
            env,
            "blockState",
            "(I)Lnet/minecraft/world/level/block/state/BlockState;",
        )?,
        block: obj(env, "block", "(I)Lnet/minecraft/world/level/block/Block;")?,
        stack: obj(
            env,
            "stack",
            "(Ljava/lang/String;I)Lnet/minecraft/world/item/ItemStack;",
        )?,
        hand: obj(env, "hand", "(Z)Lnet/minecraft/world/InteractionHand;")?,
        result: obj(env, "result", "(Z)Lnet/minecraft/world/InteractionResult;")?,
        consumed: obj(env, "consumed", "(Ljava/lang/Object;)Z")?,
        objects: env.new_global_ref(&objects)?,
        server: env.new_global_ref(server)?,
        always: env.new_global_ref(always)?,
        float: (
            env.new_global_ref(&float_class)?,
            env.get_static_method_id(&float_class, "valueOf", "(F)Ljava/lang/Float;")?,
            env.get_method_id(&float_class, "floatValue", "()F")?,
        ),
        boolean: (
            env.new_global_ref(&boolean_class)?,
            env.get_static_method_id(&boolean_class, "valueOf", "(Z)Ljava/lang/Boolean;")?,
            env.get_method_id(&boolean_class, "booleanValue", "()Z")?,
        ),
        fire: env.get_static_method_id(&game_event, "fire", "(ILjava/lang/Object;)V")?,
        game_event: env.new_global_ref(&game_event)?,
    })
}

impl Layer {
    /// Loads the mods' jars with one class loader, binds each handler, then runs the entry
    /// points. A handler that can't be bound or an entry point that fails is left out, with why
    /// in the returned list.
    pub fn load(vm: &'static JavaVM, mods: &Mods) -> Result<(Layer, Vec<String>), String> {
        let mut env = vm
            .attach_current_thread_as_daemon()
            .map_err(|e| e.to_string())?;
        let mut problems = Vec::new();
        let layer =
            Self::bind(&mut env, vm, mods, &mut problems).map_err(|e| describe(&mut env, e))?;
        layer
            .run_entrypoints(&mut env, mods, &mut problems)
            .map_err(|e| describe(&mut env, e))?;
        Ok((layer, problems))
    }

    fn bind(
        env: &mut JNIEnv,
        vm: &'static JavaVM,
        mods: &Mods,
        problems: &mut Vec<String>,
    ) -> jni::errors::Result<Layer> {
        let layer_class = <&JClass>::from(
            LAYER_CLASS
                .get()
                .ok_or(jni::errors::Error::NullPtr("layer"))?
                .as_obj(),
        );
        let mut jars: Vec<String> = mods
            .bindings
            .iter()
            .map(|b| &b.jar)
            .chain(mods.entrypoints.iter().map(|e| &e.jar))
            .map(|j| j.display().to_string())
            .collect();
        jars.sort();
        jars.dedup();
        let array: JObjectArray =
            env.new_object_array(jars.len() as i32, "java/lang/String", JObject::null())?;
        for (i, j) in jars.iter().enumerate() {
            let s = env.new_string(j)?;
            env.set_object_array_element(&array, i as i32, s)?;
        }
        let loader = env
            .call_static_method(
                layer_class,
                "loader",
                "([Ljava/lang/String;)Ljava/lang/ClassLoader;",
                &[JValue::Object(&array)],
            )?
            .l()?;
        let loader = env.new_global_ref(loader)?;
        let ids = ids(env)?;
        // Class name to (class, rm$bind when rebased, the one instance when not).
        type Defined = (GlobalRef, Option<JMethodID>, Option<GlobalRef>);
        let mut classes: Vec<(String, Defined)> = Vec::new();
        let mut bound: Vec<Vec<Bound>> = Hook::ALL.iter().map(|_| Vec::new()).collect();
        let mut index = 0;
        for b in &mods.bindings {
            let name = format!("{}.{}", b.class.replace('/', "."), b.member);
            let Some(params) = params(&b.descriptor) else {
                problems.push(format!("{name}: unreadable descriptor {}", b.descriptor));
                continue;
            };
            let class_index = match classes.iter().position(|(n, _)| *n == b.class) {
                Some(i) => i,
                None => match define(env, layer_class, loader.as_obj(), &b.class, &b.target) {
                    Ok(defined) => {
                        classes.push((b.class.clone(), defined));
                        classes.len() - 1
                    }
                    Err(e) => {
                        let why = describe(env, e);
                        problems.push(format!("{name}: {why}"));
                        continue;
                    }
                },
            };
            let (class, binds, instance) = &classes[class_index].1;
            let class_ref = <&JClass>::from(class.as_obj());
            let ids_found = if b.is_static {
                env.get_static_method_id(class_ref, &b.member, &b.descriptor)
                    .map(|m| (None, Some(m)))
            } else {
                env.get_method_id(class_ref, &b.member, &b.descriptor)
                    .map(|m| (Some(m), None))
            };
            let (method, static_method) = match ids_found {
                Ok(found) => found,
                Err(e) => {
                    let why = describe(env, e);
                    problems.push(format!("{name}: {why}"));
                    continue;
                }
            };
            let ret = b.descriptor.rsplit(')').next().unwrap_or("");
            let modifies = match (b.modifies, ret) {
                (false, _) => None,
                (true, "F") if params.first() == Some(&Param::Float) => Some(Kind::Float),
                (true, "Z") if params.first() == Some(&Param::Bool) => Some(Kind::Bool),
                (true, r) if r == format!("L{RESULT};") && kind(b.hook) == Kind::Result => {
                    Some(Kind::Result)
                }
                (true, _) => {
                    problems.push(format!(
                        "{name}: it changes a value of a type the Java layer can't hand over yet ({})",
                        b.descriptor
                    ));
                    continue;
                }
            };
            if !b.is_static && instance.is_none() && binds.is_none() {
                problems.push(format!(
                    "{name}: its class can't have an instance (an interface)"
                ));
                continue;
            }
            let method_name = env.new_string(&b.method)?;
            bound[b.hook as usize].push(Bound {
                index,
                class_index,
                name,
                method_name: env.new_global_ref(method_name)?,
                class: class.clone(),
                binds: *binds,
                instance: instance.clone(),
                method,
                static_method,
                returnable: params.contains(&Param::Returnable),
                params,
                modifies,
                cancellable: b.cancellable,
                side: b.side,
                warned: AtomicBool::new(false),
            });
            index += 1;
        }
        Ok(Layer {
            vm,
            bound,
            ids,
            classes: classes.into_iter().map(|(k, (c, _, _))| (k, c)).collect(),
            event_warned: [const { AtomicBool::new(false) }; Event::ALL.len()],
            loader,
        })
    }

    /// Tells Fabric's loader facade about the game, then runs each entry point that runs here.
    fn run_entrypoints(
        &self,
        env: &mut JNIEnv,
        mods: &Mods,
        problems: &mut Vec<String>,
    ) -> jni::errors::Result<()> {
        // The mods that are on, and what the layer provides (the game, Fabric API's covered
        // modules).
        let mut loaded: Vec<&str> = mods.mod_ids.iter().map(String::as_str).collect();
        loaded.extend(fabric::PROVIDED);
        let ids: JObjectArray =
            env.new_object_array(loaded.len() as i32, "java/lang/String", JObject::null())?;
        for (i, id) in loaded.iter().enumerate() {
            let s = env.new_string(id)?;
            env.set_object_array_element(&ids, i as i32, s)?;
        }
        let dir = env.new_string(mods.game_dir.display().to_string())?;
        env.call_static_method(
            LOADER,
            "init",
            "(Ljava/lang/String;[Ljava/lang/String;Z)V",
            &[
                JValue::Object(&dir),
                JValue::Object(&ids),
                JValue::Bool((!mods.dedicated).into()),
            ],
        )?;
        env.call_static_method(GAME_EVENT, "init", "()V", &[])?;
        let layer_class = <&JClass>::from(
            LAYER_CLASS
                .get()
                .ok_or(jni::errors::Error::NullPtr("layer"))?
                .as_obj(),
        );
        // `main` entry points first, then the side's, as Fabric's loader runs them.
        let mut order: Vec<&Entrypoint> = mods
            .entrypoints
            .iter()
            .filter(|e| e.runs(mods.dedicated))
            .collect();
        order.sort_by_key(|e| e.kind != "main");
        for e in order {
            let spec = env.new_string(&e.spec)?;
            let kind = env.new_string(&e.kind)?;
            let failed = env
                .call_static_method(
                    layer_class,
                    "entrypoint",
                    "(Ljava/lang/ClassLoader;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(self.loader.as_obj()),
                        JValue::Object(&spec),
                        JValue::Object(&kind),
                    ],
                )?
                .l()?;
            if !failed.is_null() {
                let why: String = env.get_string(&failed.into())?.into();
                problems.push(format!(
                    "{} entry point {} ({}): {why}",
                    e.mod_id, e.spec, e.kind
                ));
            }
        }
        Ok(())
    }

    /// A mixin class the layer loaded (`dev/example/Mixin`): to read a mod's state.
    pub fn class(&self, name: &str) -> Option<GlobalRef> {
        self.classes.get(name).cloned()
    }

    /// The mods' class loader: to read a mod's state.
    pub fn loader(&self) -> &GlobalRef {
        &self.loader
    }

    /// How many handlers are bound.
    pub fn handlers(&self) -> usize {
        self.bound.iter().map(Vec::len).sum()
    }

    /// Makes the hook points call the bound handlers and fire Fabric API's events, for the rest
    /// of the process. False when a runtime is installed already (one Java layer per process).
    pub fn install(self) -> bool {
        let hooks: Vec<(Hook, usize)> = Hook::ALL
            .iter()
            .map(|h| (*h, self.bound[*h as usize].len()))
            .collect();
        if !hooks::install(Box::new(move |hook, canceled, slot, site, host| {
            self.run(hook, canceled, slot, site, host)
        })) {
            return false;
        }
        for (hook, n) in hooks {
            for _ in 0..n {
                hooks::attach(hook);
            }
        }
        true
    }

    /// Fires the Fabric API events of `hook`, then calls each handler bound to it, in binding
    /// order, with `host` reachable from the Java API layer.
    pub fn run(
        &self,
        hook: Hook,
        canceled: &mut bool,
        slot: &mut Slot,
        site: &Site,
        host: &mut dyn Host,
    ) {
        let Ok(mut env) = self.vm.attach_current_thread_as_daemon() else {
            return;
        };
        let side = host.side();
        let handlers = &self.bound[hook as usize];
        natives::scope(host, || {
            let capacity = 8 + 4 * (site.args.len() as i32 + 1) * (handlers.len() as i32 + 1);
            let _ = env.with_local_frame(capacity, |env| -> jni::errors::Result<()> {
                for event in Event::at(hook).filter(|e| natives::listened(*e)) {
                    if let Err(e) = self.fire(env, event, site) {
                        let why = describe(env, e);
                        if !self.event_warned[event as usize].swap(true, Ordering::Relaxed) {
                            log::warn!("Horizon: a Fabric API {event:?} listener failed: {why}");
                        }
                    }
                }
                if handlers.is_empty() {
                    return Ok(());
                }
                let mut args = Vec::with_capacity(site.args.len());
                for a in &site.args {
                    args.push(self.arg(env, a)?);
                }
                for b in handlers {
                    if b.side.is_some_and(|s| s != side) {
                        continue;
                    }
                    let result = match b.modifies {
                        Some(k) => self.modify(env, b, k, site, &args, slot),
                        None => self.call(env, b, kind(hook), site, &args, canceled, slot),
                    };
                    if let Err(e) = result {
                        let why = describe(env, e);
                        if !b.warned.swap(true, Ordering::Relaxed) {
                            log::warn!("Horizon: Java mod handler {} failed: {why}", b.name);
                        }
                    }
                }
                Ok(())
            });
        });
    }

    fn fire(&self, env: &mut JNIEnv, event: Event, site: &Site) -> jni::errors::Result<()> {
        let arg = self.arg(env, &event.argument(site))?;
        // SAFETY: `fire` is GameEvent's static (ILjava/lang/Object;)V.
        unsafe {
            env.call_static_method_unchecked(
                <&JClass>::from(self.ids.game_event.as_obj()),
                self.ids.fire,
                ReturnType::Primitive(Primitive::Void),
                &[JValue::Int(event as i32).as_jni(), arg],
            )?;
        }
        Ok(())
    }

    /// The Java object for a site's value (a local reference, or one of the layer's globals).
    fn arg(&self, env: &mut JNIEnv, o: &Obj) -> jni::errors::Result<jvalue> {
        let ids = &self.ids;
        let objects = <&JClass>::from(ids.objects.as_obj());
        let call = |env: &mut JNIEnv, id: JStaticMethodID, args: &[jvalue]| {
            // SAFETY: each id is an Objects factory looked up with the descriptor its call follows.
            unsafe { env.call_static_method_unchecked(objects, id, ReturnType::Object, args) }
                .and_then(|v| v.l())
                .map(|o| JValue::Object(&o).as_jni())
        };
        let int = |i: i32| JValue::Int(i).as_jni();
        Ok(match o {
            Obj::Null => JValue::Object(&JObject::null()).as_jni(),
            Obj::Server => JValue::Object(ids.server.as_obj()).as_jni(),
            Obj::True => JValue::Object(ids.always.as_obj()).as_jni(),
            Obj::Level(l) => call(env, ids.level, &[int(*l)])?,
            Obj::Entity(id) => call(env, ids.entity, &[int(*id)])?,
            Obj::BlockPos([x, y, z]) => call(env, ids.block_pos, &[int(*x), int(*y), int(*z)])?,
            Obj::BlockState(s) => call(env, ids.block_state, &[int(*s as i32)])?,
            Obj::Block(s) => call(env, ids.block, &[int(*s as i32)])?,
            Obj::Stack(item, count) => {
                let item = env.new_string(item)?;
                call(
                    env,
                    ids.stack,
                    &[JValue::Object(&item).as_jni(), int(*count)],
                )?
            }
            Obj::Hand(main) => call(env, ids.hand, &[JValue::Bool((*main).into()).as_jni()])?,
        })
    }

    /// The handler's `this`: the instance standing for the site's target (made once per target
    /// and thread), or the class's one instance; None for a static handler.
    fn this(
        &self,
        env: &mut JNIEnv,
        b: &Bound,
        site: &Site,
    ) -> jni::errors::Result<Option<GlobalRef>> {
        if b.static_method.is_some() {
            return Ok(None);
        }
        let Some(bind) = b.binds else {
            return Ok(b.instance.clone());
        };
        let key = (b.class_index, handle(site.this.as_ref()));
        if let Some(found) = TARGETS.with(|t| t.borrow().get(&key).cloned()) {
            return Ok(Some(found));
        }
        let object = env.alloc_object(<&JClass>::from(b.class.as_obj()))?;
        // SAFETY: `bind` is the API class's rm$bind (J)V, found on this class.
        unsafe {
            env.call_method_unchecked(
                &object,
                bind,
                ReturnType::Primitive(Primitive::Void),
                &[JValue::Long(key.1).as_jni()],
            )?;
        }
        let global = env.new_global_ref(object)?;
        TARGETS.with(|t| {
            let mut t = t.borrow_mut();
            if t.len() >= MAX_TARGETS {
                t.clear();
            }
            t.insert(key, global.clone());
        });
        Ok(Some(global))
    }

    /// The value in `slot` as Java has it in a `CallbackInfoReturnable`, or null.
    fn boxed<'l>(
        &self,
        env: &mut JNIEnv<'l>,
        kind: Kind,
        slot: Slot,
    ) -> jni::errors::Result<JObject<'l>> {
        let ids = &self.ids;
        // SAFETY: the ids are Float.valueOf(F), Boolean.valueOf(Z) and Objects.result(Z).
        unsafe {
            match (kind, slot) {
                (Kind::Float, Slot::Float(f)) => env
                    .call_static_method_unchecked(
                        <&JClass>::from(ids.float.0.as_obj()),
                        ids.float.1,
                        ReturnType::Object,
                        &[JValue::Float(f).as_jni()],
                    )?
                    .l(),
                (Kind::Bool, Slot::Bool(v)) => env
                    .call_static_method_unchecked(
                        <&JClass>::from(ids.boolean.0.as_obj()),
                        ids.boolean.1,
                        ReturnType::Object,
                        &[JValue::Bool(v.into()).as_jni()],
                    )?
                    .l(),
                (Kind::Result, Slot::Bool(v)) => env
                    .call_static_method_unchecked(
                        <&JClass>::from(ids.objects.as_obj()),
                        ids.result,
                        ReturnType::Object,
                        &[JValue::Bool(v.into()).as_jni()],
                    )?
                    .l(),
                _ => Ok(JObject::null()),
            }
        }
    }

    /// A value Java handed back, into `slot` (unchanged when it can't be read).
    fn unboxed(
        &self,
        env: &mut JNIEnv,
        kind: Kind,
        value: &JObject,
        slot: &mut Slot,
    ) -> jni::errors::Result<()> {
        if value.is_null() {
            return Ok(());
        }
        let ids = &self.ids;
        // SAFETY: floatValue ()F and booleanValue ()Z on objects checked to be a Float and a
        // Boolean; Objects.consumed (Object)Z.
        unsafe {
            match kind {
                Kind::Float
                    if env.is_instance_of(value, <&JClass>::from(ids.float.0.as_obj()))? =>
                {
                    *slot = Slot::Float(
                        env.call_method_unchecked(
                            value,
                            ids.float.2,
                            ReturnType::Primitive(Primitive::Float),
                            &[],
                        )?
                        .f()?,
                    );
                }
                Kind::Bool
                    if env.is_instance_of(value, <&JClass>::from(ids.boolean.0.as_obj()))? =>
                {
                    *slot = Slot::Bool(
                        env.call_method_unchecked(
                            value,
                            ids.boolean.2,
                            ReturnType::Primitive(Primitive::Boolean),
                            &[],
                        )?
                        .z()?,
                    );
                }
                Kind::Result => {
                    *slot = Slot::Bool(
                        env.call_static_method_unchecked(
                            <&JClass>::from(ids.objects.as_obj()),
                            ids.consumed,
                            ReturnType::Primitive(Primitive::Boolean),
                            &[JValue::Object(value).as_jni()],
                        )?
                        .z()?,
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// This handler's `CallbackInfo` on this thread, reset: not cancelled, holding `slot`.
    fn info(
        &self,
        env: &mut JNIEnv,
        b: &Bound,
        kind: Kind,
        slot: Slot,
    ) -> jni::errors::Result<GlobalRef> {
        let cached = INFOS.with(|i| i.borrow().get(b.index).cloned().flatten());
        let info = match cached {
            Some(info) => {
                env.set_field_unchecked(&info, self.ids.cancelled, JValue::Bool(0))?;
                info
            }
            None => {
                let id = b.method_name.as_obj();
                let made = if b.returnable {
                    // SAFETY: CallbackInfoReturnable's (String, boolean, Object) constructor.
                    unsafe {
                        env.new_object_unchecked(
                            <&JClass>::from(self.ids.returnable.0.as_obj()),
                            self.ids.returnable.1,
                            &[
                                JValue::Object(id).as_jni(),
                                JValue::Bool(b.cancellable.into()).as_jni(),
                                JValue::Object(&JObject::null()).as_jni(),
                            ],
                        )?
                    }
                } else {
                    // SAFETY: CallbackInfo's (String, boolean) constructor.
                    unsafe {
                        env.new_object_unchecked(
                            <&JClass>::from(self.ids.info.0.as_obj()),
                            self.ids.info.1,
                            &[
                                JValue::Object(id).as_jni(),
                                JValue::Bool(b.cancellable.into()).as_jni(),
                            ],
                        )?
                    }
                };
                let global = env.new_global_ref(made)?;
                INFOS.with(|i| {
                    let mut i = i.borrow_mut();
                    if i.len() <= b.index {
                        i.resize(b.index + 1, None);
                    }
                    i[b.index] = Some(global.clone());
                });
                global
            }
        };
        if b.returnable {
            let value = self.boxed(env, kind, slot)?;
            env.set_field_unchecked(&info, self.ids.return_value, JValue::Object(&value))?;
        }
        Ok(info)
    }

    /// The handler's arguments: the site's values in the target method's order, `info` for a
    /// `CallbackInfo`, zero for primitives. `first` replaces the first one (a modified value).
    fn values(
        b: &Bound,
        args: &[jvalue],
        info: Option<&GlobalRef>,
        first: Option<jvalue>,
    ) -> Vec<jvalue> {
        let mut site = args.iter();
        let mut out: Vec<jvalue> = Vec::with_capacity(b.params.len());
        let mut params = b.params.iter();
        if let Some(first) = first {
            params.next();
            out.push(first);
        }
        for p in params {
            out.push(match p {
                Param::Info | Param::Returnable => match info {
                    Some(i) => JValue::Object(i.as_obj()).as_jni(),
                    None => JValue::Object(&JObject::null()).as_jni(),
                },
                Param::Object => site
                    .next()
                    .copied()
                    .unwrap_or(JValue::Object(&JObject::null()).as_jni()),
                p => {
                    site.next();
                    match p {
                        Param::Bool => JValue::Bool(0).as_jni(),
                        Param::Long => JValue::Long(0).as_jni(),
                        Param::Float => JValue::Float(0.0).as_jni(),
                        Param::Double => JValue::Double(0.0).as_jni(),
                        _ => JValue::Int(0).as_jni(),
                    }
                }
            });
        }
        out
    }

    fn invoke<'l>(
        &self,
        env: &mut JNIEnv<'l>,
        b: &Bound,
        this: Option<&GlobalRef>,
        ret: ReturnType,
        values: &[jvalue],
    ) -> jni::errors::Result<jni::objects::JValueOwned<'l>> {
        // SAFETY: the ids were looked up with this handler's descriptor, `values` follows it,
        // and `ret` is its return type.
        let value = unsafe {
            match (this, b.method, b.static_method) {
                (Some(obj), Some(m), _) => env.call_method_unchecked(obj, m, ret, values)?,
                (_, _, Some(m)) => env.call_static_method_unchecked(
                    <&JClass>::from(b.class.as_obj()),
                    m,
                    ret,
                    values,
                )?,
                _ => return Err(jni::errors::Error::NullPtr("no instance")),
            }
        };
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &self,
        env: &mut JNIEnv,
        b: &Bound,
        kind: Kind,
        site: &Site,
        args: &[jvalue],
        canceled: &mut bool,
        slot: &mut Slot,
    ) -> jni::errors::Result<()> {
        let info = self.info(env, b, kind, *slot)?;
        let this = self.this(env, b, site)?;
        let values = Self::values(b, args, Some(&info), None);
        self.invoke(
            env,
            b,
            this.as_ref(),
            ReturnType::Primitive(Primitive::Void),
            &values,
        )?;
        let cancelled = env
            .get_field_unchecked(
                &info,
                self.ids.cancelled,
                ReturnType::Primitive(Primitive::Boolean),
            )?
            .z()?;
        if cancelled {
            *canceled = true;
            if b.returnable {
                let value = env
                    .get_field_unchecked(&info, self.ids.return_value, ReturnType::Object)?
                    .l()?;
                self.unboxed(env, kind, &value, slot)?;
            }
        }
        Ok(())
    }

    /// A `@ModifyReturnValue` handler: the return value in, the new one out.
    fn modify(
        &self,
        env: &mut JNIEnv,
        b: &Bound,
        kind: Kind,
        site: &Site,
        args: &[jvalue],
        slot: &mut Slot,
    ) -> jni::errors::Result<()> {
        let (first, ret) = match (kind, *slot) {
            (Kind::Float, Slot::Float(f)) => (
                JValue::Float(f).as_jni(),
                ReturnType::Primitive(Primitive::Float),
            ),
            (Kind::Bool, Slot::Bool(v)) => (
                JValue::Bool(v.into()).as_jni(),
                ReturnType::Primitive(Primitive::Boolean),
            ),
            (Kind::Result, Slot::Bool(_)) => {
                let boxed = self.boxed(env, kind, *slot)?;
                (JValue::Object(&boxed).as_jni(), ReturnType::Object)
            }
            _ => return Ok(()),
        };
        let this = self.this(env, b, site)?;
        let values = Self::values(b, args, None, Some(first));
        let value = self.invoke(env, b, this.as_ref(), ret, &values)?;
        match kind {
            Kind::Float => *slot = Slot::Float(value.f()?),
            Kind::Bool => *slot = Slot::Bool(value.z()?),
            _ => {
                let object = value.l()?;
                self.unboxed(env, kind, &object, slot)?;
            }
        }
        Ok(())
    }
}

/// Defines the mixin class `name` from the mods' loader: rebased onto its target when the API
/// layer has that class (with its `rm$bind`), else concrete with one instance made without a
/// constructor (None for an interface).
fn define(
    env: &mut JNIEnv,
    layer: &JClass,
    loader: &JObject,
    name: &str,
    target: &str,
) -> jni::errors::Result<(GlobalRef, Option<JMethodID>, Option<GlobalRef>)> {
    let jname = env.new_string(name)?;
    let bytes = env
        .call_static_method(
            layer,
            "classBytes",
            "(Ljava/lang/ClassLoader;Ljava/lang/String;)[B",
            &[JValue::Object(loader), JValue::Object(&jname)],
        )?
        .l()?;
    if bytes.is_null() {
        return Err(jni::errors::Error::NullPtr("the class isn't in the jars"));
    }
    let bytes = env.convert_byte_array(jni::objects::JByteArray::from(bytes))?;
    let onto = rebase_onto(target);
    if api_has(onto)
        && let Some(rebased) = classfile::rebase(&bytes, onto)
    {
        let class = env.define_class(name, loader, &rebased)?;
        let binds = env.get_method_id(&class, "rm$bind", "(J)V");
        if binds.is_err() {
            let _ = env.exception_clear();
        }
        return Ok((env.new_global_ref(class)?, binds.ok(), None));
    }
    let concrete = classfile::concrete(&bytes);
    let class = env.define_class(name, loader, concrete.as_deref().unwrap_or(&bytes))?;
    let instance = match concrete {
        Some(_) => {
            let object = env.alloc_object(&class)?;
            Some(env.new_global_ref(object)?)
        }
        None => None,
    };
    Ok((env.new_global_ref(class)?, None, instance))
}

/// Nanoseconds per call of an empty static Java method from Rust, over `n` calls.
pub fn time_rust_to_java(n: u32) -> Result<f64, String> {
    let vm = vm().ok_or("no JVM")?;
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| e.to_string())?;
    let class = <&JClass>::from(LAYER_CLASS.get().ok_or("no layer")?.as_obj());
    let id = env
        .get_static_method_id(class, "empty", "()V")
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    for _ in 0..n {
        // SAFETY: `empty` is ()V.
        unsafe {
            env.call_static_method_unchecked(
                class,
                id,
                ReturnType::Primitive(Primitive::Void),
                &[],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(start.elapsed().as_secs_f64() * 1e9 / f64::from(n))
}

/// Nanoseconds per call of an empty Rust function from Java (`Layer.rust`), over `n` calls.
pub fn time_java_to_rust(n: i32) -> Result<f64, String> {
    let vm = vm().ok_or("no JVM")?;
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| e.to_string())?;
    let class = <&JClass>::from(LAYER_CLASS.get().ok_or("no layer")?.as_obj());
    let ns = env
        .call_static_method(class, "callRust", "(I)J", &[JValue::Int(n)])
        .and_then(|v| v.j())
        .map_err(|e| e.to_string())?;
    Ok(ns as f64 / f64::from(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_handler_descriptors() {
        use Param::*;
        assert_eq!(
            params(
                "(Ljava/util/function/BooleanSupplier;Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V"
            ),
            Some(vec![Object, Info])
        );
        assert_eq!(
            params(
                "(I[[Ljava/lang/String;[JFZLorg/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable;)V"
            ),
            Some(vec![Int, Object, Object, Float, Bool, Returnable])
        );
        assert_eq!(params("()V"), Some(vec![]));
        assert_eq!(params("(Q)V"), None);
        assert_eq!(params("V"), None);
    }

    #[test]
    fn the_api_layer_is_defined_superclasses_first() {
        let order = defining_order();
        assert_eq!(order.len(), CLASSES.len());
        let at = |n: &str| order.iter().position(|(c, _)| *c == n).unwrap();
        assert!(at("net/minecraft/core/Vec3i") < at("net/minecraft/core/BlockPos"));
        assert!(
            at("net/minecraft/world/entity/Entity") < at("net/minecraft/server/level/ServerPlayer")
        );
        assert!(at("net/minecraft/core/Registry") < at("net/minecraft/core/DefaultedRegistry"));
        assert!(api_has("net/minecraft/server/MinecraftServer"));
        assert!(!api_has("dev/rustyminecraft/java/Host"));
        assert!(!api_has("net/minecraft/world/level/block/FireBlock"));
    }
}
