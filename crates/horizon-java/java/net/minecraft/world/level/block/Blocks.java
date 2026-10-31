package net.minecraft.world.level.block;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;

/** Vanilla blocks mods name most often; each stands for the game's own. */
public final class Blocks {
    private Blocks() {}

    private static Block vanilla(String path) {
        return BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(path));
    }

    public static final Block AIR = vanilla("air");
    public static final Block CAVE_AIR = vanilla("cave_air");
    public static final Block STONE = vanilla("stone");
    public static final Block DEEPSLATE = vanilla("deepslate");
    public static final Block GRASS_BLOCK = vanilla("grass_block");
    public static final Block DIRT = vanilla("dirt");
    public static final Block COBBLESTONE = vanilla("cobblestone");
    public static final Block OAK_PLANKS = vanilla("oak_planks");
    public static final Block OAK_LOG = vanilla("oak_log");
    public static final Block OAK_LEAVES = vanilla("oak_leaves");
    public static final Block BEDROCK = vanilla("bedrock");
    public static final Block WATER = vanilla("water");
    public static final Block LAVA = vanilla("lava");
    public static final Block SAND = vanilla("sand");
    public static final Block GRAVEL = vanilla("gravel");
    public static final Block GLASS = vanilla("glass");
    public static final Block COAL_ORE = vanilla("coal_ore");
    public static final Block IRON_ORE = vanilla("iron_ore");
    public static final Block GOLD_ORE = vanilla("gold_ore");
    public static final Block DIAMOND_ORE = vanilla("diamond_ore");
    public static final Block IRON_BLOCK = vanilla("iron_block");
    public static final Block GOLD_BLOCK = vanilla("gold_block");
    public static final Block DIAMOND_BLOCK = vanilla("diamond_block");
    public static final Block OBSIDIAN = vanilla("obsidian");
    public static final Block TORCH = vanilla("torch");
    public static final Block CHEST = vanilla("chest");
    public static final Block CRAFTING_TABLE = vanilla("crafting_table");
    public static final Block FURNACE = vanilla("furnace");
    public static final Block FARMLAND = vanilla("farmland");
    public static final Block WHEAT = vanilla("wheat");
    public static final Block ICE = vanilla("ice");
    public static final Block SNOW = vanilla("snow");
    public static final Block NETHERRACK = vanilla("netherrack");
    public static final Block END_STONE = vanilla("end_stone");
}
