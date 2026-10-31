package org.spongepowered.asm.mixin.injection.callback;

/** What a handler of a void method is handed: Rusty Minecraft's own, with Mixin's names. */
public class CallbackInfo implements Cancellable {
    private final String name;
    private final boolean cancellable;
    private boolean cancelled;

    public CallbackInfo(String name, boolean cancellable) {
        this.name = name;
        this.cancellable = cancellable;
    }

    /** The name of the method injected into. */
    public String getId() {
        return name;
    }

    @Override
    public boolean isCancellable() {
        return cancellable;
    }

    @Override
    public boolean isCancelled() {
        return cancelled;
    }

    @Override
    public void cancel() throws CancellationException {
        if (!cancellable) {
            throw new CancellationException("The call " + name + " is not cancellable.");
        }
        cancelled = true;
    }

    @Override
    public String toString() {
        return "CallbackInfo[" + name + ", cancellable=" + cancellable + ", cancelled=" + cancelled + "]";
    }
}
