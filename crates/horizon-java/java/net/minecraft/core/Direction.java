package net.minecraft.core;

/** The six directions of a block's faces. */
public enum Direction {
    DOWN(0, -1, 0),
    UP(0, 1, 0),
    NORTH(0, 0, -1),
    SOUTH(0, 0, 1),
    WEST(-1, 0, 0),
    EAST(1, 0, 0);

    private final int stepX;
    private final int stepY;
    private final int stepZ;

    Direction(int stepX, int stepY, int stepZ) {
        this.stepX = stepX;
        this.stepY = stepY;
        this.stepZ = stepZ;
    }

    public int getStepX() {
        return stepX;
    }

    public int getStepY() {
        return stepY;
    }

    public int getStepZ() {
        return stepZ;
    }

    public Direction getOpposite() {
        return switch (this) {
            case DOWN -> UP;
            case UP -> DOWN;
            case NORTH -> SOUTH;
            case SOUTH -> NORTH;
            case WEST -> EAST;
            case EAST -> WEST;
        };
    }

    public int get3DDataValue() {
        return ordinal();
    }

    public String getName() {
        return name().toLowerCase(java.util.Locale.ROOT);
    }

    public String getSerializedName() {
        return getName();
    }

    /** By name ({@code north}), or null. */
    public static Direction byName(String name) {
        for (Direction d : values()) {
            if (d.getName().equals(name)) {
                return d;
            }
        }
        return null;
    }
}
