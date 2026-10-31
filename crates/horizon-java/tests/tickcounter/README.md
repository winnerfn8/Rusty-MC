# Tick counter: the Java layer's test mod

A Fabric mod for 26.3, written for this repository:

- `TickMixin` counts server ticks from an `@Inject` at the start of `MinecraftServer.tickServer`,
  reads the server's tick count through a `@Shadow` method, and cancels the tick when asked to.
- `MiningMixin` doubles the mining progress with a `@ModifyReturnValue` on
  `BlockBehaviour.getDestroyProgress`, and reads its arguments (the block state, the player, the
  level, the position) and `this` (the block).
- `TickCounter`, its `main` entry point, registers a block, its item and an item, and listens to
  Fabric API's `ServerTickEvents.END_SERVER_TICK` (a message to each player every 20 ticks) and
  `ServerLifecycleEvents.SERVER_STARTED`.

The tests (`tests/jvm.rs`) and `examples/jvm_cost.rs` compile it with the JDK's `javac` into a jar,
against the Java layer's own classes (`java/`: the API layer with vanilla's names, Fabric API's
facade, Mixin's `CallbackInfo`) and `stubs/`: the Mixin and MixinExtras annotations it uses, written
here (names and members from their public documentation). Neither is in the jar: the annotations are
kept in the class files only, and the layer's classes are the JVM's own.
