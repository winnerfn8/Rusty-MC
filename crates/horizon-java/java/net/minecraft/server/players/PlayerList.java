package net.minecraft.server.players;

import dev.rustyminecraft.java.Host;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;

/** The server's players. */
public class PlayerList {
    private final MinecraftServer server;

    public PlayerList(MinecraftServer server) {
        this.server = server;
    }

    public MinecraftServer getServer() {
        return server;
    }

    public List<ServerPlayer> getPlayers() {
        List<ServerPlayer> players = new ArrayList<>();
        for (int id : Host.players()) {
            if (dev.rustyminecraft.java.Objects.entity(id) instanceof ServerPlayer p) {
                players.add(p);
            }
        }
        return players;
    }

    public int getPlayerCount() {
        return Host.players().length;
    }

    public ServerPlayer getPlayerByName(String name) {
        for (ServerPlayer p : getPlayers()) {
            if (p.getName().getString().equalsIgnoreCase(name)) {
                return p;
            }
        }
        return null;
    }

    /** A system message to every player, in chat or above the hotbar. */
    public void broadcastSystemMessage(Component message, boolean overlay) {
        if (overlay) {
            for (int id : Host.players()) {
                Host.sendMessage(id, message.getString(), true);
            }
        } else {
            Host.broadcast(message.getString());
        }
    }
}
