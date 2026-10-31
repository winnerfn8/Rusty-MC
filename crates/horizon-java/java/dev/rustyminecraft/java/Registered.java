package dev.rustyminecraft.java;

import net.minecraft.resources.Identifier;

/** A value a {@link GameRegistry} holds: it learns its id, and describes itself to the game. */
public interface Registered {
    void rm$registered(Identifier id);

    Identifier rm$id();

    /** What the game applies to the mod's content, as {@code key=value} lines. */
    String rm$describe();
}
