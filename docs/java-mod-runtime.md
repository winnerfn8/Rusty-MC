# Java mods' code: mixins and the Java layer

Status: **phases 1 to 4 done.** Content mods' translation is [java-mod-translation.md](java-mod-translation.md);
this page is about the code inside a Java mod jar: what Rusty Minecraft can say about it, where a mod's mixins can
bind, the JVM that runs small 26.x mods' injections, and (phase 4) the Java API layer with vanilla's names and the
part of Fabric API those mods use most. Heavy modpacks are out of scope, and rendering is a separate, later task.

## What was checked first

**Native hook points** ([native-mods.md](native-mods.md), `crates/horizon-client/src/game/native_hooks.rs`).
Native mods change the game's Rust source before it is compiled (horizon-mixins: HEAD, RETURN, OVERWRITE... on
named Rust functions). The stable part is a set of empty `native_*` functions the game calls at fixed places,
listed with contract versions in `native-hooks.json`: 43 hooks at API 28 (tick, frame, world lifecycle, server
tick, level tick, block changes, use item, break and place block, attack, tooltips, menus, screens...), only ever
added, each addition raising `NATIVE_HOOKS_API`. They are compile-time hooks: a Java mod can't be compiled into
the game on the player's computer, so its injections need hook points that dispatch while the game runs (below).
Their names and contracts follow NeoForge's events ([native-hooks-neoforge.md](native-hooks-neoforge.md)), not
vanilla methods, so none of them is "the start of `Class.method`" a mixin names.

**The port ledger** ([port-ledger.md](port-ledger.md), `tools/port_ledger.py`). Vanilla `Class.method` maps to the
Rust function whose doc comment opens with it (rule 1), the verdict map's `port` rows (`tools/port_map/`), and
weaker matches (same name, structure, rare constants). A full build reads the 26.3 jar; this machine has no
vanilla jar (Mojang's servers are not reachable from it), so the numbers here come from the ledger's own Rust
parser and verdict map, not checked against the jar: of **10,897** vanilla methods with a port, **10,094 (93%)**
map to exactly one Rust function, 551 to two, 252 to three or more. Two cautions:

- The methods mods target most are among the split ones. Rusty Minecraft runs the local player, a server's guests
  and mobs through separate paths and splits big vanilla methods into steps: `LivingEntity.hurtServer` has 3
  functions, `LivingEntity.die` 4, `Player.tick` 4, `ServerLevel.tick` 3, `LivingEntity.aiStep` 6,
  `Player.attack` 10. They need one port each (the paths calling it) before they can have a start and an end.
- Without the jar a citation can be a partial port (a function citing a method it implements one step of), so a
  row is a lead: each method was read before it got a hook point.

**Obfuscation.** Releases since 26.1 ship unobfuscated ([version-ports.md](version-ports.md); `tools/lab.py`
refuses a version with mappings). A 26.x mod's mixins name vanilla's real classes and methods
(`net/minecraft/server/MinecraftServer`, `tickServer`), which is what makes binding them by name possible. Older
Fabric jars name the game in intermediary (`net/minecraft/class_3176`) and Forge's methods are SRG (`m_6074_`);
NeoForge has used Mojang's names since 1.20.5, so its version decides.

**How 26.x mods draw.** Through Blaze3D, not OpenGL: `RenderSystem.getDevice()` hands a `GpuDevice`, drawing goes
through `CommandEncoder` and `RenderPass` with `RenderPipeline`s (`RenderPipelines`) and
`net.minecraft.client.renderer.rendertype.RenderType`, and 26.3 has an OpenGL and a Vulkan backend behind it
([benchmark.md](benchmark.md)). The repository's Java oracles use exactly these classes and no `org.lwjgl.opengl`.
So a mod draws through Mojang's high-level classes, which a later task could put over wgpu; a mod that calls
OpenGL itself (rare, and broken on the Vulkan backend too) can't run, and its verdict says so.

## Decisions

- **Java runs only in an optional layer.** [AGENTS.md](../AGENTS.md) now allows one exception to "everything players
  run is Rust": an embedded JVM, behind the `java-mods` cargo feature, off by default (Settings > Java mods > **Run
  their code with Java**), on a Java the player already has. Nothing is bundled.
