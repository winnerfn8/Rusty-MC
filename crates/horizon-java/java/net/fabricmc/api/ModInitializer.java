package net.fabricmc.api;

/** A Fabric mod's main entry point ({@code entrypoints.main} in fabric.mod.json). */
@FunctionalInterface
public interface ModInitializer {
    void onInitialize();
}
