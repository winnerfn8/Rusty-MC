package net.minecraft.world.entity.player;

import dev.rustyminecraft.java.Host;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.LivingEntity;

/** A player. */
public class Player extends LivingEntity {
    public boolean isCreative() {
        return (Host.entityFlags(rm$id) & 4) != 0;
    }

    public boolean isSpectator() {
        return false;
    }

    /** A message to this player, in chat or above the hotbar ({@code overlay}). */
    public void displayClientMessage(Component message, boolean overlay) {
        Host.sendMessage(rm$id, message.getString(), overlay);
    }
}
