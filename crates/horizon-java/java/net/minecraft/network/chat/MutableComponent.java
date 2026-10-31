package net.minecraft.network.chat;

import net.minecraft.ChatFormatting;

/** A component that can be added to. */
public class MutableComponent implements Component {
    private final StringBuilder text;

    MutableComponent(String text) {
        this.text = new StringBuilder(text);
    }

    public MutableComponent append(String more) {
        text.append(more);
        return this;
    }

    public MutableComponent append(Component more) {
        text.append(more.getString());
        return this;
    }

    public MutableComponent withStyle(ChatFormatting format) {
        return this;
    }

    public MutableComponent withStyle(ChatFormatting... formats) {
        return this;
    }

    @Override
    public String getString() {
        return text.toString();
    }

    @Override
    public MutableComponent copy() {
        return new MutableComponent(text.toString());
    }

    @Override
    public String toString() {
        return "literal{" + text + "}";
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof MutableComponent c && c.getString().equals(getString());
    }

    @Override
    public int hashCode() {
        return getString().hashCode();
    }
}
