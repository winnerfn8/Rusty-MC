package net.minecraft.world.entity;

import dev.rustyminecraft.java.Host;

/** An entity with health. */
public class LivingEntity extends Entity {
    public float getHealth() {
        float h = Host.entityHealth(rm$id, false);
        return Float.isNaN(h) ? 0.0F : h;
    }

    public float getMaxHealth() {
        float h = Host.entityHealth(rm$id, true);
        return Float.isNaN(h) ? 0.0F : h;
    }

    public void setHealth(float health) {
        Host.setHealth(rm$id, Math.max(0.0F, Math.min(health, getMaxHealth())));
    }

    public void heal(float amount) {
        if (getHealth() > 0.0F) {
            setHealth(getHealth() + amount);
        }
    }

    public boolean isDeadOrDying() {
        return getHealth() <= 0.0F;
    }
}
