package dev.rustyminecraft.java.fabric;

import java.nio.file.Path;
import java.util.Set;
import net.fabricmc.api.EnvType;
import net.fabricmc.loader.api.FabricLoader;

/** {@link FabricLoader}'s answers, set by Rust before any entry point runs. */
public final class Loader implements FabricLoader {
    public static final Loader INSTANCE = new Loader();

    private volatile Path gameDir = Path.of(".");
    private volatile Set<String> mods = Set.of("minecraft", "java", "fabricloader");
    private volatile boolean client;

    private Loader() {}

    /** The game folder, the ids of the mods loaded and the side (from Rust). */
    public static void init(String gameDir, String[] mods, boolean client) {
        INSTANCE.gameDir = Path.of(gameDir).toAbsolutePath();
        java.util.HashSet<String> ids = new java.util.HashSet<>(Set.of(mods));
        ids.add("minecraft");
        ids.add("java");
        ids.add("fabricloader");
        INSTANCE.mods = Set.copyOf(ids);
        INSTANCE.client = client;
    }

    @Override
    public boolean isModLoaded(String id) {
        return mods.contains(id);
    }

    @Override
    public Path getGameDir() {
        return gameDir;
    }

    @Override
    public Path getConfigDir() {
        return gameDir.resolve("config");
    }

    @Override
    public EnvType getEnvironmentType() {
        return client ? EnvType.CLIENT : EnvType.SERVER;
    }

    @Override
    public boolean isDevelopmentEnvironment() {
        return false;
    }
}
