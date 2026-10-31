package net.minecraft.world.item;

import net.minecraft.world.level.ItemLike;

/** A stack of an item (its count; components aren't handed over yet). */
public final class ItemStack {
    public static final ItemStack EMPTY = new ItemStack((Item) null, 0);

    private final Item item;
    private int count;

    public ItemStack(ItemLike item) {
        this(item, 1);
    }

    public ItemStack(ItemLike item, int count) {
        this.item = item == null ? null : item.asItem();
        this.count = count;
    }

    public Item getItem() {
        return isEmpty() ? Items.AIR : item;
    }

    public boolean isEmpty() {
        return item == null || count <= 0 || (item.rm$id() != null && "minecraft:air".equals(item.rm$id().toString()));
    }

    public int getCount() {
        return isEmpty() ? 0 : count;
    }

    public void setCount(int count) {
        if (this != EMPTY) {
            this.count = count;
        }
    }

    public void grow(int n) {
        setCount(count + n);
    }

    public void shrink(int n) {
        grow(-n);
    }

    public boolean is(Item other) {
        return getItem() == other || (other != null && java.util.Objects.equals(getItem().rm$id(), other.rm$id()));
    }

    public int getMaxStackSize() {
        return getItem().getDefaultMaxStackSize();
    }

    public ItemStack copy() {
        return isEmpty() ? EMPTY : new ItemStack(item, count);
    }

    public ItemStack copyWithCount(int count) {
        return isEmpty() ? EMPTY : new ItemStack(item, count);
    }

    @Override
    public String toString() {
        return getCount() + " " + getItem();
    }
}
