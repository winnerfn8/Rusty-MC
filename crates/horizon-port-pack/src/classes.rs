//! What a mod's Java code does, read from its class files without running
//! or decompiling them: each class's superclass, interfaces, declared
//! methods and the classes and annotations its constant pool names. The
//! mod's own classes are followed up to the first game class they extend,
//! and that class says what the code is: a block entity, a menu, an entity,
//! a block or item with behaviour of its own, a worldgen feature...
//!
//! Since 26.1 the game ships unobfuscated, and NeoForge has run on Mojang's
//! names since 1.20.5, so game classes and methods are read by name. Fabric
//! jars before 26.1 name them in intermediary (`net/minecraft/class_2248`)
//! and Forge's methods are SRG (`m_60503_`): a class extending such a game
//! class is counted as code of its own, whatever it overrides.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One class file, as much as this module reads.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Class {
    pub name: String,
    pub superclass: String,
    pub interfaces: Vec<String>,
    /// Declared methods, by name (constructors and lambdas left out).
    pub methods: Vec<String>,
    /// Every class the constant pool names (`net/minecraft/...`).
    pub refs: BTreeSet<String>,
    /// Annotation and descriptor strings (`Lorg/spongepowered/asm/mixin/Mixin;`).
    pub mixin: bool,
    /// The fields and methods its code uses: (class, name, descriptor).
    pub members: BTreeSet<(String, String, String)>,
    /// The Java version its class file needs (major 69 is Java 25).
    pub java: u32,
}

/// Reads a class file's header, constant pool and declared methods.
pub fn parse(bytes: &[u8]) -> Option<Class> {
    let mut r = Reader { bytes, at: 0 };
    if r.u4()? != 0xCAFE_BABE {
        return None;
    }
    r.skip(2)?; // minor
    let java = u32::from(r.u2()?).saturating_sub(44);
    let count = r.u2()? as usize;
    let mut utf8: HashMap<usize, String> = HashMap::new();
    let mut class_of: HashMap<usize, usize> = HashMap::new();
    // Field and method references: (class entry, name-and-type entry); name-and-type entries.
    let mut member_of: Vec<(usize, usize)> = Vec::new();
    let mut name_and_type: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut i = 1;
    while i < count {
        let tag = r.u1()?;
        match tag {
            1 => {
                let len = r.u2()? as usize;
                let s = r.take(len)?;
                utf8.insert(i, String::from_utf8_lossy(s).into_owned());
            }
            7 => {
                class_of.insert(i, r.u2()? as usize);
            }
            9..=11 => member_of.push((r.u2()? as usize, r.u2()? as usize)),
            12 => {
                name_and_type.insert(i, (r.u2()? as usize, r.u2()? as usize));
            }
            8 | 16 | 19 | 20 => r.skip(2)?,
            3 | 4 | 17 | 18 => r.skip(4)?,
            15 => r.skip(3)?,
            5 | 6 => {
                r.skip(8)?;
                i += 1;
            }
            _ => return None,
        }
        i += 1;
    }
    let class_name = |idx: usize| -> String {
        class_of
            .get(&idx)
            .and_then(|u| utf8.get(u))
            .cloned()
            .unwrap_or_default()
    };
    r.skip(2)?; // access flags
    let name = class_name(r.u2()? as usize);
    let superclass = class_name(r.u2()? as usize);
    let n = r.u2()? as usize;
    let mut interfaces = Vec::with_capacity(n);
    for _ in 0..n {
        interfaces.push(class_name(r.u2()? as usize));
    }
    // Fields, then methods: flags, name, descriptor, attributes.
    let mut methods = Vec::new();
    for member in 0..2 {
        let n = r.u2()?;
        for _ in 0..n {
            r.skip(2)?;
            let name = r.u2()? as usize;
            r.skip(2)?;
            let attributes = r.u2()?;
            for _ in 0..attributes {
                r.skip(2)?;
                let len = r.u4()? as usize;
                r.skip(len)?;
            }
            if member == 1
                && let Some(m) = utf8.get(&name)
                && !m.starts_with('<')
                && !m.starts_with("lambda$")
            {
                methods.push(m.clone());
            }
        }
    }
    let refs = class_of
        .values()
        .filter_map(|u| utf8.get(u))
        .filter(|s| !s.starts_with('['))
        .cloned()
        .collect();
    let mixin = utf8
        .values()
        .any(|s| s == "Lorg/spongepowered/asm/mixin/Mixin;");
    let members = member_of
        .iter()
        .filter_map(|(class, nat)| {
            let (n, d) = name_and_type.get(nat)?;
            Some((
                class_name(*class),
                utf8.get(n)?.clone(),
                utf8.get(d)?.clone(),
            ))
        })
        .filter(|(c, _, _)| !c.is_empty() && !c.starts_with('['))
        .collect();
    Some(Class {
        name,
        superclass,
        interfaces,
        methods,
        refs,
        mixin,
        members,
        java,
    })
}

