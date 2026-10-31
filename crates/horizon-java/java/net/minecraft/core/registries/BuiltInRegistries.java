package net.minecraft.core.registries;

import dev.rustyminecraft.java.GameRegistry;
import net.minecraft.core.DefaultedRegistry;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;

/**
 * The registries a mod adds blocks and items to. A vanilla id the registry hasn't seen yet answers
 * a value made on the spot that stands for the game's own (its behaviour is the game's); a mod's
 * registration is also handed to the game (crates/horizon-java/src/jvm.rs), which applies it to
 * the mod's translated content.
 */
public final class BuiltInRegistries {
    private BuiltInRegistries() {}

    public static final DefaultedRegistry<Block> BLOCK = new GameRegistry<>(Registries.BLOCK, Identifier.withDefaultNamespace("air"), id -> new Block(BlockBehaviour.Properties.of()));
    public static final DefaultedRegistry<Item> ITEM = new GameRegistry<>(Registries.ITEM, Identifier.withDefaultNamespace("air"), id -> new Item(new Item.Properties()));
}
