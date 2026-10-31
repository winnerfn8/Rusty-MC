package net.minecraft.world.level.block.state;

import dev.rustyminecraft.java.Host;
import net.minecraft.world.level.block.Block;

/** One state of a block, by the game's state id (the block's default state until it has one). */
public class BlockState {
    private final Block block;
    private final int state;

    public BlockState(Block block, int state) {
        this.block = block;
        this.state = state;
    }

    public Block getBlock() {
        return block;
    }

    public boolean is(Block other) {
        return other != null && java.util.Objects.equals(block.rm$id(), other.rm$id());
    }

    public boolean isAir() {
        String path = block.rm$id() == null ? "" : block.rm$id().getPath();
        return block.rm$id() != null && "minecraft".equals(block.rm$id().getNamespace()) && (path.equals("air") || path.equals("cave_air") || path.equals("void_air"));
    }

    /** The game's id of this state (-1 when the game doesn't know the block). */
    public int rm$state() {
        return state >= 0 || block.rm$id() == null ? state : Host.defaultState(block.rm$id().toString());
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof BlockState s && s.is(block) && s.rm$state() == rm$state();
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hashCode(block.rm$id()) * 31 + state;
    }

    @Override
    public String toString() {
        return "Block{" + block.rm$id() + "}";
    }
}
