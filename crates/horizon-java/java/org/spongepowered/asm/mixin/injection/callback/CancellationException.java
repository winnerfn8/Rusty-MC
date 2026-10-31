package org.spongepowered.asm.mixin.injection.callback;

/** Thrown when a handler cancels an injection that isn't cancellable. */
public class CancellationException extends RuntimeException {
    public CancellationException(String message) {
        super(message);
    }
}
