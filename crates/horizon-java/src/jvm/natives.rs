//! The Java API layer's calls into Rust: `dev.rustyminecraft.java.Host`'s native methods (answered
//! by the game's [`Host`] while a hook point runs on this thread), a mod's registrations
//! (`GameRegistry.rust`), and a Fabric API event's first listener (`GameEvent.listen`).
//!
//! The host is reachable only inside [`scope`], on the thread running the hook point: a native
//! called anywhere else (a mod's entry point at start, a thread of the mod's own) finds none and
//! answers nothing.
use crate::{
    fabric::{Event, Registration},
    hooks::{self, EntityView, Host, Side},
};
use jni::{
    JNIEnv, NativeMethod,
    objects::{JClass, JString},
    sys::{jboolean, jdouble, jfloat, jint, jintArray, jlong, jstring},
};
use std::{
    cell::Cell,
    ffi::c_void,
    ptr::{NonNull, null_mut},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

thread_local! {
    /// The host of the hook point running on this thread, if any.
    static HOST: Cell<Option<NonNull<dyn Host + 'static>>> = const { Cell::new(None) };
}

/// Puts the previous host back when a scope (or a use) ends, even by a panic.
struct Restore(Option<NonNull<dyn Host + 'static>>);

impl Drop for Restore {
    fn drop(&mut self) {
        HOST.with(|h| h.set(self.0));
    }
}

/// Runs `f` with `host` reachable from the natives on this thread.
pub(super) fn scope<R>(host: &mut dyn Host, f: impl FnOnce() -> R) -> R {
    let ptr: NonNull<dyn Host + '_> = NonNull::from(host);
    // SAFETY: only the trait object's lifetime is erased. The pointer is reachable only while `f`
    // runs (`Restore` takes it away again), and `host` stays mutably borrowed by this call until
    // then, so nothing else reaches the host meanwhile.
    let ptr: NonNull<dyn Host + 'static> = unsafe { std::mem::transmute(ptr) };
    let previous = HOST.with(|h| h.replace(Some(ptr)));
    let _restore = Restore(previous);
    f()
}

/// `f` with this thread's host, or None outside a hook point.
fn with<R>(f: impl FnOnce(&mut dyn Host) -> R) -> Option<R> {
    let ptr = HOST.with(|h| h.take())?;
    let _restore = Restore(Some(ptr));
    // SAFETY: set by `scope` on this thread, whose borrow of the host outlives this call; it is
    // taken out while in use, so a native reached again from inside `f` finds none instead of a
    // second mutable reference.
    Some(f(unsafe { &mut *ptr.as_ptr() }))
}

fn entity(id: jint) -> Option<EntityView> {
    with(|h| h.entity(id)).flatten()
}

fn string(env: &JNIEnv, s: Option<String>) -> jstring {
    match s {
        Some(s) if !s.is_empty() => env.new_string(s).map_or(null_mut(), |s| s.into_raw()),
        _ => null_mut(),
    }
}

fn ints(env: &JNIEnv, values: &[i32]) -> jintArray {
    let Ok(array) = env.new_int_array(values.len() as i32) else {
        return null_mut();
    };
    if env.set_int_array_region(&array, 0, values).is_err() {
        return null_mut();
    }
    array.into_raw()
}

fn read(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(String::from).unwrap_or_default()
}

extern "system" fn in_game(_: JNIEnv, _: JClass) -> jboolean {
    HOST.with(|h| h.get().is_some()).into()
}

extern "system" fn is_client(_: JNIEnv, _: JClass) -> jboolean {
    with(|h| h.side() == Side::Client).unwrap_or(false).into()
}

extern "system" fn tick_count(_: JNIEnv, _: JClass) -> jint {
    with(|h| h.tick_count()).unwrap_or(0)
}

extern "system" fn levels(env: JNIEnv, _: JClass) -> jintArray {
    ints(&env, &with(|h| h.levels()).unwrap_or_default())
}

extern "system" fn current_level(_: JNIEnv, _: JClass) -> jint {
    with(|h| h.current_level()).unwrap_or(0)
}

extern "system" fn level_name(env: JNIEnv, _: JClass, level: jint) -> jstring {
    string(&env, with(|h| h.level_name(level)))
}

extern "system" fn level_time(_: JNIEnv, _: JClass, level: jint, day: jboolean) -> jlong {
    with(|h| h.level_time(level)[usize::from(day != 0)]).unwrap_or(0)
}

extern "system" fn block_state(
    _: JNIEnv,
    _: JClass,
    level: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    with(|h| h.block_state(level, [x, y, z]))
        .flatten()
        .and_then(|s| i32::try_from(s).ok())
        .unwrap_or(-1)
}

extern "system" fn set_block_state(
    _: JNIEnv,
    _: JClass,
    level: jint,
    x: jint,
    y: jint,
    z: jint,
    state: jint,
) -> jboolean {
    let Ok(state) = u32::try_from(state) else {
        return 0;
    };
    with(|h| h.set_block_state(level, [x, y, z], state))
        .unwrap_or(false)
        .into()
}

extern "system" fn state_block(env: JNIEnv, _: JClass, state: jint) -> jstring {
    let Ok(state) = u32::try_from(state) else {
        return null_mut();
    };
    string(&env, with(|h| h.state_block(state)))
}

extern "system" fn default_state(mut env: JNIEnv, _: JClass, block: JString) -> jint {
    let block = read(&mut env, &block);
    with(|h| h.default_state(&block))
        .flatten()
        .and_then(|s| i32::try_from(s).ok())
        .unwrap_or(-1)
}

extern "system" fn players(env: JNIEnv, _: JClass) -> jintArray {
    ints(&env, &with(|h| h.players()).unwrap_or_default())
}

extern "system" fn entity_kind(env: JNIEnv, _: JClass, id: jint) -> jstring {
    string(&env, entity(id).map(|e| e.kind))
}

extern "system" fn entity_level(_: JNIEnv, _: JClass, id: jint) -> jint {
    entity(id).map_or(0, |e| e.level)
}

extern "system" fn entity_pos(_: JNIEnv, _: JClass, id: jint, axis: jint) -> jdouble {
    let axis = usize::try_from(axis).unwrap_or(0).min(2);
    entity(id).map_or(0.0, |e| e.pos[axis])
}

extern "system" fn entity_health(_: JNIEnv, _: JClass, id: jint, max: jboolean) -> jfloat {
    entity(id)
        .and_then(|e| e.health)
        .map_or(f32::NAN, |h| h[usize::from(max != 0)])
}

extern "system" fn entity_name(env: JNIEnv, _: JClass, id: jint) -> jstring {
    string(&env, entity(id).map(|e| e.name))
}

extern "system" fn entity_flags(_: JNIEnv, _: JClass, id: jint) -> jint {
    entity(id).map_or(0, |e| {
        i32::from(e.alive) | i32::from(e.player) << 1 | i32::from(e.creative) << 2
    })
}

extern "system" fn set_health(_: JNIEnv, _: JClass, id: jint, health: jfloat) -> jboolean {
    with(|h| h.set_health(id, health)).unwrap_or(false).into()
}

extern "system" fn send_message(
    mut env: JNIEnv,
    _: JClass,
    player: jint,
    text: JString,
    overlay: jboolean,
) {
    let text = read(&mut env, &text);
    with(|h| h.send_message(player, &text, overlay != 0));
}

extern "system" fn broadcast(mut env: JNIEnv, _: JClass, text: JString) {
    let text = read(&mut env, &text);
    with(|h| h.broadcast(&text));
}

/// The registrations mods made, in order, until [`take_registrations`].
static REGISTRATIONS: Mutex<Vec<Registration>> = Mutex::new(Vec::new());

extern "system" fn registered(
    mut env: JNIEnv,
    _: JClass,
    registry: JString,
    id: JString,
    description: JString,
) {
    let registration = Registration::new(
        &read(&mut env, &registry),
        &read(&mut env, &id),
        &read(&mut env, &description),
    );
    if let Ok(mut all) = REGISTRATIONS.lock() {
        all.push(registration);
    }
}

/// The blocks and items mods registered from Java since the last call.
pub fn take_registrations() -> Vec<Registration> {
    REGISTRATIONS
        .lock()
        .map(|mut all| std::mem::take(&mut *all))
        .unwrap_or_default()
}

/// Which Fabric API events have a listener.
static LISTENED: [AtomicBool; Event::ALL.len()] =
    [const { AtomicBool::new(false) }; Event::ALL.len()];

pub(super) fn listened(event: Event) -> bool {
    LISTENED[event as usize].load(Ordering::Relaxed)
}

extern "system" fn listen(_: JNIEnv, _: JClass, event: jint) {
    if let Some(e) = Event::from_index(event)
        && !LISTENED[e as usize].swap(true, Ordering::Relaxed)
    {
        hooks::attach(e.hook());
    }
}

fn native(name: &str, sig: &str, f: *mut c_void) -> NativeMethod {
    NativeMethod {
        name: name.into(),
        sig: sig.into(),
        fn_ptr: f,
    }
}

/// Registers the natives on the layer's classes.
pub(super) fn register(env: &mut JNIEnv) -> jni::errors::Result<()> {
    let host = env.find_class("dev/rustyminecraft/java/Host")?;
    env.register_native_methods(
        &host,
        &[
            native("inGame", "()Z", in_game as *mut c_void),
            native("isClient", "()Z", is_client as *mut c_void),
            native("tickCount", "()I", tick_count as *mut c_void),
            native("levels", "()[I", levels as *mut c_void),
            native("currentLevel", "()I", current_level as *mut c_void),
            native(
                "levelName",
                "(I)Ljava/lang/String;",
                level_name as *mut c_void,
            ),
            native("levelTime", "(IZ)J", level_time as *mut c_void),
            native("blockState", "(IIII)I", block_state as *mut c_void),
            native("setBlockState", "(IIIII)Z", set_block_state as *mut c_void),
            native(
                "stateBlock",
                "(I)Ljava/lang/String;",
                state_block as *mut c_void,
            ),
            native(
                "defaultState",
                "(Ljava/lang/String;)I",
                default_state as *mut c_void,
            ),
            native("players", "()[I", players as *mut c_void),
            native(
                "entityKind",
                "(I)Ljava/lang/String;",
                entity_kind as *mut c_void,
            ),
            native("entityLevel", "(I)I", entity_level as *mut c_void),
            native("entityPos", "(II)D", entity_pos as *mut c_void),
            native("entityHealth", "(IZ)F", entity_health as *mut c_void),
            native(
                "entityName",
                "(I)Ljava/lang/String;",
                entity_name as *mut c_void,
            ),
            native("entityFlags", "(I)I", entity_flags as *mut c_void),
            native("setHealth", "(IF)Z", set_health as *mut c_void),
            native(
                "sendMessage",
                "(ILjava/lang/String;Z)V",
                send_message as *mut c_void,
            ),
            native(
                "broadcast",
                "(Ljava/lang/String;)V",
                broadcast as *mut c_void,
            ),
        ],
    )?;
    let registry = env.find_class("dev/rustyminecraft/java/GameRegistry")?;
    env.register_native_methods(
        &registry,
        &[native(
            "rust",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            registered as *mut c_void,
        )],
    )?;
    let events = env.find_class("dev/rustyminecraft/java/fabric/GameEvent")?;
    env.register_native_methods(&events, &[native("listen", "(I)V", listen as *mut c_void)])
}