- **Libraries.** Loader APIs and libraries that touch Minecraft (Fabric API, NeoForge, Architectury, GeckoLib,
  Cloth Config...) run only once this repository has a Rust version of them (none yet). Plain Java libraries that
  never touch Minecraft (bundled JSON, config or maths libraries) run as they are in the JVM.
- **No weaving.** There is no vanilla bytecode to weave into. Each supported injection binds to a hook point in
  the Rust code.

## Phase 1: the verdicts

`crates/horizon-port-pack/src/mixins.rs` reads each jar's mixin configs and every mixin member's annotations
(`classes.rs` reads class files' annotations), puts each in a group with its reason (supported, needs the Java
layer, not supported), lists the libraries, and tells 26.x mods from older ones. `check` and `translate` print it,
and Settings > Java mods shows a **Code:** line and every mixin and library on demand
([java-mod-translation.md](java-mod-translation.md#what-its-code-needs)). Tests build their jars and class files
themselves.

## Phase 2: hook points

- **The table.** `python3 tools/mixin_hook_table.py` writes `crates/horizon-port-pack/data/vanilla-one-to-one.tsv`:
  the vanilla methods with exactly one Rust function, names only (`Class.method`, `file::Impl::fn`, how matched).
  With a full ledger build (`.local/port-ledger/ledger.json`, from the jar) it takes the jar-checked matches;
  without one it says so in its header. `--stats` prints the counts above. `tools/check_public_tree.py` accepts it.
- **Choosing methods.** `horizon-port-pack mixin-targets <folder of jars>` ranks the vanilla methods the jars'
  mixins inject into (mods, then injections; start, end and inside counted apart), each with whether it has one
  Rust function and a hook point. Popular mods couldn't be downloaded on this machine (Modrinth and Fabric's Maven
  aren't reachable from it), so the first methods were chosen as methods small 26.x mods commonly inject into at
  their start or end, that have exactly one port and can be read whole: the server tick, mining speed, block drops
  and entity use. The next batch should come from running `mixin-targets` over popular 26.x mods on a machine that
  has them (never committed).
- **The hook points** (API 29): the start and end of `MinecraftServer.tickServer` (`tick_server`),
  `BlockBehaviour.getDestroyProgress` (`destroy_progress`), `Block.getDrops` (`block_loot_at`) and
  `Player.interactOn` (`interact_entity`). Each is a pair of native hooks (`native_mixin_<class>_<method>_head`
  and `_return`, taking a `horizon_java::hooks::Call`: cancel, set or change the return value), catalogued with
  `vanilla`, `class`, `at`, `side` and `cancellable`, and a runtime entry (`horizon_java::hooks::Hook`) that
  handlers attach to while the game runs. `tick_server`'s end comes before `FixedStep::record` (vanilla records the
  tick's time inside `tickServer`); `TAIL` binds to the end, which also runs after early returns.
- **Cost.** Nothing attached, a hook point is one relaxed atomic load and a branch, inlined at the call site.
  `cargo run --release -p horizon-java --example hook_cost` times the call-site pattern around a small body:
  **−0.08 to +0.49 ns per call** (noise) over three runs; a Rust handler that does nothing attached to both ends
  adds about 7 ns. The game's benchmark mode (`--benchmark standard`) needs vanilla resources, which this machine
  can't prepare: run it on a machine with them before and after to confirm in the real game.
- **Parity.** With nothing attached the calls change no gameplay step and no random draw (a start that answers
  skips the method, as the mixin would in vanilla). `native_hooks::tests::catalog_matches_hook_points` and
  `mixin_hook_points_match_the_runtime` pass. `tools/parity.py --changed-files` lists 366 of 367 harnesses for the
  touched files (`game.rs` and the client lib); they need the vanilla data in `.local/`, which this machine
  doesn't have, so CI or a machine with the data must run them.

## Phase 3: the Java layer

`crates/horizon-java` (feature `jvm`; the game's feature `java-mods`):

1. **The player's Java.** `locate.rs` finds a Java home from Settings' choice (a JDK or JRE folder, its `java`, or
   its `libjvm`) or on the computer (`JAVA_HOME`, `PATH`, the usual install folders, newest first), and its JVM
   library. The launcher shows which, with **Choose Java…**.
2. **The JVM.** JNI's invocation API through the `jni` crate, which loads `libjvm` at run time (nothing links
   against Java, and a game without Java starts as before). Options: `-Xrs` (the game keeps its signals) and
   headless. The layer's own classes (`java/`: `CallbackInfo`, `CallbackInfoReturnable` with Mixin's public names,
   and a helper) are compiled by `build.rs` with `javac --release 21` and defined in the system class loader.
3. **Mod jars** load with a `URLClassLoader` over the jars of the mods that are on. A mixin class is read from it,
   defined with `ACC_ABSTRACT` cleared (`classfile.rs`: most mixins are abstract), and one instance is made without
   running a constructor.
4. **Binding.** Each supported injection (from the same reading as the verdicts) becomes a `jvm::Binding`: its
   hook point, class, handler and descriptor. A call hands the handler a `CallbackInfo` (or
   `CallbackInfoReturnable` with a boolean or float return value), `null` and zeros for the target method's other
   arguments (phase 4 hands the real ones), and reads back `isCancelled` and the return value; a `@ModifyReturnValue` gets the value and returns
   the new one (boolean and float for now). A handler that throws is logged once and skipped.
5. **In the game**, `mods::start_java_layer` (after the translation, at start) binds the injections of the
   translated mods that are on, and of code-only jars (verdict *Doesn't run*: no content mod to switch, so the jar
   in the folder is what turns it on), when the switch is on, and logs `Horizon: Java layer: N handlers bound on
   <libjvm>`.

**The test mod** (`crates/horizon-java/tests/tickcounter/`, written here) is a Fabric mod for 26.3 with an
abstract mixin: an `@Inject(method = "tickServer", at = @At("HEAD"), cancellable = true)` that counts ticks and
cancels one, and a `@ModifyReturnValue` on `getDestroyProgress` that doubles it. `cargo test -p horizon-java
--features jvm` builds it with `javac`, reads it with horizon-port-pack, starts the JVM from this machine's Java,
binds both handlers and calls `hooks::run` exactly as `tick_server` and `destroy_progress` do: three ticks counted,
a cancelled tick, the progress doubled.

In the real game (`horizon-client` built with `--features java-mods`, the test mod's jar in a scratch game folder's
`mods/`, the switch on), the start translated the jar and logged `Horizon: Java layer: 2 handlers bound on
/usr/lib/jvm/java-21-openjdk-amd64/lib/server/libjvm.so in 55 ms`. It stopped there: this machine has no display
and no prepared vanilla resources, so no world opened and `tick_server` never ran. The run through `tick_server`
in a real world is the first thing to check on a machine with them: the same build and jar, then the counter
(or a log line from the handler) once the world ticks.

### Measurements

Linux, 4 cores of an Intel Xeon at 2.1 GHz (a cloud VM), OpenJDK 21.0.12,
`cargo run --release -p horizon-java --features jvm --example jvm_cost`, two runs:

| What | Time |
|---|---|
| Start the JVM | 35 to 41 ms |
| Load the test mod and bind its handler | 16 to 20 ms |
| One call, Rust to Java (an empty static method, through the `jni` crate) | 37 ns |
| One call, Java to Rust (an empty native method) | 9 to 10 ns |
| A hook point with the test mod's handler (a new `CallbackInfo` each call, a local frame, `isCancelled`) | 360 to 400 ns |
| A server tick's hook point with 100 handlers attached | 33 to 40 µs: under 0.1% of a 50 ms tick |
| A hook point with nothing attached | under 0.5 ns (above) |

The handler call is dominated by what surrounds the Java call (the `CallbackInfo` object, the local frame, the
second call to read `isCancelled`), not by crossing into Java.

## Phase 4: the Java API layer and Fabric API

### Hook points hand the game and the arguments

A hook point now passes the game as a `horizon_java::hooks::Host` and what the vanilla method is handed (a `Site`:
`this` and the arguments, as handles: the server, a level, an entity id, a block state id, a position, an item
stack, a hand), made only when something is attached (`hooks::run(hook, &mut call, self, |game| site)`; still one
relaxed load and a branch when idle). The game implements `Host` in `crates/horizon-client/src/game/java_host.rs`
through the same paths as native mods' server helpers: blocks of the level the hook point runs in (set through
`/setblock`'s path), entities (a server's players by entity id, mobs, -1 for the acting player), the players, a
message to one player or all. Six lifecycle points sit beside the native lifecycle hooks: a server starting
(`native_lifecycle_open_started`), started (world ready), stopping (the close request, before the players leave and
the final save), stopped (after the final save), and each level's part of a server tick (`native_level_tick_pre`
and `_post`).

