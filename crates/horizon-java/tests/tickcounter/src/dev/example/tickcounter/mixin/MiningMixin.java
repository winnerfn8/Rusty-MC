package dev.example.tickcounter.mixin;

import com.llamalad7.mixinextras.injector.ModifyReturnValue;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** Mines twice as fast: doubles the mining progress per tick, and notes what was mined. */
@Mixin(BlockBehaviour.class)
public class MiningMixin {
    public static String last = "";

    @ModifyReturnValue(method = "getDestroyProgress", at = @At("RETURN"))
    private float tickcounter$faster(float original, BlockState state, Player player, BlockGetter level, BlockPos pos) {
        Block self = (Block) (Object) this;
        last = self + " " + state.getBlock() + " at " + pos.toShortString() + " by " + player.getName().getString()
                + ", above " + level.getBlockState(pos.below()).getBlock();
        return original * 2f;
    }
}
