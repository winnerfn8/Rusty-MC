//! The Java layer end to end: the test mod, built here with `javac` against the layer's own
//! classes, loads in a JVM started from this computer's Java, with a fake game as the host.
//!
//! - Its entry point runs at load: its block and items reach Rust as registrations, and it
//!   listens to Fabric API's events.
//! - Its `@Inject` at the start of `MinecraftServer.tickServer` runs from the hook point the
//!   game's `tick_server` calls, with the server as `this` (its `@Shadow getTickCount` answers the
//!   host's tick count), counting ticks and cancelling one.
//! - `ServerTickEvents.END_SERVER_TICK` fires at the tick's end and messages the host's player;
//!   `SERVER_STARTED` fires from its lifecycle point.
//! - Its `@ModifyReturnValue` doubles the mining progress at `getDestroyProgress`'s end, reading
//!   the block (`this`), the state, the player, the level and the position.
//!
//! Skipped (with a line saying so) where there is no JDK.
#![cfg(feature = "jvm")]
mod support;

use horizon_java::{
    hooks::{self, Call, EntityView, Hook, Host, Obj, Site},
    jvm::{self, Mods},
    locate,
};
use jni::objects::{GlobalRef, JClass, JValue};

/// A world with one player (entity 5, Alex) standing on stone at (1, 64, 2), at tick 40.
#[derive(Default)]
struct World {
    messages: Vec<(i32, String)>,
}

impl Host for World {
    fn tick_count(&self) -> i32 {
        40
    }
    fn levels(&self) -> Vec<i32> {
        vec![0]
    }
    fn level_name(&self, level: i32) -> String {
        if level == 0 {
            "minecraft:overworld".into()
        } else {
            String::new()
        }
    }
    fn block_state(&self, _level: i32, pos: [i32; 3]) -> Option<u32> {
        Some(if pos[1] <= 64 { 7 } else { 0 })
    }
    fn state_block(&self, state: u32) -> String {
        match state {
            7 => "minecraft:stone".into(),
            _ => "minecraft:air".into(),
        }
    }
    fn players(&self) -> Vec<i32> {
        vec![5]
    }
    fn entity(&self, id: i32) -> Option<EntityView> {
        (id == 5).then(|| EntityView {
            kind: "minecraft:player".into(),
            pos: [1.5, 65.0, 2.5],
            health: Some([20.0, 20.0]),
            name: "Alex".into(),
            alive: true,
            player: true,
            ..EntityView::default()
        })
    }
    fn send_message(&mut self, player: i32, text: &str, _overlay: bool) {
        self.messages.push((player, text.to_owned()));
    }
}

fn int(env: &mut jni::JNIEnv, class: &GlobalRef, name: &str) -> i32 {
    env.get_static_field(<&JClass>::from(class.as_obj()), name, "I")
        .and_then(|v| v.i())
        .unwrap()
}

fn flag(env: &mut jni::JNIEnv, class: &GlobalRef, name: &str) -> bool {
    env.get_static_field(<&JClass>::from(class.as_obj()), name, "Z")
        .and_then(|v| v.z())
        .unwrap()
}

fn text(env: &mut jni::JNIEnv, class: &GlobalRef, name: &str) -> String {
    let value = env
        .get_static_field(<&JClass>::from(class.as_obj()), name, "Ljava/lang/String;")
        .and_then(|v| v.l())
        .unwrap();
    env.get_string(&value.into()).unwrap().into()
}

fn server_site(_: &World) -> Site {
    Site::new(Obj::Server, vec![Obj::True])
}

