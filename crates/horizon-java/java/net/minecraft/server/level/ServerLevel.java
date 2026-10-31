package net.minecraft.server.level;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.Level;

/** A level on the server. */
public class ServerLevel extends Level {
    @Override
    public MinecraftServer getServer() {
        return MinecraftServer.rm$instance();
    }

    /** The players in this level. */
    public List<ServerPlayer> players() {
        List<ServerPlayer> here = new ArrayList<>();
        for (ServerPlayer p : getServer().getPlayerList().getPlayers()) {
            if (p.level().equals(this)) {
                here.add(p);
            }
        }
        return here;
    }
}