### The Java API layer

`crates/horizon-java/java/net/minecraft/`: Java classes with vanilla 26.x's names, written for this repository
from public documentation only (vanilla's class, method and field names and signatures are the API mods compile
against; no line comes from Mojang's code). `tools/check_public_tree.py` accepts these sources and the test mod's,
and still rejects any other `.java` or `.class` file. `crates/horizon-java/api.txt` lists every class and public
member (64 classes); a test checks it against the compiled classes.

| Package | Classes |
|---|---|
| `resources` | `Identifier`, `ResourceKey` |
| `core`, `core.registries` | `BlockPos`, `Vec3i`, `Direction`, `Registry` (and `register`), `WritableRegistry`, `DefaultedRegistry`, `BuiltInRegistries` (`BLOCK`, `ITEM`), `Registries` |
| `network.chat`, `ChatFormatting` | `Component` (`literal`, `translatable`, `empty`), `MutableComponent` |
| `server`, `server.level`, `server.players` | `MinecraftServer`, `ServerLevel`, `ServerPlayer`, `PlayerList` |
| `world`, `world.phys` | `InteractionHand`, `InteractionResult`, `Vec3` |
| `world.level` and below | `Level`, `BlockGetter`, `ItemLike`, `Block`, `Blocks` (34 common ones), `BlockBehaviour` and its `Properties`, `BlockState`, `BlockEntity` |
| `world.item` | `Item` and its `Properties`, `BlockItem`, `Items` (18 common ones), `ItemStack`, `Rarity` |
| `world.entity` | `Entity`, `LivingEntity`, `Player` |

Values (`Identifier`, `BlockPos`, `Vec3`, `Component`...) are plain Java. Game objects hold the game's handle and
ask the game through `dev.rustyminecraft.java.Host`'s native methods (`src/jvm/natives.rs`), which reach the
`Host` of the hook point running on that thread and answer nothing anywhere else. A vanilla id the registries
haven't seen stands for the game's own block or item (`Blocks.STONE` is `minecraft:stone`, whatever the game makes
of it).

**A mixin is its target.** When the layer has a mixin's target class, the mixin class is rebased onto it as it loads
(`classfile::rebase`: its superclass becomes the API class, its constructors and abstract methods go), so `this`
*is* the server, the level, the entity or the block (`(MinecraftServer) (Object) this` works), and a `@Shadow`
method resolves to the API class's method of the same name and descriptor. One instance stands for each target
object (per thread, up to 4,096 per class), so a mixin's own fields (`@Unique`) are kept per target, as in Mixin.
A `BlockBehaviour` mixin is rebased onto `Block` (every `BlockBehaviour` is a `Block` in vanilla).

### Fabric API, the smallest useful part

Fabric API's modules `fabric-api-base` and `fabric-lifecycle-events-v1` have a Rust version: `src/fabric.rs`
knows which hook point fires each event and what it hands, with a Java facade under `java/net/fabricmc/` that mods
compile against:

- `ModInitializer`, `ClientModInitializer`, `DedicatedServerModInitializer`: the entry points `fabric.mod.json`
  names (a class, or `Class::field`) run when the layer starts, `main` first, then `client` in a game with a window
  or `server` on a dedicated server. A Kotlin adapter's entry point fails to run, saying so.
- `FabricLoader`: `isModLoaded` (the mods that are on, the game, and the covered Fabric API modules),
  `getGameDir`, `getConfigDir`, `getEnvironmentType`, `isDevelopmentEnvironment`.
- `Event` and `EventFactory` (`createArrayBacked`, `createWithPhases`): array-backed invokers, for a mod's own
  events too. Phases are accepted; listeners run in registration order.
- `ServerTickEvents` (`START_SERVER_TICK`, `END_SERVER_TICK`, `START_WORLD_TICK`, `END_WORLD_TICK`) and
  `ServerLifecycleEvents` (`SERVER_STARTING`, `SERVER_STARTED`, `SERVER_STOPPING`, `SERVER_STOPPED`): an event's
  first listener attaches its hook point, which then fires it before the mods' own mixin handlers there (as Fabric
  API's own mixins apply first).

**Registrations.** `Registry.register(BuiltInRegistries.BLOCK, ...)` and `ITEM` hand each registration to Rust;
after the entry points ran, `translate::apply_registrations` applies them to the mod's translated content before the
game reads it: a block's hardness, explosion resistance, light, tool requirement and friction; an item's stack size,
rarity, durability, fire resistance and crafting remainder. A block the jar has no blockstate for isn't added (it
would have no model).

