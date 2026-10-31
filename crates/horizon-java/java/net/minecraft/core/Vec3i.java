package net.minecraft.core;

/** Three integer coordinates. */
public class Vec3i implements Comparable<Vec3i> {
    public static final Vec3i ZERO = new Vec3i(0, 0, 0);

    private final int x;
    private final int y;
    private final int z;

    public Vec3i(int x, int y, int z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public int getX() {
        return x;
    }

    public int getY() {
        return y;
    }

    public int getZ() {
        return z;
    }

    public double distSqr(Vec3i other) {
        double dx = x - other.x, dy = y - other.y, dz = z - other.z;
        return dx * dx + dy * dy + dz * dz;
    }

    public int distManhattan(Vec3i other) {
        return Math.abs(x - other.x) + Math.abs(y - other.y) + Math.abs(z - other.z);
    }

    public boolean closerThan(Vec3i other, double distance) {
        return distSqr(other) < distance * distance;
    }

    public String toShortString() {
        return x + ", " + y + ", " + z;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Vec3i v && x == v.x && y == v.y && z == v.z;
    }

    @Override
    public int hashCode() {
        return (y + z * 31) * 31 + x;
    }

    @Override
    public int compareTo(Vec3i other) {
        if (y != other.y) {
            return Integer.compare(y, other.y);
        }
        return z != other.z ? Integer.compare(z, other.z) : Integer.compare(x, other.x);
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "{x=" + x + ", y=" + y + ", z=" + z + "}";
    }
}
