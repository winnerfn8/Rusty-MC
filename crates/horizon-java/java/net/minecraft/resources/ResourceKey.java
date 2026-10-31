package net.minecraft.resources;

import net.minecraft.core.Registry;

/** An id inside a registry: {@code ResourceKey[minecraft:block / ruby:ruby_block]}. */
public final class ResourceKey<T> {
    private final Identifier registryName;
    private final Identifier identifier;

    private ResourceKey(Identifier registryName, Identifier identifier) {
        this.registryName = registryName;
        this.identifier = identifier;
    }

    public static <T> ResourceKey<T> create(ResourceKey<? extends Registry<T>> registry, Identifier id) {
        return new ResourceKey<>(registry.identifier, id);
    }

    public static <T> ResourceKey<Registry<T>> createRegistryKey(Identifier name) {
        return new ResourceKey<>(Identifier.withDefaultNamespace("root"), name);
    }

    public Identifier identifier() {
        return identifier;
    }

    /** The id's name before 26.x ({@code ResourceKey.location}). */
    public Identifier location() {
        return identifier;
    }

    public Identifier registry() {
        return registryName;
    }

    public boolean isFor(ResourceKey<? extends Registry<?>> registry) {
        return registryName.equals(registry.identifier);
    }

    @Override
    public String toString() {
        return "ResourceKey[" + registryName + " / " + identifier + "]";
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ResourceKey<?> other && registryName.equals(other.registryName) && identifier.equals(other.identifier);
    }

    @Override
    public int hashCode() {
        return 31 * registryName.hashCode() + identifier.hashCode();
    }
}
