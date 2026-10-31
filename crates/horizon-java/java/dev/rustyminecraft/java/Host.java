package dev.rustyminecraft.java;

/**
 * The game, as the Java API layer reaches it: native methods registered by
 * crates/horizon-java/src/jvm.rs, answered by the game's {@code horizon_java::hooks::Host}
 * while a hook point runs. Outside a hook point (a mod's entry point at start) there is no game:
 * they answer nothing (0, -1, null, false). Handles are the game's: levels, entity ids, block
 * state ids.
 */
public final class Host {
    private Host() {}

    /** Whether a hook point is running on this thread (the methods below answer only then). */
    public static native boolean inGame();

    public static native boolean isClient();

    public static native int tickCount();

    public static native int[] levels();

    public static native int currentLevel();

    public static native String levelName(int level);

    /** The level's game time, or its day time. */
    public static native long levelTime(int level, boolean day);

    /** The block state id at a position, or -1. */
    public static native int blockState(int level, int x, int y, int z);

    public static native boolean setBlockState(int level, int x, int y, int z, int state);

    /** A block state's block, {@code minecraft:stone}, or null. */
    public static native String stateBlock(int state);

    /** A block's default state id, or -1. */
    public static native int defaultState(String block);

    public static native int[] players();

    /** An entity's type, {@code minecraft:zombie}, or null when there is none with that id. */
    public static native String entityKind(int id);

    public static native int entityLevel(int id);

    /** An entity's coordinate on an axis (0 x, 1 y, 2 z). */
    public static native double entityPos(int id, int axis);

    /** Its health, or its maximum health; NaN for an entity that isn't living. */
    public static native float entityHealth(int id, boolean max);

    public static native String entityName(int id);

    /** Bits: 1 alive, 2 a player, 4 in creative mode. */
    public static native int entityFlags(int id);

    public static native boolean setHealth(int id, float health);

    public static native void sendMessage(int player, String text, boolean overlay);

    public static native void broadcast(String text);
}
