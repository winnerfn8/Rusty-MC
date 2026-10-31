package net.minecraft.world.item;

import dev.rustyminecraft.java.Registered;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ItemLike;

/**
 * An item. A mod's item registered in {@code BuiltInRegistries.ITEM} applies its properties to
 * the item of its translated content mod; a vanilla one stands for the game's own.
 */
public class Item implements ItemLike, Registered {
    private final Properties properties;
    private Identifier rm$id;

    public Item(Properties properties) {
        this.properties = properties;
        if (properties != null && properties.id != null) {
            rm$id = properties.id.identifier();
        }
    }

    @Override
    public Item asItem() {
        return this;
    }

    @Override
    public void rm$registered(Identifier id) {
        rm$id = id;
    }

    @Override
    public Identifier rm$id() {
        return rm$id;
    }

    @Override
    public String rm$describe() {
        StringBuilder b = new StringBuilder();
        if (properties != null) {
            if (properties.maxStackSize != null) {
                b.append("max_stack_size=").append(properties.maxStackSize).append('\n');
            }
            if (properties.maxDamage != null) {
                b.append("durability=").append(properties.maxDamage).append('\n');
            }
            if (properties.fireResistant) {
                b.append("fire_resistant=true\n");
            }
            if (properties.rarity != null) {
                b.append("rarity=").append(properties.rarity.name().toLowerCase(java.util.Locale.ROOT)).append('\n');
            }
            if (properties.craftRemainder != null && properties.craftRemainder.rm$id() != null) {
                b.append("crafting_remainder=").append(properties.craftRemainder.rm$id()).append('\n');
            }
        }
        return b.toString();
    }

    public ItemStack getDefaultInstance() {
        return new ItemStack(this);
    }

    public int getDefaultMaxStackSize() {
        return properties == null || properties.maxStackSize == null ? (properties != null && properties.maxDamage != null ? 1 : 64) : properties.maxStackSize;
    }

    public String getDescriptionId() {
        return rm$id == null ? "item.unregistered" : rm$id.toLanguageKey("item");
    }

    public Component getName(ItemStack stack) {
        return Component.translatable(getDescriptionId());
    }

    @Override
    public String toString() {
        return rm$id == null ? "unregistered_item" : rm$id.toString();
    }

    /** An item's properties, set by the mod that adds it. */
    public static class Properties {
        Integer maxStackSize;
        Integer maxDamage;
        boolean fireResistant;
        Rarity rarity;
        Item craftRemainder;
        ResourceKey<Item> id;

        public Properties() {}

        public Properties setId(ResourceKey<Item> id) {
            this.id = id;
            return this;
        }

        public Properties stacksTo(int max) {
            maxStackSize = max;
            return this;
        }

        public Properties durability(int max) {
            maxDamage = max;
            maxStackSize = 1;
            return this;
        }

        public Properties fireResistant() {
            fireResistant = true;
            return this;
        }

        public Properties rarity(Rarity rarity) {
            this.rarity = rarity;
            return this;
        }

        public Properties craftRemainder(Item item) {
            craftRemainder = item;
            return this;
        }

        public Properties useBlockDescriptionPrefix() {
            return this;
        }

        public Properties useItemDescriptionPrefix() {
            return this;
        }
    }
}
