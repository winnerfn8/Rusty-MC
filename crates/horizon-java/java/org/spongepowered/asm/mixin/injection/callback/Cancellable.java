package org.spongepowered.asm.mixin.injection.callback;

/** Something a handler can cancel. */
public interface Cancellable {
    boolean isCancellable();

    boolean isCancelled();

    void cancel() throws CancellationException;
}
