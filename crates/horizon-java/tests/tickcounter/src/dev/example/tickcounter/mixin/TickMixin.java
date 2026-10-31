package dev.example.tickcounter.mixin;

import java.util.function.BooleanSupplier;
import net.minecraft.server.MinecraftServer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Counts server ticks and reads the server's own tick count through a {@code @Shadow}; cancels
 * the tick while {@link #cancel} is set. Abstract, as most mixins are.
 */
@Mixin(MinecraftServer.class)
public abstract class TickMixin {
    public static int ticks;
    public static int seen = -1;
    public static boolean isServer;
    public static boolean cancel;

    @Shadow
    public abstract int getTickCount();

    @Inject(method = "tickServer", at = @At("HEAD"), cancellable = true)
    private void tickcounter$onTick(BooleanSupplier hasTimeLeft, CallbackInfo ci) {
        ticks++;
        seen = getTickCount();
        isServer = ((Object) this) instanceof MinecraftServer && hasTimeLeft.getAsBoolean();
        if (cancel) {
            ci.cancel();
        }
    }
}
