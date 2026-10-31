package net.minecraft.server;

import dev.rustyminecraft.java.Host;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.level.Level;

/**
 * The server: the game's own (a server thread, a dedicated server). One object stands for it; a
 * mixin of this class gets it as {@code this}.
 */
public class MinecraftServer {
    private static final MinecraftServer INSTANCE = new MinecraftServer();
    private final PlayerList playerList = new PlayerList(this);

    /** The object standing for the server. */
    public static MinecraftServer rm$instance() {
        return INSTANCE;
    }

    /** A mixin instance standing for the server: nothing to bind. */
    public void rm$bind(long handle) {}

    public int getTickCount() {
        return Host.tickCount();
    }

    public PlayerList getPlayerList() {
        return INSTANCE.playerList;
    }

    public int getPlayerCount() {
        return Host.players().length;
    }

    public Iterable<ServerLevel> getAllLevels() {
        List<ServerLevel> levels = new ArrayList<>();
        for (int l : Host.levels()) {
            levels.add((ServerLevel) dev.rustyminecraft.java.Objects.level(l));
        }
        return levels;
    }

    public ServerLevel overworld() {
        return (ServerLevel) dev.rustyminecraft.java.Objects.level(0);
    }

    public boolean isDedicatedServer() {
        return false;
    }

    public boolean isSameThread() {
        return Host.inGame();
    }

    public boolean isRunning() {
        return true;
    }

    @Override
    public String toString() {
        return "MinecraftServer";
    }
}
