package net.fabricmc.fabric.api.event.lifecycle.v1;

import dev.rustyminecraft.java.fabric.GameEvent;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;

/** The server's tick and its levels' ticks; fired by the game's hook points (Rust). */
public final class ServerTickEvents {
    private ServerTickEvents() {}

    public static final Event<StartTick> START_SERVER_TICK = GameEvent.create(GameEvent.START_SERVER_TICK, StartTick.class, listeners -> server -> {
        for (StartTick l : listeners) {
            l.onStartTick(server);
        }
    });

    public static final Event<EndTick> END_SERVER_TICK = GameEvent.create(GameEvent.END_SERVER_TICK, EndTick.class, listeners -> server -> {
        for (EndTick l : listeners) {
            l.onEndTick(server);
        }
    });

    public static final Event<StartWorldTick> START_WORLD_TICK = GameEvent.create(GameEvent.START_WORLD_TICK, StartWorldTick.class, listeners -> world -> {
        for (StartWorldTick l : listeners) {
            l.onStartTick(world);
        }
    });

    public static final Event<EndWorldTick> END_WORLD_TICK = GameEvent.create(GameEvent.END_WORLD_TICK, EndWorldTick.class, listeners -> world -> {
        for (EndWorldTick l : listeners) {
            l.onEndTick(world);
        }
    });

    @FunctionalInterface
    public interface StartTick {
        void onStartTick(MinecraftServer server);
    }

    @FunctionalInterface
    public interface EndTick {
        void onEndTick(MinecraftServer server);
    }

    @FunctionalInterface
    public interface StartWorldTick {
        void onStartTick(ServerLevel world);
    }

    @FunctionalInterface
    public interface EndWorldTick {
        void onEndTick(ServerLevel world);
    }
}