#[test]
fn a_mod_runs_from_its_entry_point_and_hook_points() {
    let dir = std::env::temp_dir().join(format!("horizon-java-jvm-{}", std::process::id()));
    let jar = match support::tickcounter(&dir) {
        Ok(jar) => jar,
        Err(e) => {
            eprintln!("skipped: no JDK to build the test mod ({e})");
            return;
        }
    };
    let bindings = support::bindings(&jar);
    assert_eq!(bindings.len(), 2, "{bindings:?}");
    let tick = bindings
        .iter()
        .find(|b| b.hook == Hook::TickServerHead)
        .unwrap();
    assert_eq!(tick.member, "tickcounter$onTick");
    assert_eq!(tick.target, "net/minecraft/server/MinecraftServer");
    assert!(
        bindings
            .iter()
            .any(|b| b.hook == Hook::GetDestroyProgressReturn && b.modifies)
    );
    let entrypoints = support::entrypoints(&jar);
    assert_eq!(entrypoints.len(), 1);
    assert_eq!(entrypoints[0].spec, "dev.example.tickcounter.TickCounter");

    let libjvm = locate::libjvm("").expect("a Java runtime beside the JDK");
    let vm = jvm::start(&libjvm).unwrap();
    let mods = Mods {
        bindings,
        entrypoints,
        mod_ids: vec!["tickcounter".into()],
        game_dir: dir.clone(),
        dedicated: false,
    };
    let (layer, problems) = jvm::Layer::load(vm, &mods).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(layer.handlers(), 2);

    // The entry point registered a block, its item and an item.
    let registered = jvm::take_registrations();
    let ids: Vec<(&str, &str)> = registered
        .iter()
        .map(|r| (r.registry.as_str(), r.id.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            ("block", "tickcounter:ruby_block"),
            ("item", "tickcounter:ruby_block"),
            ("item", "tickcounter:ruby")
        ]
    );
    assert_eq!(registered[0].get("hardness"), Some("1.5"));
    assert_eq!(registered[0].get("explosion_resistance"), Some("6.0"));
    assert_eq!(registered[0].get("light"), Some("7"));
    assert_eq!(registered[0].get("requires_tool"), Some("true"));
    assert_eq!(registered[1].get("block"), Some("tickcounter:ruby_block"));
    assert_eq!(registered[2].get("max_stack_size"), Some("16"));
    assert_eq!(registered[2].get("rarity"), Some("rare"));
    // Its tick listener attached the tick's end; its start listener the server's start.
    assert_eq!(hooks::attached(Hook::TickServerReturn), 1);
    assert_eq!(hooks::attached(Hook::ServerStarted), 1);

    let mut env = vm.attach_current_thread_as_daemon().unwrap();
    let loader = layer.loader().clone();
    let class_of = |env: &mut jni::JNIEnv, name: &str| {
        let name = env.new_string(name).unwrap();
        let class = env
            .call_static_method(
                "java/lang/Class",
                "forName",
                "(Ljava/lang/String;ZLjava/lang/ClassLoader;)Ljava/lang/Class;",
                &[
                    JValue::Object(&name),
                    JValue::Bool(1),
                    JValue::Object(loader.as_obj()),
                ],
            )
            .unwrap()
            .l()
            .unwrap();
        env.new_global_ref(class).unwrap()
    };
    let counter = class_of(&mut env, "dev.example.tickcounter.TickCounter");
    assert!(
        flag(&mut env, &counter, "loaded"),
        "FabricLoader knows the mod"
    );
    let tick_mixin = layer
        .class("dev/example/tickcounter/mixin/TickMixin")
        .unwrap();
    let mining = layer
        .class("dev/example/tickcounter/mixin/MiningMixin")
        .unwrap();
    assert!(layer.install());

    let mut world = World::default();
    for _ in 0..3 {
        let mut call = Call::<()>::head();
        hooks::run(Hook::TickServerHead, &mut call, &mut world, server_site);
        assert!(!call.is_canceled());
    }
    assert_eq!(int(&mut env, &tick_mixin, "ticks"), 3);
    assert_eq!(
        int(&mut env, &tick_mixin, "seen"),
        40,
        "@Shadow getTickCount"
    );
    assert!(
        flag(&mut env, &tick_mixin, "isServer"),
        "this is the server"
    );

    let cancel = env
        .get_static_field_id(<&JClass>::from(tick_mixin.as_obj()), "cancel", "Z")
        .unwrap();
    env.set_static_field(
        <&JClass>::from(tick_mixin.as_obj()),
        cancel,
        JValue::Bool(1),
    )
    .unwrap();
    let mut call = Call::<()>::head();
    hooks::run(Hook::TickServerHead, &mut call, &mut world, server_site);
    assert!(call.is_canceled(), "ci.cancel() cancels the tick");
    assert_eq!(int(&mut env, &tick_mixin, "ticks"), 4);

    // The tick's end fires END_SERVER_TICK: tick 40 messages the player.
    let mut end = Call::returning(());
    hooks::run(Hook::TickServerReturn, &mut end, &mut world, server_site);
    assert_eq!(int(&mut env, &counter, "endTicks"), 1);
    assert_eq!(world.messages, [(5, "Tick 40, Alex".to_owned())]);

    hooks::event(Hook::ServerStarted, &mut world, |_| {
        Site::new(Obj::Server, Vec::new())
    });
    assert_eq!(int(&mut env, &counter, "started"), 1);

    // The @ModifyReturnValue doubles the mining progress at its end, with the real arguments.
    let mut progress = Call::returning(0.25f32);
    hooks::run(
        Hook::GetDestroyProgressReturn,
        &mut progress,
        &mut world,
        |_| {
            Site::new(
                Obj::Block(7),
                vec![
                    Obj::BlockState(7),
                    Obj::Entity(5),
                    Obj::Level(0),
                    Obj::BlockPos([1, 64, 2]),
                ],
            )
        },
    );
    assert_eq!(progress.into_value(), Some(0.5));
    let last = text(&mut env, &mining, "last");
    assert_eq!(
        last,
        "Block{minecraft:stone} Block{minecraft:stone} at 1, 64, 2 by Alex, above Block{minecraft:stone}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
