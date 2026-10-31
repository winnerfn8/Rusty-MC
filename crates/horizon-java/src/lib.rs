//! Java mods' code (docs/java-mod-runtime.md).
//!
//! - [`hooks`]: the runtime side of the mixin hook points. The game's ports of a few vanilla
//!   methods call [`hooks::run`] at their start and end, which costs one relaxed load and a
//!   branch while nothing is attached.
//! - [`locate`]: the player's own Java (never bundled), for the launcher and the game.
//! - [`classfile`]: the changes made to a mod's mixin class as it loads (concrete, or rebased onto
//!   the Java API layer's class for its target).
//! - [`fabric`]: Fabric API's Rust side: which hook point fires each event, the modules covered,
//!   entry points and registrations.
//! - `jvm` (cargo feature `jvm`): the embedded JVM, started from the player's Java through JNI,
//!   with the Java API layer (vanilla's names, `java/`) and Fabric API's facade; it loads mod jars,
//!   attaches their supported injections to the hook points and runs their entry points. It is
//!   the only module with unsafe code: JNI calls.
#![deny(unsafe_code)]
pub mod classfile;
pub mod fabric;
pub mod hooks;
#[cfg(feature = "jvm")]
#[allow(unsafe_code)]
pub mod jvm;
pub mod locate;