### Verdicts, sides and versions

- **What the layer lacks.** `horizon-port-pack` reads `api.txt` and every class of a 26.x jar: the vanilla, Fabric
  and Mixin classes and members its code uses that the layer doesn't have yet are counted on the **Code:** line
  (*uses 14 vanilla or Fabric API classes or members the Java layer doesn't have yet*) and the first twelve listed
  (`SoundType`, `BlockBehaviour.Properties.sound()`). A mod using them fails where it uses them
  (`NoClassDefFoundError`, logged), so this is the list to grow the layer from.
- **Groups.** A `@Shadow` of a method the layer has, and a mod's own `@Unique` members, are now *supported*;
  `@Accessor`, `@Invoker`, `@Shadow` fields and `@Shadow` of a method the layer lacks *need more of the Java layer*.
  Fabric API counts as having a Rust version for its covered part, and its covered modules each get a line.
- **Sides.** A mixin config's `client` and `server` lists apply by the game running the hook point (`Host::side`:
  the server thread and a dedicated server are the server, the window the client); entry points by the process.
- **Versions.** The **Code:** details say which Java a mod's classes need (25 for 26.x). At start, a mod whose
  classes need a newer Java than the chosen one, or that requires a library with no Rust version (one that isn't
  another mod loading here), doesn't run its code, and the log says why.
