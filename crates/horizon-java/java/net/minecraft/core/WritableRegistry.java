package net.minecraft.core;

import net.minecraft.resources.ResourceKey;

/** A registry values can be added to. */
public interface WritableRegistry<T> extends Registry<T> {
    T register(ResourceKey<T> key, T value);
}
