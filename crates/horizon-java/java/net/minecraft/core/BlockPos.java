package net.minecraft.core;

import net.minecraft.world.phys.Vec3;

/** A block's position. Packed into a long as 26 bits of x, 26 of z and 12 of y. */
public class BlockPos extends Vec3i {
    public static final BlockPos ZERO = new BlockPos(0, 0, 0);

    private static final int PACKED_XZ = 26;
    private static final int PACKED_Y = 12;
    private static final long MASK_XZ = (1L << PACKED_XZ) - 1;
    private static final long MASK_Y = (1L << PACKED_Y) - 1;
    private static final int Z_OFFSET = PACKED_Y;
    private static final int X_OFFSET = PACKED_Y + PACKED_XZ;

    public BlockPos(int x, int y, int z) {
        super(x, y, z);
    }

    public BlockPos(Vec3i v) {
        this(v.getX(), v.getY(), v.getZ());
    }

    public static BlockPos containing(double x, double y, double z) {
        return new BlockPos((int) Math.floor(x), (int) Math.floor(y), (int) Math.floor(z));
    }

    public static long asLong(int x, int y, int z) {
        return ((x & MASK_XZ) << X_OFFSET) | ((z & MASK_XZ) << Z_OFFSET) | (y & MASK_Y);
    }

    public static int getX(long packed) {
        return (int) (packed << (64 - X_OFFSET - PACKED_XZ) >> (64 - PACKED_XZ));
    }

    public static int getY(long packed) {
        return (int) (packed << (64 - PACKED_Y) >> (64 - PACKED_Y));
    }

    public static int getZ(long packed) {
        return (int) (packed << (64 - Z_OFFSET - PACKED_XZ) >> (64 - PACKED_XZ));
    }

    public static BlockPos of(long packed) {
        return new BlockPos(getX(packed), getY(packed), getZ(packed));
    }

    public long asLong() {
        return asLong(getX(), getY(), getZ());
    }

    public BlockPos offset(int dx, int dy, int dz) {
        return dx == 0 && dy == 0 && dz == 0 ? this : new BlockPos(getX() + dx, getY() + dy, getZ() + dz);
    }

    public BlockPos offset(Vec3i v) {
        return offset(v.getX(), v.getY(), v.getZ());
    }

    public BlockPos subtract(Vec3i v) {
        return offset(-v.getX(), -v.getY(), -v.getZ());
    }

    public BlockPos multiply(int scale) {
        return new BlockPos(getX() * scale, getY() * scale, getZ() * scale);
    }

    public BlockPos relative(Direction d) {
        return offset(d.getStepX(), d.getStepY(), d.getStepZ());
    }

    public BlockPos relative(Direction d, int n) {
        return offset(d.getStepX() * n, d.getStepY() * n, d.getStepZ() * n);
    }

    public BlockPos above() {
        return relative(Direction.UP);
    }

    public BlockPos above(int n) {
        return relative(Direction.UP, n);
    }

    public BlockPos below() {
        return relative(Direction.DOWN);
    }

    public BlockPos below(int n) {
        return relative(Direction.DOWN, n);
    }

    public BlockPos north() {
        return relative(Direction.NORTH);
    }

    public BlockPos north(int n) {
        return relative(Direction.NORTH, n);
    }

    public BlockPos south() {
        return relative(Direction.SOUTH);
    }

    public BlockPos south(int n) {
        return relative(Direction.SOUTH, n);
    }

    public BlockPos west() {
        return relative(Direction.WEST);
    }

    public BlockPos west(int n) {
        return relative(Direction.WEST, n);
    }

    public BlockPos east() {
        return relative(Direction.EAST);
    }

    public BlockPos east(int n) {
        return relative(Direction.EAST, n);
    }

    public BlockPos immutable() {
        return this;
    }

    public Vec3 getCenter() {
        return Vec3.atCenterOf(this);
    }

    public Vec3 getBottomCenter() {
        return Vec3.atBottomCenterOf(this);
    }
}
