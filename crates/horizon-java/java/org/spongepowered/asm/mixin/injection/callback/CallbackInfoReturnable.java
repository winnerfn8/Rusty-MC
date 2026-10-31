package org.spongepowered.asm.mixin.injection.callback;

/** What a handler of a method with a return value is handed. */
public class CallbackInfoReturnable<R> extends CallbackInfo {
    private R returnValue;

    public CallbackInfoReturnable(String name, boolean cancellable) {
        super(name, cancellable);
    }

    public CallbackInfoReturnable(String name, boolean cancellable, R returnValue) {
        super(name, cancellable);
        this.returnValue = returnValue;
    }

    /** Returns from the method with this value (cancelling it, so the injection must be cancellable). */
    public void setReturnValue(R returnValue) {
        super.cancel();
        this.returnValue = returnValue;
    }

    public R getReturnValue() {
        return returnValue;
    }

    public boolean getReturnValueZ() {
        return returnValue != null && (Boolean) returnValue;
    }

    public float getReturnValueF() {
        return returnValue == null ? 0f : (Float) returnValue;
    }

    public int getReturnValueI() {
        return returnValue == null ? 0 : (Integer) returnValue;
    }
}