/// An annotation as a class file keeps it: its type and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The type's descriptor: `Lorg/spongepowered/asm/mixin/Mixin;`.
    pub kind: String,
    pub values: Vec<(String, Value)>,
}

/// An annotation's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    /// Every integral constant, booleans and chars included.
    Int(i64),
    Float(f64),
    /// A class literal, as a descriptor: `Lnet/minecraft/server/MinecraftServer;`.
    Class(String),
    /// An enum constant: its type's descriptor and its name.
    Enum(String, String),
    Annotation(Annotation),
    Array(Vec<Value>),
}

impl Annotation {
    /// The type's simple name: `Mixin`, `Inject`, `At`.
    pub fn name(&self) -> &str {
        let k = self.kind.trim_start_matches('L').trim_end_matches(';');
        k.rsplit(['/', '$']).next().unwrap_or(k)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// A value's strings, whether it holds one or an array of them.
    pub fn strings(&self, key: &str) -> Vec<&str> {
        self.get(key).map(Value::strings).unwrap_or_default()
    }

    /// A boolean value (the class file keeps it as an integer).
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Int(n)) if *n != 0)
    }
}

impl Value {
    /// The strings of a string or an array of them (anything else gives none).
    pub fn strings(&self) -> Vec<&str> {
        match self {
            Value::Str(s) => vec![s.as_str()],
            Value::Array(v) => v.iter().flat_map(Value::strings).collect(),
            _ => Vec::new(),
        }
    }

    /// The internal names of a class literal or an array of them.
    pub fn classes(&self) -> Vec<&str> {
        match self {
            Value::Class(d) => vec![d.trim_start_matches('L').trim_end_matches(';')],
            Value::Array(v) => v.iter().flat_map(Value::classes).collect(),
            _ => Vec::new(),
        }
    }

    /// The annotations of a nested annotation or an array of them.
    pub fn annotations(&self) -> Vec<&Annotation> {
        match self {
            Value::Annotation(a) => vec![a],
            Value::Array(v) => v.iter().flat_map(Value::annotations).collect(),
            _ => Vec::new(),
        }
    }
}

/// A field or method with its annotations.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Member {
    pub name: String,
    pub descriptor: String,
    pub access: u16,
    pub annotations: Vec<Annotation>,
}

impl Member {
    pub fn is_static(&self) -> bool {
        self.access & 0x0008 != 0
    }
}

/// A class with the annotations of the class, its fields and its methods:
/// what a mixin says about where it applies.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Annotated {
    pub name: String,
    pub access: u16,
    pub annotations: Vec<Annotation>,
    pub fields: Vec<Member>,
    pub methods: Vec<Member>,
}

impl Annotated {
    pub fn is_abstract(&self) -> bool {
        self.access & (0x0400 | 0x0200) != 0
    }
}

/// The constant pool entries annotations point at.
#[derive(Default)]
struct Pool {
    utf8: HashMap<usize, String>,
    ints: HashMap<usize, i64>,
    floats: HashMap<usize, f64>,
    class_of: HashMap<usize, usize>,
}

impl Pool {
    fn utf(&self, i: u16) -> String {
        self.utf8.get(&(i as usize)).cloned().unwrap_or_default()
    }
    fn class(&self, i: u16) -> String {
        self.class_of
            .get(&(i as usize))
            .and_then(|u| self.utf8.get(u))
            .cloned()
            .unwrap_or_default()
    }
}

