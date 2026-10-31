package net.minecraft.world.item;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;

/** Vanilla items mods name most often; each stands for the game's own. */
public final class Items {
    private Items() {}

    private static Item vanilla(String path) {
        return BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(path));
    }

    public static final Item AIR = vanilla("air");
    public static final Item STICK = vanilla("stick");
    public static final Item COAL = vanilla("coal");
    public static final Item IRON_INGOT = vanilla("iron_ingot");
    public static final Item GOLD_INGOT = vanilla("gold_ingot");
    public static final Item DIAMOND = vanilla("diamond");
    public static final Item EMERALD = vanilla("emerald");
    public static final Item REDSTONE = vanilla("redstone");
    public static final Item APPLE = vanilla("apple");
    public static final Item BREAD = vanilla("bread");
    public static final Item WHEAT = vanilla("wheat");
    public static final Item BUCKET = vanilla("bucket");
    public static final Item WATER_BUCKET = vanilla("water_bucket");
    public static final Item BOWL = vanilla("bowl");
    public static final Item STRING = vanilla("string");
    public static final Item BONE = vanilla("bone");
    public static final Item IRON_PICKAXE = vanilla("iron_pickaxe");
    public static final Item DIAMOND_SWORD = vanilla("diamond_sword");
}
