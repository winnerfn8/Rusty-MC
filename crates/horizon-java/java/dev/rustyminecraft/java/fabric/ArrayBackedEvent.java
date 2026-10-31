package dev.rustyminecraft.java.fabric;

import java.lang.reflect.Array;
import java.util.function.Function;
import net.fabricmc.fabric.api.event.Event;

/** An event whose invoker calls its listeners, in registration order. */
public class ArrayBackedEvent<T> extends Event<T> {
    private final Class<? super T> type;
    private final T empty;
    private final Function<T[], T> factory;
    private T[] listeners;

    @SuppressWarnings("unchecked")
    public ArrayBackedEvent(Class<? super T> type, T empty, Function<T[], T> factory) {
        this.type = type;
        this.empty = empty;
        this.factory = factory;
        this.listeners = (T[]) Array.newInstance(type, 0);
        update();
    }

    private void update() {
        invoker = listeners.length == 0 && empty != null ? empty : factory.apply(listeners.clone());
    }

    @Override
    public synchronized void register(T listener) {
        if (listener == null) {
            throw new NullPointerException("Tried to register a null listener");
        }
        @SuppressWarnings("unchecked")
        T[] more = (T[]) Array.newInstance(type, listeners.length + 1);
        System.arraycopy(listeners, 0, more, 0, listeners.length);
        more[listeners.length] = listener;
        listeners = more;
        update();
    }

    public synchronized int listeners() {
        return listeners.length;
    }
}
