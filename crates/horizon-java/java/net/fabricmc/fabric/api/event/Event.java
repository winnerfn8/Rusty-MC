package net.fabricmc.fabric.api.event;

import net.minecraft.resources.Identifier;

/**
 * An event: listeners register, and whoever fires it calls {@link #invoker}. Phases are accepted
 * and listeners run in registration order.
 */
public abstract class Event<T> {
    public static final Identifier DEFAULT_PHASE = Identifier.fromNamespaceAndPath("fabric", "default");

    protected volatile T invoker;

    public final T invoker() {
        return invoker;
    }

    public abstract void register(T listener);

    public void register(Identifier phase, T listener) {
        register(listener);
    }

    public void addPhaseOrdering(Identifier firstPhase, Identifier secondPhase) {}
}
