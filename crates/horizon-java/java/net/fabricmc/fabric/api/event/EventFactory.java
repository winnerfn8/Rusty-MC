package net.fabricmc.fabric.api.event;

import dev.rustyminecraft.java.fabric.ArrayBackedEvent;
import java.util.function.Function;
import net.minecraft.resources.Identifier;

/** Makes events whose invoker calls every listener. */
public final class EventFactory {
    private EventFactory() {}

    public static <T> Event<T> createArrayBacked(Class<? super T> type, Function<T[], T> invokerFactory) {
        return new ArrayBackedEvent<>(type, null, invokerFactory);
    }

    public static <T> Event<T> createArrayBacked(Class<T> type, T emptyInvoker, Function<T[], T> invokerFactory) {
        return new ArrayBackedEvent<>(type, emptyInvoker, invokerFactory);
    }

    public static <T> Event<T> createWithPhases(Class<? super T> type, Function<T[], T> invokerFactory, Identifier... defaultPhases) {
        return new ArrayBackedEvent<>(type, null, invokerFactory);
    }

    public static boolean isProfilingEnabled() {
        return false;
    }
}
