package net.fabricmc.loader.api;

import java.nio.file.Path;
import net.fabricmc.api.EnvType;

/** What a Fabric mod asks its loader: the mods loaded, the game's folders, the side. */
public interface FabricLoader {
    static FabricLoader getInstance() {
        return dev.rustyminecraft.java.fabric.Loader.INSTANCE;
    }

    boolean isModLoaded(String id);

    Path getGameDir();

    Path getConfigDir();

    EnvType getEnvironmentType();

    boolean isDevelopmentEnvironment();
}
