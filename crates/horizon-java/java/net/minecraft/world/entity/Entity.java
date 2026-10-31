package net.minecraft.world.entity;

import dev.rustyminecraft.java.Host;
import dev.rustyminecraft.java.Objects;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** An entity, by the game's id. A mixin of an entity class gets the entity as {@code this}. */
public class Entity {
    protected int rm$id;

    public void rm$bind(long handle) {
        rm$id = (int) handle;
    }

    public int getId() {
        return rm$id;
    }

    public double getX() {
        return Host.entityPos(rm$id, 0);
    }

    public double getY() {
        return Host.entityPos(rm$id, 1);
    }

    public double getZ() {
        return Host.entityPos(rm$id, 2);
    }

    public Vec3 position() {
        return new Vec3(getX(), getY(), getZ());
    }

    public BlockPos blockPosition() {
        return BlockPos.containing(getX(), getY(), getZ());
    }

    public Level level() {
        return Objects.level(Host.entityLevel(rm$id));
    }

    public Component getName() {
        String name = Host.entityName(rm$id);
        return Component.literal(name == null ? "" : name);
    }

    public Component getDisplayName() {
        return getName();
    }

    public boolean isAlive() {
        return (Host.entityFlags(rm$id) & 1) != 0;
    }

    public boolean isRemoved() {
        return Host.entityKind(rm$id) == null;
    }

    public double distanceToSqr(double x, double y, double z) {
        double dx = getX() - x, dy = getY() - y, dz = getZ() - z;
        return dx * dx + dy * dy + dz * dz;
    }

    public double distanceToSqr(Entity other) {
        return distanceToSqr(other.getX(), other.getY(), other.getZ());
    }

    public float distanceTo(Entity other) {
        return (float) Math.sqrt(distanceToSqr(other));
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Entity e && e.rm$id == rm$id;
    }

    @Override
    public int hashCode() {
        return rm$id;
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "[" + Host.entityKind(rm$id) + " " + rm$id + "]";
    }
}
