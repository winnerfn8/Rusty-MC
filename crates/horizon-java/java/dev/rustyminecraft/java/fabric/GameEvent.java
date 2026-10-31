package dev.rustyminecraft.java.fabric;

import java.util.function.Function;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;

/**
 * A Fabric API event the game fires: Rust (crates/horizon-java/src/fabric.rs) knows which hook
 * point fires each, hears when one gets its first listener, and calls {@link #fire} there.
 */
public final class GameEvent<T> extends ArrayBackedEvent<T> {
    public static final int START_SERVER_TICK = 0;
    public static final int END_SERVER_TICK = 1;
    public static final int START_WORLD_TICK = 2;
    public static final int END_WORLD_TICK = 3;
    public static final int SERVER_STARTING = 4;
    public static final int SERVER_STARTED = 5;
    public static final int SERVER_STOPPING = 6;
    public static final int SERVER_STOPPED = 7;
    private static final int COUNT = 8;

    @SuppressWarnings("rawtypes")
    private static final GameEvent[] EVENTS = new GameEvent[COUNT];

    private final int id;

    private GameEvent(int id, Class<? super T> type, Function<T[], T> factory) {
        super(type, null, factory);
        this.id = id;
    }

    public static <T> GameEvent<T> create(int id, Class<? super T> type, Function<T[], T> factory) {
        GameEvent<T> event = new GameEvent<>(id, type, factory);
        EVENTS[id] = event;
        return event;
    }

    /** The event's first listener: the game attaches its hook point (registered by jvm.rs). */
    private static native void listen(int event);

    @Override
    public synchronized void register(T listener) {
        boolean first = listeners() == 0;
        super.register(listener);
        if (first) {
            listen(id);
        }
    }

    /** Fires event {@code id} with its argument (the server, or a level), from Rust. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static void fire(int id, Object argument) {
        GameEvent event = EVENTS[id];
        if (event == null) {
            return;
        }
        Object invoker = event.invoker();
        switch (id) {
            case START_SERVER_TICK -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.StartTick) invoker).onStartTick((MinecraftServer) argument);
            case END_SERVER_TICK -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.EndTick) invoker).onEndTick((MinecraftServer) argument);
            case START_WORLD_TICK -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.StartWorldTick) invoker).onStartTick((ServerLevel) argument);
            case END_WORLD_TICK -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.EndWorldTick) invoker).onEndTick((ServerLevel) argument);
            case SERVER_STARTING -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents.ServerStarting) invoker).onServerStarting((MinecraftServer) argument);
            case SERVER_STARTED -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents.ServerStarted) invoker).onServerStarted((MinecraftServer) argument);
            case SERVER_STOPPING -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents.ServerStopping) invoker).onServerStopping((MinecraftServer) argument);
            case SERVER_STOPPED -> ((net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents.ServerStopped) invoker).onServerStopped((MinecraftServer) argument);
            default -> {}
        }
    }

    /** Loads the events' classes, so their fields exist before a mod registers (from Rust). */
    public static void init() {
        net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents.START_SERVER_TICK.getClass();
        net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents.SERVER_STARTED.getClass();
    }
}
