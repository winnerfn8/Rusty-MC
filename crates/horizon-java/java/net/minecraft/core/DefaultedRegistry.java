package net.minecraft.core;

import net.minecraft.resources.Identifier;

/** A registry that answers its default value for an unknown id. */
public interface DefaultedRegistry<T> extends Registry<T> {
    Identifier getDefaultKey();
}
