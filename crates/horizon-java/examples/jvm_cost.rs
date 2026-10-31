//! What the Java layer costs (docs/java-mod-runtime.md): one call each way between Rust and Java,
//! one hook point with the test mod's tick handler (the server as `this`), the mining handler
//! (four arguments made per call), and a server tick's hook point with 100 handlers attached.
//! Needs a JDK (it builds the test mod).
//! `cargo run --release -p horizon-java --features jvm --example jvm_cost`
#[path = "../tests/support/mod.rs"]
mod support;

use horizon_java::{
    hooks::{self, Call, Hook, NoHost, Obj, Site, Slot},
    jvm, locate,
};
use std::time::Instant;

fn main() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("horizon-java-cost-{}", std::process::id()));
    let jar = support::tickcounter(&dir)?;
    let all = support::bindings(&jar);
    let one: Vec<jvm::Binding> = all
        .iter()
        .filter(|b| b.hook == Hook::TickServerHead)
        .cloned()
        .collect();
    let mods = |bindings: Vec<jvm::Binding>| jvm::Mods {
        bindings,
        game_dir: dir.clone(),
        ..jvm::Mods::default()
    };
    let libjvm = locate::libjvm("")?;
    let home = locate::home_of(&libjvm);
    println!(
        "Java: {} (version {})",
        libjvm.display(),
        home.as_deref()
            .and_then(locate::version_of)
            .map_or("?".into(), |v| v.to_string())
    );
    let started = Instant::now();
    let vm = jvm::start(&libjvm)?;
    println!("JVM start: {:.0} ms", started.elapsed().as_secs_f64() * 1e3);

    // Warm up the JIT, then measure.
    jvm::time_rust_to_java(2_000_000)?;
    jvm::time_java_to_rust(2_000_000)?;
    println!(
        "Rust -> Java, empty static method: {:.1} ns per call",
        jvm::time_rust_to_java(10_000_000)?
    );
    println!(
        "Java -> Rust, empty native method: {:.1} ns per call",
        jvm::time_java_to_rust(10_000_000)?
    );

    let loaded = Instant::now();
    let (layer, problems) = jvm::Layer::load(vm, &mods(all.clone()))?;
    println!(
        "load and bind the test mod: {:.1} ms ({} handlers, problems: {problems:?})",
        loaded.elapsed().as_secs_f64() * 1e3,
        layer.handlers()
    );
    let server = Site::new(Obj::Server, vec![Obj::True]);
    let per_hook = |layer: &jvm::Layer, hook: Hook, site: &Site, slot: Slot, n: u32| {
        let mut host = NoHost;
        for _ in 0..n / 10 {
            layer.run(hook, &mut false, &mut slot.clone(), site, &mut host);
        }
        let start = Instant::now();
        for _ in 0..n {
            layer.run(hook, &mut false, &mut slot.clone(), site, &mut host);
        }
        start.elapsed().as_secs_f64() * 1e9 / f64::from(n)
    };
    println!(
        "a hook point with the tick handler (the server as this, its CallbackInfo reused): {:.0} ns",
        per_hook(&layer, Hook::TickServerHead, &server, Slot::None, 1_000_000)
    );
    let mining = Site::new(
        Obj::Block(1),
        vec![
            Obj::BlockState(1),
            Obj::Entity(5),
            Obj::Level(0),
            Obj::BlockPos([1, 64, 2]),
        ],
    );
    println!(
        "a hook point with the mining handler (four arguments made per call): {:.0} ns",
        per_hook(
            &layer,
            Hook::GetDestroyProgressReturn,
            &mining,
            Slot::Float(0.25),
            1_000_000
        )
    );

    let hundred: Vec<jvm::Binding> = one.iter().cycle().take(100).cloned().collect();
    let (layer100, _) = jvm::Layer::load(vm, &mods(hundred))?;
    let ns = per_hook(&layer100, Hook::TickServerHead, &server, Slot::None, 20_000);
    println!(
        "a server tick's hook point with 100 handlers: {:.1} us ({:.3}% of a 50 ms tick)",
        ns / 1e3,
        ns / 1e6 / 50.0 * 100.0
    );

    // Through the game's own call: hooks::run, as tick_server calls it.
    layer100.install();
    let start = Instant::now();
    let n = 20_000;
    for _ in 0..n {
        let mut call = Call::<()>::head();
        hooks::run(Hook::TickServerHead, &mut call, &mut NoHost, |_| {
            Site::new(Obj::Server, vec![Obj::True])
        });
    }
    println!(
        "the same through hooks::run (what tick_server calls): {:.1} us per tick",
        start.elapsed().as_secs_f64() * 1e6 / f64::from(n)
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
