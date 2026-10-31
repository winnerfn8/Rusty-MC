//! What the optional Java layer's API offers (`crates/horizon-java/api.txt`, generated from its
//! classes: a part of vanilla 26.x's API with its names, Fabric API's covered modules, Mixin's
//! callback classes), and what a jar's code uses of those APIs that the layer doesn't have yet
//! (docs/java-mod-runtime.md). A mod that uses something missing fails where it uses it
//! (`NoClassDefFoundError`, `NoSuchMethodError`): its verdict says what.
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

const LISTING: &str = include_str!("../../horizon-java/api.txt");

/// The packages the layer stands in for: a mod's use of anything else is its own business.
const SURFACE: &[&str] = &[
    "net/minecraft/",
    "com/mojang/",
    "net/fabricmc/",
    "org/spongepowered/asm/mixin/injection/callback/",
];

/// Methods every class has from Java itself (`Object`, `Enum`, `Comparable`, `Iterable`).
const FROM_JAVA: &[&str] = &[
    "equals",
    "hashCode",
    "toString",
    "getClass",
    "name",
    "ordinal",
    "compareTo",
    "getDeclaringClass",
    "forEach",
    "spliterator",
];

struct Class {
    supers: Vec<String>,
    members: HashSet<(String, String)>,
}

/// The layer's API, as `api.txt` lists it.
pub struct Api {
    classes: HashMap<String, Class>,
    modules: Vec<String>,
}

/// The API `api.txt` lists.
pub fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| Api::parse(LISTING))
}

/// Whether the layer stands in for this class's package.
pub fn on_surface(class: &str) -> bool {
    SURFACE.iter().any(|p| class.starts_with(p))
}

impl Api {
    fn parse(text: &str) -> Api {
        let mut classes = HashMap::new();
        let mut modules = Vec::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if let Some(m) = line.strip_prefix("module ") {
                modules.push(m.trim().to_owned());
            } else if let Some(rest) = line.strip_prefix("class ") {
                let mut words = rest.split_whitespace();
                let Some(name) = words.next() else { continue };
                classes.insert(
                    name.to_owned(),
                    Class {
                        supers: words.map(str::to_owned).collect(),
                        members: HashSet::new(),
                    },
                );
                current = Some(name.to_owned());
            } else if let (Some(c), Some((n, d))) = (&current, line.trim().split_once(' ')) {
                if let Some(class) = classes.get_mut(c) {
                    class.members.insert((n.to_owned(), d.to_owned()));
                }
            }
        }
        Api { classes, modules }
    }

    /// Fabric API's modules with a Rust version here.
    pub fn modules(&self) -> &[String] {
        &self.modules
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.classes.contains_key(class)
    }

    /// Whether `class` has the member `name` with `descriptor`, itself or through its
    /// superclasses and interfaces.
    pub fn has_member(&self, class: &str, name: &str, descriptor: &str) -> bool {
        let mut seen = HashSet::new();
        let mut todo = vec![class.to_owned()];
        while let Some(c) = todo.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            match self.classes.get(&c) {
                Some(k) => {
                    if k.members
                        .contains(&(name.to_owned(), descriptor.to_owned()))
                    {
                        return true;
                    }
                    todo.extend(k.supers.iter().cloned());
                }
                None if c.starts_with("java/") && FROM_JAVA.contains(&name) => return true,
                None => {}
            }
        }
        false
    }

    /// Whether the layer has the method `class.name` with `descriptor` (a `@Shadow` of it binds).
    pub fn has_method(&self, class: &str, name: &str, descriptor: &str) -> bool {
        self.has_member(class, name, descriptor)
    }
}

/// A class name as people read it: `net/minecraft/world/level/block/state/BlockBehaviour$Properties`
/// is `BlockBehaviour.Properties`.
fn simple(class: &str) -> String {
    class.rsplit('/').next().unwrap_or(class).replace('$', ".")
}

/// What `uses` (a jar's class and member references) needs of the layer's surface that it lacks,
/// as lines people read: `SoundType` for a class, `BlockBehaviour.Properties.sound()` for a method.
pub fn missing<'a>(
    classes: impl IntoIterator<Item = &'a str>,
    members: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
) -> Vec<String> {
    let api = api();
    let mut out = BTreeSet::new();
    for c in classes {
        if on_surface(c) && !api.has_class(c) {
            out.insert(simple(c));
        }
    }
    for (c, n, d) in members {
        if !on_surface(c) {
            continue;
        }
        if !api.has_class(c) {
            out.insert(simple(c));
        } else if !api.has_member(c, n, d) {
            let shown = match n {
                "<init>" => format!("new {}()", simple(c)),
                n if d.starts_with('(') => format!("{}.{n}()", simple(c)),
                n => format!("{}.{n}", simple(c)),
            };
            out.insert(shown);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_has_the_layer() {
        let api = api();
        assert!(
            api.modules()
                .iter()
                .any(|m| m == "fabric-lifecycle-events-v1")
        );
        assert!(api.has_class("net/minecraft/core/BlockPos"));
        assert!(api.has_member(
            "net/minecraft/core/BlockPos",
            "above",
            "()Lnet/minecraft/core/BlockPos;"
        ));
        // Through the superclass, and from Java itself.
        assert!(api.has_member("net/minecraft/core/BlockPos", "getX", "()I"));
        assert!(api.has_member(
            "net/minecraft/server/level/ServerPlayer",
            "getHealth",
            "()F"
        ));
        assert!(api.has_member("net/minecraft/core/Direction", "ordinal", "()I"));
        assert!(!api.has_member("net/minecraft/core/BlockPos", "spiral", "()V"));
        assert!(api.has_method(
            "net/minecraft/server/MinecraftServer",
            "getTickCount",
            "()I"
        ));
    }

    #[test]
    fn says_what_a_mod_uses_that_the_layer_lacks() {
        let props = "net/minecraft/world/level/block/state/BlockBehaviour$Properties";
        let lacking = missing(
            [
                "net/minecraft/core/BlockPos",
                "net/minecraft/world/level/block/SoundType",
                "com/example/Own",
                "java/util/List",
            ],
            [
                (
                    props,
                    "strength",
                    "(F)Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;",
                ),
                (
                    props,
                    "sound",
                    "(Lnet/minecraft/world/level/block/SoundType;)Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;",
                ),
                (
                    "net/minecraft/world/level/block/Block",
                    "<init>",
                    "(Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;)V",
                ),
                (
                    "net/minecraft/world/entity/Mob",
                    "setTarget",
                    "(Lnet/minecraft/world/entity/LivingEntity;)V",
                ),
                ("com/example/Own", "go", "()V"),
            ],
        );
        assert_eq!(
            lacking,
            ["BlockBehaviour.Properties.sound()", "Mob", "SoundType"]
        );
    }
}
