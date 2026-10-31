package net.fabricmc.api;

/** A Fabric mod's client entry point ({@code entrypoints.client}). */
@FunctionalInterface
public interface ClientModInitializer {
    void onInitializeClient();
}