/// Reads a class file with its annotations, visible and invisible (mixin
/// annotations are kept in the class file only).
pub fn parse_annotated(bytes: &[u8]) -> Option<Annotated> {
    let mut r = Reader { bytes, at: 0 };
    if r.u4()? != 0xCAFE_BABE {
        return None;
    }
    r.skip(4)?;
    let count = r.u2()? as usize;
    let mut pool = Pool::default();
    let mut i = 1;
    while i < count {
        match r.u1()? {
            1 => {
                let len = r.u2()? as usize;
                let s = r.take(len)?;
                pool.utf8.insert(i, String::from_utf8_lossy(s).into_owned());
            }
            3 => {
                pool.ints.insert(i, i64::from(r.u4()? as i32));
            }
            4 => {
                pool.floats.insert(i, f64::from(f32::from_bits(r.u4()?)));
            }
            5 => {
                let hi = u64::from(r.u4()?);
                pool.ints
                    .insert(i, ((hi << 32) | u64::from(r.u4()?)) as i64);
                i += 1;
            }
            6 => {
                let hi = u64::from(r.u4()?);
                pool.floats
                    .insert(i, f64::from_bits((hi << 32) | u64::from(r.u4()?)));
                i += 1;
            }
            7 => {
                pool.class_of.insert(i, r.u2()? as usize);
            }
            8 | 16 | 19 | 20 => r.skip(2)?,
            9..=12 | 17 | 18 => r.skip(4)?,
            15 => r.skip(3)?,
            _ => return None,
        }
        i += 1;
    }
    let access = r.u2()?;
    let name = pool.class(r.u2()?);
    r.skip(2)?; // superclass
    let n = r.u2()? as usize;
    r.skip(2 * n)?;
    let mut members = [Vec::new(), Vec::new()];
    for list in &mut members {
        for _ in 0..r.u2()? {
            let access = r.u2()?;
            let name = pool.utf(r.u2()?);
            let descriptor = pool.utf(r.u2()?);
            let annotations = attributes(&mut r, &pool)?;
            list.push(Member {
                name,
                descriptor,
                access,
                annotations,
            });
        }
    }
    let annotations = attributes(&mut r, &pool)?;
    let [fields, methods] = members;
    Some(Annotated {
        name,
        access,
        annotations,
        fields,
        methods,
    })
}

/// A member's or class's attributes, keeping only the annotations.
fn attributes(r: &mut Reader, pool: &Pool) -> Option<Vec<Annotation>> {
    let mut out = Vec::new();
    for _ in 0..r.u2()? {
        let name = pool.utf(r.u2()?);
        let len = r.u4()? as usize;
        let body = r.take(len)?;
        if name == "RuntimeInvisibleAnnotations" || name == "RuntimeVisibleAnnotations" {
            let mut a = Reader { bytes: body, at: 0 };
            for _ in 0..a.u2()? {
                out.push(annotation(&mut a, pool, 0)?);
            }
        }
    }
    Some(out)
}

fn annotation(r: &mut Reader, pool: &Pool, depth: usize) -> Option<Annotation> {
    if depth > 16 {
        return None;
    }
    let kind = pool.utf(r.u2()?);
    let mut values = Vec::new();
    for _ in 0..r.u2()? {
        let key = pool.utf(r.u2()?);
        values.push((key, element(r, pool, depth)?));
    }
    Some(Annotation { kind, values })
}

