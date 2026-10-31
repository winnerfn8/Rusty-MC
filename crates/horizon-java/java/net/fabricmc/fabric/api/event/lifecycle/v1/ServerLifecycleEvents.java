package net.fabricmc.fabric.api.event.lifecycle.v1;

import dev.rustyminecraft.java.fabric.GameEvent;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.MinecraftServer;

/** A server starting and stopping; fired by the game's lifecycle points (Rust). */
public final class ServerLifecycleEvents {
    private ServerLifecycleEvents() {}

    public static final Event<ServerStarting> SERVER_STARTING = GameEvent.create(GameEvent.SERVER_STARTING, ServerStarting.class, listeners -> server -> {
        for (ServerStarting l : listeners) {
            l.onServerStarting(server);
        }
    });

    public static final Event<ServerStarted> SERVER_STARTED = GameEvent.create(GameEvent.SERVER_STARTED, ServerStarted.class, listeners -> server -> {
        for (ServerStarted l : listeners) {
            l.onServerStarted(server);
        }
    });

    public static final Event<ServerStopping> SERVER_STOPPING = GameEvent.create(GameEvent.SERVER_STOPPING, ServerStopping.class, listeners -> server -> {
        for (ServerStopping l : listeners) {
            l.onServerStopping(server);
        }
    });

    public static final Event<ServerStopped> SERVER_STOPPED = GameEvent.create(GameEvent.SERVER_STOPPED, ServerStopped.class, listeners -> server -> {
        for (ServerStopped l : listeners) {
            l.onServerStopped(server);
        }
    });

    @FunctionalInterface
    public interface ServerStarting {
        void onServerStarting(MinecraftServer server);
    }

    @FunctionalInterface
    public interface ServerStarted {
        void onServerStarted(MinecraftServer server);
    }

    @FunctionalInterface
    public interface ServerStopping {
        void onServerStopping(MinecraftServer server);
    }

    @FunctionalInterface
    public interface ServerStopped {
        void onServerStopped(MinecraftServer server);
    }
}
