package net.fabricmc.api;

/** A Fabric mod's dedicated server entry point ({@code entrypoints.server}). */
@FunctionalInterface
public interface DedicatedServerModInitializer {
    void onInitializeServer();
}