- **Trust.** The Settings row says Java mods' code runs with full access to the computer, like native mods.

### The test mod, end to end

`tests/tickcounter/` (written here) now has three parts: `TickMixin` (`@Inject` at `tickServer`'s start, reading
`getTickCount` through a `@Shadow` and checking that `this` is the server), `MiningMixin` (`@ModifyReturnValue`
doubling `getDestroyProgress`, reading `this`, the state, the player, the level and the block below the position),
and `TickCounter`, its `main` entry point, which registers a block (hardness 1.5, light 7, tool required), its item
and an item (stacks of 16, rare), and listens to `END_SERVER_TICK` (a message to each player every 20 ticks) and
`SERVER_STARTED`. `cargo test -p horizon-java --features jvm` builds it against the layer's own classes, runs it
with a fake game as the host and checks each: the registrations reaching Rust with their properties, the shadow
answering the host's tick count, the cancelled tick, the player's message, the start event, the doubled progress
and the handler's reading of its arguments. `horizon-port-pack check` on its jar says *3 of 3 mixins supported*
and lists nothing missing.

In the game, `mods::start_java_layer` gathers the mods that are on, made for 26.x, whose Java and libraries are
there; loads them; runs their entry points; applies their registrations; and logs `Horizon: Java layer: N handlers
bound and M entry points run on <libjvm>`. As in phase 3, this machine has no display and no prepared vanilla
resources, so no world ran here: the first check on a machine with them is the test mod's jar in a game folder's
`mods/` with the switch on, then its message in chat every second.

