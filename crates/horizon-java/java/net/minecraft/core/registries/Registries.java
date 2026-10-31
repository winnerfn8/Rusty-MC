package net.minecraft.core.registries;

import net.minecraft.core.Registry;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;

/** The registries' keys. */
public final class Registries {
    private Registries() {}

    public static final ResourceKey<Registry<Block>> BLOCK = ResourceKey.createRegistryKey(Identifier.withDefaultNamespace("block"));
    public static final ResourceKey<Registry<Item>> ITEM = ResourceKey.createRegistryKey(Identifier.withDefaultNamespace("item"));
    public static final ResourceKey<Registry<Level>> DIMENSION = ResourceKey.createRegistryKey(Identifier.withDefaultNamespace("dimension"));
}
