package dev.rustyminecraft.java;

import java.util.Collections;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.function.Function;
import java.util.stream.Stream;
import net.minecraft.core.DefaultedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.WritableRegistry;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;

/**
 * A registry of the Java API layer: what mods register, and values standing for the game's own
 * vanilla ones, made when first asked for. Each registration is handed to the game.
 */
public final class GameRegistry<T extends Registered> implements DefaultedRegistry<T>, WritableRegistry<T> {
    private final ResourceKey<? extends Registry<T>> key;
    private final Identifier defaultKey;
    private final Function<Identifier, T> vanilla;
    private final Map<Identifier, T> entries = new LinkedHashMap<>();

    public GameRegistry(ResourceKey<? extends Registry<T>> key, Identifier defaultKey, Function<Identifier, T> vanilla) {
        this.key = key;
        this.defaultKey = defaultKey;
        this.vanilla = vanilla;
    }

    /** A registration, to the game (registered by jvm.rs). */
    private static native void rust(String registry, String id, String description);

    @Override
    public synchronized T register(ResourceKey<T> key, T value) {
        Identifier id = key.identifier();
        if (entries.containsKey(id) && entries.get(id) != value) {
            throw new IllegalStateException("Duplicate registration: " + key);
        }
        entries.put(id, value);
        value.rm$registered(id);
        if (!Identifier.DEFAULT_NAMESPACE.equals(id.getNamespace())) {
            rust(this.key.identifier().getPath(), id.toString(), value.rm$describe());
        }
        return value;
    }

    @Override
    public ResourceKey<? extends Registry<T>> key() {
        return key;
    }

    @Override
    public Identifier getDefaultKey() {
        return defaultKey;
    }

    @Override
    public synchronized Identifier getKey(T value) {
        Identifier id = value == null ? null : value.rm$id();
        return id != null ? id : defaultKey;
    }

    @Override
    public Optional<ResourceKey<T>> getResourceKey(T value) {
        Identifier id = value == null ? null : value.rm$id();
        return id == null ? Optional.empty() : Optional.of(ResourceKey.create(key, id));
    }

    @Override
    public synchronized T getValue(Identifier id) {
        if (id == null) {
            return getValue(defaultKey);
        }
        T value = entries.get(id);
        if (value == null && Identifier.DEFAULT_NAMESPACE.equals(id.getNamespace())) {
            value = vanilla.apply(id);
            entries.put(id, value);
            value.rm$registered(id);
        }
        return value != null ? value : getValue(defaultKey);
    }

    @Override
    public T getValue(ResourceKey<T> key) {
        return key == null ? getValue(defaultKey) : getValue(key.identifier());
    }

    @Override
    public synchronized Optional<T> getOptional(Identifier id) {
        return containsKey(id) ? Optional.of(getValue(id)) : Optional.empty();
    }

    @Override
    public T getValueOrThrow(ResourceKey<T> key) {
        if (!containsKey(key.identifier())) {
            throw new IllegalStateException("Missing key in " + this.key + ": " + key);
        }
        return getValue(key);
    }

    /** A mod's registered id, or a vanilla one (any id in {@code minecraft}: the game decides). */
    @Override
    public synchronized boolean containsKey(Identifier id) {
        return id != null && (entries.containsKey(id) || Identifier.DEFAULT_NAMESPACE.equals(id.getNamespace()));
    }

    @Override
    public synchronized Set<Identifier> keySet() {
        return Collections.unmodifiableSet(new java.util.LinkedHashSet<>(entries.keySet()));
    }

    @Override
    public synchronized Stream<T> stream() {
        return new java.util.ArrayList<>(entries.values()).stream();
    }

    @Override
    public synchronized int size() {
        return entries.size();
    }

    @Override
    public Iterator<T> iterator() {
        return stream().iterator();
    }

    @Override
    public String toString() {
        return "Registry[" + key.identifier() + "]";
    }
}