fn element(r: &mut Reader, pool: &Pool, depth: usize) -> Option<Value> {
    Some(match r.u1()? {
        b'B' | b'C' | b'I' | b'J' | b'S' | b'Z' => {
            Value::Int(pool.ints.get(&(r.u2()? as usize)).copied().unwrap_or(0))
        }
        b'D' | b'F' => Value::Float(pool.floats.get(&(r.u2()? as usize)).copied().unwrap_or(0.0)),
        b's' => Value::Str(pool.utf(r.u2()?)),
        b'e' => {
            let kind = pool.utf(r.u2()?);
            Value::Enum(kind, pool.utf(r.u2()?))
        }
        b'c' => Value::Class(pool.utf(r.u2()?)),
        b'@' => Value::Annotation(annotation(r, pool, depth + 1)?),
        b'[' => {
            let n = r.u2()?;
            let mut v = Vec::with_capacity(n.min(256) as usize);
            for _ in 0..n {
                v.push(element(r, pool, depth + 1)?);
            }
            Value::Array(v)
        }
        _ => return None,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.bytes.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(drop)
    }
    fn u1(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u2(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u4(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// What a class does, as the port sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    /// A block with behaviour of its own (use, ticks, collisions...).
    BlockBehaviour,
    /// A block with a shape of its own: it ports with its template's.
    BlockShape,
    /// An item with behaviour of its own (use, on hit...).
    ItemBehaviour,
    BlockEntity,
    Menu,
    Entity,
    Screen,
    Renderer,
    /// A worldgen feature, placement or structure type written in Java.
    Worldgen,
    /// A recipe type (its recipes don't craft) or ingredient type.
    Recipe,
    Effect,
    Enchantment,
    Particle,
    Command,
    Network,
    /// Reacts to game events (NeoForge's bus, Fabric's callbacks).
    Events,
    /// A Mixin: a patch to the game's own code.
    Mixin,
    /// Extends a game class under obfuscated names (Fabric before 26.1,
    /// Forge's methods): what it does can't be read.
    Unread,
}

impl Feature {
    /// How the coverage line names `n` of them.
    pub fn describe(self, n: usize) -> String {
        let (one, many) = match self {
            Feature::BlockBehaviour => (
                "block with behaviour of its own (use, ticks, collisions)",
                "blocks with behaviour of their own (use, ticks, collisions)",
            ),
            Feature::BlockShape => (
                "block with a shape of its own: it ports with its template's",
                "blocks with shapes of their own: they port with their templates'",
            ),
            Feature::ItemBehaviour => (
                "item with behaviour of its own (use, on hit)",
                "items with behaviour of their own (use, on hit)",
            ),
            Feature::BlockEntity => (
                "block entity (machine, container or animated block)",
                "block entities (machines, containers, animated blocks)",
            ),
            Feature::Menu => ("menu (a container screen)", "menus (container screens)"),
            Feature::Entity => ("mob or entity", "mobs or entities"),
            Feature::Screen => ("screen of its own", "screens of its own"),
            Feature::Renderer => ("renderer or model in code", "renderers or models in code"),
            Feature::Worldgen => (
                "worldgen feature or structure type in code",
                "worldgen features or structure types in code",
            ),
            Feature::Recipe => (
                "recipe type of its own (its recipes don't craft)",
                "recipe types of its own (their recipes don't craft)",
            ),
            Feature::Effect => ("status effect", "status effects"),
            Feature::Enchantment => ("enchantment effect in code", "enchantment effects in code"),
            Feature::Particle => ("particle type", "particle types"),
            Feature::Command => ("command", "commands"),
            Feature::Network => ("network message", "network messages"),
            Feature::Events => ("game event it reacts to", "game events it reacts to"),
            Feature::Mixin => (
                "Mixin (a patch to the game's code)",
                "Mixins (patches to the game's code)",
            ),
            Feature::Unread => (
                "class extending the game under obfuscated names",
                "classes extending the game under obfuscated names",
            ),
        };
        format!("{n} {}", if n == 1 { one } else { many })
    }
}

/// Block methods that only say how it looks or is placed: the template's
/// do the same for the states a blockstate file lists.
const BLOCK_LOOKS: [&str; 18] = [
    "codec",
    "createBlockStateDefinition",
    "getStateForPlacement",
    "rotate",
    "mirror",
    "getRenderShape",
    "skipRendering",
    "getShadeBrightness",
    "propagatesSkylightDown",
    "getCloneItemStack",
    "isPathfindable",
    "getFluidState",
    "updateShape",
    "canBeReplaced",
    "getLightBlock",
    "useShapeForLightOcclusion",
    "getDescriptionId",
    "animateTick",
];
/// Block methods that only change its outline or collision.
const BLOCK_SHAPES: [&str; 6] = [
    "getShape",
    "getCollisionShape",
    "getVisualShape",
    "getOcclusionShape",
    "getInteractionShape",
    "getBlockSupportShape",
];
/// Item methods that only say how it looks or is named.
const ITEM_LOOKS: [&str; 6] = [
    "appendHoverText",
    "getName",
    "isFoil",
    "getDescriptionId",
    "getBarColor",
    "getBarWidth",
];

/// The game's own, loader and library classes: the first of these a mod's
/// class chain reaches is what it extends.
fn external(name: &str, own: &BTreeMap<String, Class>) -> bool {
    !own.contains_key(name)
}

fn obfuscated_method(m: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    m.strip_prefix("method_").is_some_and(digits)
        || m.strip_prefix("m_")
            .and_then(|s| s.strip_suffix('_'))
            .is_some_and(digits)
}

/// What a game class (Mojang's names) is, by its package and name.
fn kind_of_game_class(name: &str) -> Option<Feature> {
    let rest = name.strip_prefix("net/minecraft/")?;
    if rest.starts_with("class_") {
        return Some(Feature::Unread);
    }
    let (package, simple) = rest.rsplit_once('/').unwrap_or(("", rest));
    Some(match package {
        "world/level/block/entity" => Feature::BlockEntity,
        p if p.starts_with("world/level/block") => Feature::BlockBehaviour,
        "world/inventory" if simple.ends_with("Menu") => Feature::Menu,
        p if p.starts_with("world/entity") => Feature::Entity,
        p if p.starts_with("world/item/crafting") => Feature::Recipe,
        p if p.starts_with("world/item/enchantment") => Feature::Enchantment,
        p if p.starts_with("world/item") => Feature::ItemBehaviour,
        p if p.starts_with("world/level/levelgen") => Feature::Worldgen,
        "world/effect" => Feature::Effect,
        p if p.starts_with("client/gui/screens") || p.starts_with("client/gui/components") => {
            Feature::Screen
        }
        p if p.starts_with("client/renderer") || p.starts_with("client/model") => Feature::Renderer,
        p if p.starts_with("client/particle") || p.starts_with("core/particles") => {
            Feature::Particle
        }
        _ => return None,
    })
}

/// Events a content mod uses to register what it has: not behaviour.
fn registration_event(name: &str) -> bool {
    [
        "BuildCreativeModeTabContentsEvent",
        "RegisterEvent",
        "Register",
        "Lifecycle",
        "ModConfigEvent",
        "AddPackFindersEvent",
        "ItemGroupEvents",
        "ClientLifecycleEvents",
        "ServerLifecycleEvents",
    ]
    .iter()
    .any(|k| name.rsplit('/').next().unwrap_or(name).contains(k))
        || name.contains("/lifecycle/")
        || name.contains("/registries/")
        || name.contains("/registry/")
}

/// The code census: how many of each feature, and examples.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Code {
    pub classes: usize,
    pub features: BTreeMap<Feature, usize>,
    /// The game events it subscribes to, by simple name.
    pub events: BTreeSet<String>,
    /// Blocks and items with behaviour, and the methods they override.
    pub examples: BTreeMap<String, Vec<String>>,
}

impl Code {
    /// Whether it does anything beyond registering blocks and items.
    pub fn behaviour(&self) -> bool {
        self.features
            .keys()
            .any(|f| *f != Feature::BlockShape && *f != Feature::Particle)
    }
}

/// Reads every class of a jar. `classes` are the class files' bytes.
pub fn census<'a>(classes: impl IntoIterator<Item = &'a [u8]>) -> Code {
    let own: BTreeMap<String, Class> = classes
        .into_iter()
        .filter_map(parse)
        .map(|c| (c.name.clone(), c))
        .collect();
    let mut code = Code {
        classes: own.len(),
        ..Code::default()
    };
    let mut count = |f: Feature| *code.features.entry(f).or_default() += 1;
    for c in own.values() {
        if c.mixin {
            count(Feature::Mixin);
            continue;
        }
        // The first class outside the jar the chain reaches, and the
        // methods declared along the way.
        let mut methods: BTreeSet<&str> = c.methods.iter().map(String::as_str).collect();
        let mut at = c;
        let mut seen = 0;
        while !external(&at.superclass, &own) && seen < 64 {
            at = &own[&at.superclass];
            methods.extend(at.methods.iter().map(String::as_str));
            seen += 1;
        }
        let base = at.superclass.as_str();
        let kind = kind_of_game_class(base).or_else(|| {
            c.interfaces.iter().find_map(|i| match i.as_str() {
                "net/minecraft/network/protocol/common/custom/CustomPacketPayload" => {
                    Some(Feature::Network)
                }
                "net/minecraft/world/MenuProvider" => Some(Feature::Menu),
                _ => None,
            })
        });
        let Some(kind) = kind else {
            continue;
        };
        let feature = match kind {
            Feature::BlockBehaviour => {
                let behaviour: Vec<&str> = methods
                    .iter()
                    .copied()
                    .filter(|m| !BLOCK_LOOKS.contains(m) && !BLOCK_SHAPES.contains(m))
                    .collect();
                if !behaviour.is_empty() {
                    code.examples.insert(
                        c.name.rsplit('/').next().unwrap_or("").to_owned(),
                        behaviour.iter().map(|m| m.to_string()).collect(),
                    );
                    if behaviour.iter().any(|m| obfuscated_method(m)) {
                        Some(Feature::Unread)
                    } else {
                        Some(Feature::BlockBehaviour)
                    }
                } else if methods.iter().any(|m| BLOCK_SHAPES.contains(m)) {
                    Some(Feature::BlockShape)
                } else {
                    None
                }
            }
            Feature::ItemBehaviour => {
                let behaviour: Vec<&str> = methods
                    .iter()
                    .copied()
                    .filter(|m| !ITEM_LOOKS.contains(m))
                    .collect();
                if behaviour.is_empty() {
                    None
                } else {
                    code.examples.insert(
                        c.name.rsplit('/').next().unwrap_or("").to_owned(),
                        behaviour.iter().map(|m| m.to_string()).collect(),
                    );
                    Some(Feature::ItemBehaviour)
                }
            }
            // A class extending an obfuscated game class without declaring
            // anything adds nothing to read.
            Feature::Unread if methods.is_empty() => None,
            k => Some(k),
        };
        if let Some(f) = feature {
            count(f);
        }
    }
    for c in own.values() {
        for r in &c.refs {
            let event = (r.starts_with("net/neoforged/neoforge/event/")
                || r.starts_with("net/minecraftforge/event/")
                || r.starts_with("net/fabricmc/fabric/api/event/")
                || r.starts_with("net/fabricmc/fabric/api/entity/event/"))
                && !registration_event(r);
            if event {
                let simple = r.rsplit('/').next().unwrap_or(r);
                code.events
                    .insert(simple.split('$').next().unwrap_or(simple).to_owned());
            }
            if r.starts_with("com/mojang/brigadier/")
                && !code.features.contains_key(&Feature::Command)
            {
                code.features.insert(Feature::Command, 1);
            }
        }
    }
    if !code.events.is_empty() {
        code.features.insert(Feature::Events, code.events.len());
    }
    code
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// A class file with `name` extending `superclass`, declaring `methods`
    /// and naming `refs` in its constant pool.
    pub fn class_with(
        name: &str,
        superclass: &str,
        methods: &[&str],
        refs: &[&str],
        mixin: bool,
    ) -> Vec<u8> {
        let mut pool: Vec<Vec<u8>> = Vec::new();
        let utf = |pool: &mut Vec<Vec<u8>>, s: &str| -> u16 {
            let mut e = vec![1];
            e.extend((s.len() as u16).to_be_bytes());
            e.extend(s.as_bytes());
            pool.push(e);
            pool.len() as u16
        };
        let class = |pool: &mut Vec<Vec<u8>>, s: &str| -> u16 {
            let u = utf(pool, s);
            let mut e = vec![7];
            e.extend(u.to_be_bytes());
            pool.push(e);
            pool.len() as u16
        };
        let this = class(&mut pool, name);
        let sup = class(&mut pool, superclass);
        for r in refs {
            class(&mut pool, r);
        }
        // A long takes two slots.
        pool.push(vec![5, 0, 0, 0, 0, 0, 0, 0, 1]);
        pool.push(Vec::new());
        if mixin {
            utf(&mut pool, "Lorg/spongepowered/asm/mixin/Mixin;");
        }
        let desc = utf(&mut pool, "()V");
        let names: Vec<u16> = methods.iter().map(|m| utf(&mut pool, m)).collect();
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65];
        out.extend(((pool.len() + 1) as u16).to_be_bytes());
        for e in &pool {
            out.extend(e);
        }
        out.extend([0, 0x21]);
        out.extend(this.to_be_bytes());
        out.extend(sup.to_be_bytes());
        out.extend([0, 0]); // interfaces
        out.extend([0, 0]); // fields
        out.extend((names.len() as u16).to_be_bytes());
        for n in names {
            out.extend([0, 1]);
            out.extend(n.to_be_bytes());
            out.extend(desc.to_be_bytes());
            // One attribute, to be skipped.
            out.extend([0, 1, 0, 1, 0, 0, 0, 2, 0, 0]);
        }
        out.extend([0, 0]); // class attributes
        out
    }

    /// A class extending `superclass` and declaring nothing.
    pub fn class_file(name: &str, superclass: &str) -> Vec<u8> {
        class_with(name, superclass, &[], &[], false)
    }

    use super::{Annotation, Member, Value};
    use std::collections::HashMap;

    /// A constant pool being written.
    #[derive(Default)]
    struct Pool {
        entries: Vec<Vec<u8>>,
        utf: HashMap<String, u16>,
    }

    impl Pool {
        fn push(&mut self, e: Vec<u8>) -> u16 {
            self.entries.push(e);
            self.entries.len() as u16
        }
        fn utf(&mut self, s: &str) -> u16 {
            if let Some(i) = self.utf.get(s) {
                return *i;
            }
            let mut e = vec![1];
            e.extend((s.len() as u16).to_be_bytes());
            e.extend(s.as_bytes());
            let i = self.push(e);
            self.utf.insert(s.to_owned(), i);
            i
        }
        fn class(&mut self, s: &str) -> u16 {
            let u = self.utf(s);
            let mut e = vec![7];
            e.extend(u.to_be_bytes());
            self.push(e)
        }
        fn int(&mut self, n: i64) -> u16 {
            if let Ok(n) = i32::try_from(n) {
                let mut e = vec![3];
                e.extend(n.to_be_bytes());
                return self.push(e);
            }
            let mut e = vec![5];
            e.extend(n.to_be_bytes());
            let i = self.push(e);
            self.entries.push(Vec::new()); // a long takes two slots
            i
        }
        fn float(&mut self, f: f64) -> u16 {
            let mut e = vec![6];
            e.extend(f.to_bits().to_be_bytes());
            let i = self.push(e);
            self.entries.push(Vec::new());
            i
        }
        fn annotation(&mut self, a: &Annotation, out: &mut Vec<u8>) {
            out.extend(self.utf(&a.kind).to_be_bytes());
            out.extend((a.values.len() as u16).to_be_bytes());
            for (k, v) in &a.values {
                out.extend(self.utf(k).to_be_bytes());
                self.value(v, out);
            }
        }
        fn value(&mut self, v: &Value, out: &mut Vec<u8>) {
            match v {
                Value::Str(s) => {
                    out.push(b's');
                    out.extend(self.utf(s).to_be_bytes());
                }
                Value::Int(n) => {
                    out.push(if i32::try_from(*n).is_ok() {
                        b'I'
                    } else {
                        b'J'
                    });
                    out.extend(self.int(*n).to_be_bytes());
                }
                Value::Float(f) => {
                    out.push(b'D');
                    out.extend(self.float(*f).to_be_bytes());
                }
                Value::Class(d) => {
                    out.push(b'c');
                    out.extend(self.utf(d).to_be_bytes());
                }
                Value::Enum(t, n) => {
                    out.push(b'e');
                    out.extend(self.utf(t).to_be_bytes());
                    out.extend(self.utf(n).to_be_bytes());
                }
                Value::Annotation(a) => {
                    out.push(b'@');
                    self.annotation(a, out);
                }
                Value::Array(items) => {
                    out.push(b'[');
                    out.extend((items.len() as u16).to_be_bytes());
                    for i in items {
                        self.value(i, out);
                    }
                }
            }
        }
        /// An attributes table holding only `RuntimeInvisibleAnnotations`.
        fn annotations(&mut self, list: &[Annotation], out: &mut Vec<u8>) {
            if list.is_empty() {
                out.extend([0, 0]);
                return;
            }
            let mut body = (list.len() as u16).to_be_bytes().to_vec();
            for a in list {
                self.annotation(a, &mut body);
            }
            out.extend([0, 1]);
            out.extend(self.utf("RuntimeInvisibleAnnotations").to_be_bytes());
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
    }

    /// A class file with annotations on the class, its fields and methods
    /// (the members' bodies are left out: nothing here runs).
    pub fn annotated_class(
        name: &str,
        access: u16,
        annotations: &[Annotation],
        fields: &[Member],
        methods: &[Member],
    ) -> Vec<u8> {
        let mut pool = Pool::default();
        let this = pool.class(name);
        let sup = pool.class("java/lang/Object");
        let mut body = Vec::new();
        body.extend(access.to_be_bytes());
        body.extend(this.to_be_bytes());
        body.extend(sup.to_be_bytes());
        body.extend([0, 0]); // interfaces
        for list in [fields, methods] {
            body.extend((list.len() as u16).to_be_bytes());
            for m in list {
                body.extend(m.access.to_be_bytes());
                body.extend(pool.utf(&m.name).to_be_bytes());
                body.extend(pool.utf(&m.descriptor).to_be_bytes());
                pool.annotations(&m.annotations, &mut body);
            }
        }
        pool.annotations(annotations, &mut body);
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65];
        out.extend(((pool.entries.len() + 1) as u16).to_be_bytes());
        for e in &pool.entries {
            out.extend(e);
        }
        out.extend(body);
        out
    }

    /// An annotation of type `kind` (`org/spongepowered/asm/mixin/Mixin`).
    pub fn ann(kind: &str, values: Vec<(&str, Value)>) -> Annotation {
        Annotation {
            kind: format!("L{kind};"),
            values: values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        }
    }

    pub fn s(v: &str) -> Value {
        Value::Str(v.to_owned())
    }

    pub fn member(name: &str, descriptor: &str, annotations: Vec<Annotation>) -> Member {
        Member {
            name: name.to_owned(),
            descriptor: descriptor.to_owned(),
            access: 0x0002,
            annotations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::class_with as class_file;
    use super::*;

    /// Annotations come back as written: nested ones, arrays, class literals,
    /// enums, booleans and longs, on the class, a field and a method.
    #[test]
    fn reads_annotations_of_the_class_and_its_members() {
        use super::tests_support::{ann, annotated_class, member, s};
        let at = ann(
            "org/spongepowered/asm/mixin/injection/At",
            vec![("value", s("HEAD"))],
        );
        let inject = ann(
            "org/spongepowered/asm/mixin/injection/Inject",
            vec![
                ("method", Value::Array(vec![s("tick")])),
                ("at", Value::Array(vec![Value::Annotation(at)])),
                ("cancellable", Value::Int(1)),
                ("order", Value::Int(1 << 40)),
                ("weight", Value::Float(0.5)),
                ("policy", Value::Enum("Lx/Policy;".into(), "STRICT".into())),
            ],
        );
        let mixin = ann(
            "org/spongepowered/asm/mixin/Mixin",
            vec![(
                "value",
                Value::Array(vec![Value::Class(
                    "Lnet/minecraft/server/MinecraftServer;".into(),
                )]),
            )],
        );
        let shadow = ann("org/spongepowered/asm/mixin/Shadow", vec![]);
        let bytes = annotated_class(
            "a/ServerMixin",
            0x0421,
            std::slice::from_ref(&mixin),
            &[member("ticks", "I", vec![shadow])],
            &[member(
                "onTick",
                "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V",
                vec![inject.clone()],
            )],
        );
        let c = parse_annotated(&bytes).unwrap();
        assert_eq!(c.name, "a/ServerMixin");
        assert!(c.is_abstract());
        assert_eq!(c.annotations, std::slice::from_ref(&mixin));
        assert_eq!(c.annotations[0].name(), "Mixin");
        assert_eq!(
            c.annotations[0].get("value").unwrap().classes(),
            ["net/minecraft/server/MinecraftServer"]
        );
        assert_eq!(c.fields[0].name, "ticks");
        assert_eq!(c.fields[0].annotations[0].name(), "Shadow");
        let m = &c.methods[0];
        assert_eq!(m.annotations, [inject]);
        let i = &m.annotations[0];
        assert_eq!(i.strings("method"), ["tick"]);
        assert!(i.flag("cancellable"));
        assert_eq!(i.get("order"), Some(&Value::Int(1 << 40)));
        let at = i.get("at").unwrap().annotations();
        assert_eq!(at[0].strings("value"), ["HEAD"]);
        // The plain reader still reads it, and a cut file reads as nothing.
        assert!(parse(&bytes).unwrap().mixin);
        assert!(parse_annotated(&bytes[..bytes.len() - 2]).is_none());
    }

    #[test]
    fn reads_the_header_methods_and_pool() {
        let bytes = class_file(
            "a/Lamp",
            "net/minecraft/world/level/block/Block",
            &["<init>", "useWithoutItem", "lambda$x$0"],
            &["net/minecraft/world/item/Item"],
            false,
        );
        let c = parse(&bytes).unwrap();
        assert_eq!(c.name, "a/Lamp");
        assert_eq!(c.superclass, "net/minecraft/world/level/block/Block");
        assert_eq!(c.methods, ["useWithoutItem"]);
        assert!(c.refs.contains("net/minecraft/world/item/Item"));
        assert!(!c.mixin);
        assert!(parse(&bytes[..bytes.len() - 3]).is_none());
        assert!(parse(b"not a class").is_none());
    }

    #[test]
    fn follows_own_classes_to_the_game() {
        let block = "net/minecraft/world/level/block/Block";
        let files = [
            // Looks only: a template covers it.
            class_file("m/Plain", block, &["getStateForPlacement"], &[], false),
            class_file("m/Round", block, &["getShape"], &[], false),
            // Behaviour two levels down.
            class_file("m/Base", block, &["randomTick"], &[], false),
            class_file("m/Lamp", "m/Base", &[], &[], false),
            class_file(
                "m/Tile",
                "net/minecraft/world/level/block/entity/BlockEntity",
                &[],
                &[],
                false,
            ),
            class_file("m/Mix", "java/lang/Object", &[], &[], true),
            class_file(
                "m/Old",
                "net/minecraft/class_2248",
                &["method_9534"],
                &[],
                false,
            ),
            class_file(
                "m/Events",
                "java/lang/Object",
                &[],
                &[
                    "net/neoforged/neoforge/event/entity/player/PlayerInteractEvent$RightClickBlock",
                    "net/neoforged/neoforge/event/BuildCreativeModeTabContentsEvent",
                    "net/neoforged/neoforge/registries/DeferredRegister",
                ],
                false,
            ),
            class_file(
                "m/Gem",
                "net/minecraft/world/item/Item",
                &["appendHoverText"],
                &[],
                false,
            ),
        ];
        let code = census(files.iter().map(Vec::as_slice));
        assert_eq!(code.classes, 9);
        let n = |f| code.features.get(&f).copied().unwrap_or(0);
        assert_eq!(n(Feature::BlockBehaviour), 2, "{code:?}");
        assert_eq!(n(Feature::BlockShape), 1);
        assert_eq!(n(Feature::BlockEntity), 1);
        assert_eq!(n(Feature::Mixin), 1);
        assert_eq!(n(Feature::Unread), 1);
        assert_eq!(n(Feature::ItemBehaviour), 0);
        assert_eq!(
            code.events.iter().collect::<Vec<_>>(),
            ["PlayerInteractEvent"]
        );
        assert!(code.behaviour());
    }

    #[test]
    fn registration_only_is_not_behaviour() {
        let files = [
            class_file(
                "m/Mod",
                "java/lang/Object",
                &["register"],
                &[
                    "net/neoforged/neoforge/registries/DeferredRegister",
                    "net/minecraft/world/level/block/Block",
                ],
                false,
            ),
            class_file(
                "m/Pillar",
                "net/minecraft/world/level/block/RotatedPillarBlock",
                &[],
                &[],
                false,
            ),
        ];
        let code = census(files.iter().map(Vec::as_slice));
        assert!(!code.behaviour(), "{code:?}");
    }
}
