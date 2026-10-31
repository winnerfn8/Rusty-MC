package dev.rustyminecraft.java;

import java.util.function.BooleanSupplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

/** The Java objects a hook point hands a handler, made from the game's handles (called from Rust). */
public final class Objects {
    private Objects() {}

    public static final BooleanSupplier ALWAYS = () -> true;

    public static MinecraftServer server() {
        return MinecraftServer.rm$instance();
    }

    public static Level level(int level) {
        Level l = Host.isClient() ? new Level() : new ServerLevel();
        l.rm$bind(level);
        return l;
    }

    public static Entity entity(int id) {
        int flags = Host.entityFlags(id);
        Entity e;
        if ((flags & 2) != 0) {
            e = Host.isClient() ? new Player() : new ServerPlayer();
        } else if (!Float.isNaN(Host.entityHealth(id, true))) {
            e = new LivingEntity();
        } else {
            e = new Entity();
        }
        e.rm$bind(id);
        return e;
    }

    public static BlockPos blockPos(int x, int y, int z) {
        return new BlockPos(x, y, z);
    }

    /** The block state with this id (air when there is none). */
    public static BlockState blockState(int state) {
        return block(state).rm$state(state);
    }

    /** The block of the state with this id (air when there is none). */
    public static Block block(int state) {
        String name = state < 0 ? null : Host.stateBlock(state);
        Identifier id = name == null ? null : Identifier.tryParse(name);
        return id == null ? Blocks.AIR : BuiltInRegistries.BLOCK.getValue(id);
    }

    public static ItemStack stack(String item, int count) {
        Identifier id = Identifier.tryParse(item);
        return id == null || count <= 0 ? ItemStack.EMPTY : new ItemStack(BuiltInRegistries.ITEM.getValue(id), count);
    }

    public static InteractionHand hand(boolean main) {
        return main ? InteractionHand.MAIN_HAND : InteractionHand.OFF_HAND;
    }

    public static InteractionResult result(boolean consumed) {
        return consumed ? InteractionResult.SUCCESS : InteractionResult.PASS;
    }

    public static boolean consumed(Object result) {
        return result instanceof InteractionResult r && r.consumesAction();
    }
}
