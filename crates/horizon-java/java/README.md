# The Java layer's own classes

Compiled by `build.rs` (feature `jvm`, with the JDK's `javac --release 21`) and defined in the JVM's
system class loader before any mod loads (`src/jvm/mod.rs`, superclasses first). All of it is written
for Rusty Minecraft, from public documentation only: the class, method and field names and signatures
are the public APIs mods compile against (vanilla 26.x's names, Fabric's, Mixin's), so a mod's
bytecode links to them; no line comes from Mojang's, Fabric's or Mixin's code.

- `net/minecraft/`: the Java API layer, a small part of vanilla's API with its names
  (docs/java-mod-runtime.md lists it). Values (`Identifier`, `BlockPos`, `Vec3`, `Direction`,
  `Component`) are plain Java; game objects (`MinecraftServer`, `Level`, `ServerLevel`, `Entity`,
  `LivingEntity`, `Player`, `ServerPlayer`, `Block`, `BlockState`, `Item`, `ItemStack`) hold the
  game's handle (a level, an entity id, a block state id) and ask the game through
  `dev/rustyminecraft/java/Host.java`'s native methods. `BuiltInRegistries.BLOCK` and `ITEM` take a
  mod's registrations and hand them to the game.
- `net/fabricmc/`: Fabric's facade: `ModInitializer` and the other entry points, `FabricLoader`,
  `Event` and `EventFactory`, and `ServerTickEvents` and `ServerLifecycleEvents`, which the game's
  hook points fire (`src/fabric.rs` says which).
- `org/spongepowered/asm/mixin/injection/callback/`: what a mixin handler is handed
  (`CallbackInfo`, `CallbackInfoReturnable`), with Mixin's public names.
- `dev/rustyminecraft/java/`: the layer's internals: `Host` (natives into the game), `Objects`
  (the objects a hook point hands a handler), `GameRegistry`, `Layer` (the mods' class loader,
  entry points, measurements) and `fabric/` (events the game fires, the loader's answers).

Members named `rm$...` are the layer's own (binding an object to the game's handle, describing a
registration); no mod calls them.
