package net.minecraft.core;

import java.util.Optional;
import java.util.Set;
import java.util.stream.Stream;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;

/** A registry: ids to values. Mods add theirs with {@link #register}. */
public interface Registry<T> extends Iterable<T> {
    ResourceKey<? extends Registry<T>> key();

    Identifier getKey(T value);

    Optional<ResourceKey<T>> getResourceKey(T value);

    T getValue(Identifier id);

    T getValue(ResourceKey<T> key);

    Optional<T> getOptional(Identifier id);

    T getValueOrThrow(ResourceKey<T> key);

    boolean containsKey(Identifier id);

    Set<Identifier> keySet();

    Stream<T> stream();

    int size();

    static <T> T register(Registry<? super T> registry, String name, T value) {
        return register(registry, Identifier.parse(name), value);
    }

    @SuppressWarnings("unchecked")
    static <V, T extends V> T register(Registry<V> registry, Identifier id, T value) {
        ((WritableRegistry<V>) registry).register(ResourceKey.create(registry.key(), id), value);
        return value;
    }

    @SuppressWarnings("unchecked")
    static <V, T extends V> T register(Registry<V> registry, ResourceKey<V> key, T value) {
        ((WritableRegistry<V>) registry).register(key, value);
        return value;
    }
}
