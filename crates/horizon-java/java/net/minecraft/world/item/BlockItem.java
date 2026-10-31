package net.minecraft.world.item;

import net.minecraft.world.level.block.Block;

/** The item that places a block. */
public class BlockItem extends Item {
    private final Block block;

    public BlockItem(Block block, Item.Properties properties) {
        super(properties);
        this.block = block;
    }

    public Block getBlock() {
        return block;
    }

    @Override
    public String getDescriptionId() {
        return block.getDescriptionId();
    }

    @Override
    public String rm$describe() {
        return super.rm$describe() + (block.rm$id() == null ? "" : "block=" + block.rm$id() + "\n");
    }
}
