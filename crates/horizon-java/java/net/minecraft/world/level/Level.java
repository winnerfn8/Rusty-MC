package net.minecraft.world.level;

import dev.rustyminecraft.java.Host;
import dev.rustyminecraft.java.Objects;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.block.state.BlockState;

/**
 * A level (a dimension), by the game's handle. Only the level a hook point runs in is at hand:
 * reading another answers air.
 */
public class Level implements BlockGetter {
    public static final ResourceKey<Level> OVERWORLD = ResourceKey.create(Registries.DIMENSION, Identifier.withDefaultNamespace("overworld"));
    public static final ResourceKey<Level> NETHER = ResourceKey.create(Registries.DIMENSION, Identifier.withDefaultNamespace("the_nether"));
    public static final ResourceKey<Level> END = ResourceKey.create(Registries.DIMENSION, Identifier.withDefaultNamespace("the_end"));

    /** The field before it became a method; kept for mods that read it. */
    public boolean isClientSide;
    protected int rm$level;

    public void rm$bind(long handle) {
        rm$level = (int) handle;
        isClientSide = Host.isClient();
    }

    public int rm$level() {
        return rm$level;
    }

    public boolean isClientSide() {
        return Host.isClient();
    }

    public ResourceKey<Level> dimension() {
        String name = Host.levelName(rm$level);
        return name == null || name.isEmpty() ? OVERWORLD : ResourceKey.create(Registries.DIMENSION, Identifier.parse(name));
    }

    public long getGameTime() {
        return Host.levelTime(rm$level, false);
    }

    public long getDayTime() {
        return Host.levelTime(rm$level, true);
    }

    public MinecraftServer getServer() {
        return Host.isClient() ? null : MinecraftServer.rm$instance();
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        return Objects.blockState(Host.blockState(rm$level, pos.getX(), pos.getY(), pos.getZ()));
    }

    public boolean isEmptyBlock(BlockPos pos) {
        return getBlockState(pos).isAir();
    }

    /** Sets a block with its updates; {@code flags} are the game's to decide. */
    public boolean setBlock(BlockPos pos, BlockState state, int flags) {
        int id = state.rm$state();
        return id >= 0 && Host.setBlockState(rm$level, pos.getX(), pos.getY(), pos.getZ(), id);
    }

    public boolean setBlockAndUpdate(BlockPos pos, BlockState state) {
        return setBlock(pos, state, 3);
    }

    public boolean removeBlock(BlockPos pos, boolean moved) {
        return setBlock(pos, net.minecraft.world.level.block.Blocks.AIR.defaultBlockState(), 3);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Level l && l.rm$level == rm$level;
    }

    @Override
    public int hashCode() {
        return rm$level;
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "[" + Host.levelName(rm$level) + "]";
    }
}
