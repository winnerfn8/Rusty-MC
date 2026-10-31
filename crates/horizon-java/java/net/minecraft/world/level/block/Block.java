package net.minecraft.world.level.block;

import dev.rustyminecraft.java.Host;
import dev.rustyminecraft.java.Registered;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;

/**
 * A block. A mod's block registered in {@code BuiltInRegistries.BLOCK} becomes a block of its
 * translated content mod with these properties; a vanilla one stands for the game's own. A mixin
 * of a block class gets the block the hook point is about as {@code this}.
 */
public class Block extends BlockBehaviour implements ItemLike, Registered {
    private Identifier rm$id;
    private BlockState rm$default;

    public Block(BlockBehaviour.Properties properties) {
        super(properties);
        if (properties != null && properties.rm$id() != null) {
            rm$id = properties.rm$id();
        }
    }

    /** A mixin instance standing for the block of a state. */
    public void rm$bind(long state) {
        String name = Host.stateBlock((int) state);
        rm$id = name == null ? null : Identifier.tryParse(name);
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
        return properties == null ? "" : properties.rm$describe(defaultBlockState());
    }

    public BlockState rm$state(int state) {
        return state < 0 ? defaultBlockState() : new BlockState(this, state);
    }

    public final BlockState defaultBlockState() {
        if (rm$default == null) {
            rm$default = new BlockState(this, -1);
        }
        return rm$default;
    }

    @Override
    public Item asItem() {
        return rm$id == null ? BuiltInRegistries.ITEM.getValue((Identifier) null) : BuiltInRegistries.ITEM.getValue(rm$id);
    }

    public String getDescriptionId() {
        return rm$id == null ? "block.unregistered" : rm$id.toLanguageKey("block");
    }

    public MutableComponent getName() {
        return Component.translatable(getDescriptionId());
    }

    @Override
    public String toString() {
        return "Block{" + rm$id + "}";
    }
}
