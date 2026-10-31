package net.minecraft.server.level;

import dev.rustyminecraft.java.Host;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Player;

/** A player on the server. */
public class ServerPlayer extends Player {
    @Override
    public ServerLevel level() {
        return (ServerLevel) super.level();
    }

    public ServerLevel serverLevel() {
        return level();
    }

    public void sendSystemMessage(Component message) {
        Host.sendMessage(rm$id, message.getString(), false);
    }

    public void sendSystemMessage(Component message, boolean overlay) {
        Host.sendMessage(rm$id, message.getString(), overlay);
    }
}
