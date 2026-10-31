//! Fabric API's Rust side (docs/java-mod-runtime.md): the smallest useful part of Fabric API,
//! with a Java facade under `java/net/fabricmc/` that mods compile against. What lives here is
//! what touches the game: which hook point fires each event and what it is handed, the Fabric API
//! modules this repository has a Rust version of (the verdicts name them), and what a mod's
//! entry points and registrations are. The JVM side (`jvm`) runs them.
use crate::hooks::{Hook, Obj, Site};
use std::path::PathBuf;

/// Fabric API's modules with a Rust version here (their mod ids, as mods' `depends` name them).
pub const MODULES: &[&str] = &["fabric-api-base", "fabric-lifecycle-events-v1"];

/// Mods the Java layer itself counts as loaded (`FabricLoader.isModLoaded`).
pub const PROVIDED: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-api-base",
    "fabric-lifecycle-events-v1",
];

/// A Fabric API event the game fires, in `GameEvent.java`'s order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `ServerTickEvents.START_SERVER_TICK`.
    StartServerTick,
    EndServerTick,
    /// `ServerTickEvents.START_WORLD_TICK`: each level's part of a server tick.
    StartWorldTick,
    EndWorldTick,
    /// `ServerLifecycleEvents.SERVER_STARTING`.
    ServerStarting,
    ServerStarted,
    ServerStopping,
    ServerStopped,
}

impl Event {
    pub const ALL: [Event; 8] = [
        Event::StartServerTick,
        Event::EndServerTick,
        Event::StartWorldTick,
        Event::EndWorldTick,
        Event::ServerStarting,
        Event::ServerStarted,
        Event::ServerStopping,
        Event::ServerStopped,
    ];

    pub fn from_index(i: i32) -> Option<Event> {
        usize::try_from(i)
            .ok()
            .and_then(|i| Event::ALL.get(i).copied())
    }

    /// The hook point that fires it (as Fabric API's own mixins do at the same vanilla places:
    /// `tickServer`'s start and end, `ServerLevel.tick`'s, the server's start and stop).
    pub fn hook(self) -> Hook {
        match self {
            Event::StartServerTick => Hook::TickServerHead,
            Event::EndServerTick => Hook::TickServerReturn,
            Event::StartWorldTick => Hook::LevelTickStart,
            Event::EndWorldTick => Hook::LevelTickEnd,
            Event::ServerStarting => Hook::ServerStarting,
            Event::ServerStarted => Hook::ServerStarted,
            Event::ServerStopping => Hook::ServerStopping,
            Event::ServerStopped => Hook::ServerStopped,
        }
    }

    /// What its listeners are handed, from the hook point's site: the server, or the level.
    pub fn argument(self, site: &Site) -> Obj {
        match self {
            Event::StartWorldTick | Event::EndWorldTick => site.this.clone().unwrap_or(Obj::Null),
            _ => Obj::Server,
        }
    }

    /// The events a hook point fires.
    pub fn at(hook: Hook) -> impl Iterator<Item = Event> {
        Event::ALL.into_iter().filter(move |e| e.hook() == hook)
    }
}

/// A mod's entry point (`fabric.mod.json`'s `entrypoints`): its kind (`main`, `client`,
/// `server`) and the class (or `Class::field`) to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrypoint {
    pub jar: PathBuf,
    pub mod_id: String,
    pub kind: String,
    pub spec: String,
}

impl Entrypoint {
    /// Whether a process with a window (else a dedicated server) runs it: `main` always,
    /// `client` with a window, `server` on a dedicated server.
    pub fn runs(&self, dedicated: bool) -> bool {
        match self.kind.as_str() {
            "main" => true,
            "client" => !dedicated,
            "server" => dedicated,
            _ => false,
        }
    }
}

/// A block or item a mod registered from Java (`Registry.register`), with its properties as
/// `key=value` pairs (`hardness=1.5`, `max_stack_size=16`...).
#[derive(Clone, Debug, PartialEq)]
pub struct Registration {
    /// `block` or `item`.
    pub registry: String,
    /// `ruby:ruby_block`.
    pub id: String,
    pub properties: Vec<(String, String)>,
}

impl Registration {
    pub fn new(registry: &str, id: &str, description: &str) -> Self {
        Self {
            registry: registry.to_owned(),
            id: id.to_owned(),
            properties: description
                .lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_event_has_one_hook_point_and_its_argument() {
        for (i, e) in Event::ALL.into_iter().enumerate() {
            assert_eq!(Event::from_index(i as i32), Some(e));
            assert!(Event::at(e.hook()).any(|x| x == e));
        }
        assert_eq!(Event::from_index(8), None);
        assert_eq!(Event::from_index(-1), None);
        assert_eq!(Event::at(Hook::GetDropsHead).count(), 0);
        let level = Site::new(Obj::Level(1), Vec::new());
        assert_eq!(Event::StartWorldTick.argument(&level), Obj::Level(1));
        assert_eq!(Event::ServerStarted.argument(&level), Obj::Server);
    }

    #[test]
    fn registrations_and_entry_points_read_as_mods_write_them() {
        let r = Registration::new("block", "ruby:ruby_block", "hardness=1.5\nlight=7\nbad\n");
        assert_eq!(r.get("hardness"), Some("1.5"));
        assert_eq!(r.get("light"), Some("7"));
        assert_eq!(r.properties.len(), 2);
        let e = |kind: &str| Entrypoint {
            jar: PathBuf::new(),
            mod_id: "m".into(),
            kind: kind.into(),
            spec: "a.B".into(),
        };
        assert!(e("main").runs(true) && e("main").runs(false));
        assert!(e("client").runs(false) && !e("client").runs(true));
        assert!(e("server").runs(true) && !e("server").runs(false));
        assert!(!e("preLaunch").runs(false));
    }
}