### Measurements

Same machine as phase 3 (4 cores of a 2.1 GHz Xeon cloud VM, OpenJDK 21.0.12), `jvm_cost` and `hook_cost`, two
runs (the second beside a launcher build, so slower):

| What | Phase 3 | Phase 4 |
|---|---|---|
| Start the JVM (and define the layer's 72 classes) | 35 to 41 ms | 37 to 111 ms |
| Load the test mod and bind its handlers | 16 to 20 ms | 29 to 43 ms |
| A hook point with the tick handler (a `CallbackInfo` reused per handler and thread; the server as `this`) | 360 to 400 ns | 242 to 251 ns |
| A hook point with the mining handler (four argument objects made per call) | (no arguments) | 863 to 966 ns |
| A server tick's hook point with 100 handlers | 33 to 40 µs | 16 to 17 µs: 0.03% of a 50 ms tick |
| A hook point with nothing attached | under 0.5 ns | under 0.7 ns (noise) |

Per handler that is about 160 ns, short of the 100 ns aimed for: each still costs three JNI calls (reset the
`CallbackInfo`, the call, read `cancelled`) and the `jni` crate's exception check after each. A dispatcher in Java
that loops over a hook point's handlers (one JNI call per hook point) is the next step if it matters. Arguments cost
most: each is a JNI call that makes an object; a hook point makes them once for all its handlers.

## Limits

- Only the level a hook point runs in is reachable (the others are parked: reading them answers air), and only
  while a hook point runs: an entry point can register and listen, not read the world.
- Text is plain: styles are accepted and dropped, and a translatable component shows its key.
- `Block.getDrops`'s list isn't handed over (a start that cancels drops nothing); hook points hand no block entity;
  item stacks carry their item and count only. Players' health can't be set (mobs' can).
- No accessors or invokers yet, and a `@Shadow` field is the mixin's own, not the game's value.
- Of Fabric API, only the two modules above: no item groups, registry sync, networking, commands, keybinds or
  rendering. Of vanilla, the 64 classes above: anything else a mod uses is listed in its verdict.
- 26.x mods are compiled for Java 25; the layer's own classes for 21. Java mods run with the player's rights, like
  native mods; the JVM has no sandbox.
- Official builds don't have the feature; the switch says so, and such a game logs it.

## Next

1. Grow the API layer from the verdicts: run `horizon-port-pack check` over popular small 26.x mods on a machine
   that has them (never committed) and add the most-used missing classes and members, then `api.txt`.
2. Fabric API's `ItemGroupEvents` and creative tabs (content mods' items in vanilla's tabs), then registry sync.
3. Accessors and invokers (an interface the rebased class implements), and `@Shadow` fields read through the game.
4. More hook points from `mixin-targets` over the same mods, each with an API bump, its parity harnesses and a
   benchmark-mode run; one port for the split methods they target (`hurtServer`, `die`, `Player.tick`).
5. A Java-side dispatcher per hook point, if the per-handler cost matters.
6. Rendering (Mojang's high-level drawing classes over wgpu), separately.
