package net.minecraft.network.chat;

/** Text shown to players. The Java layer keeps it plain: styles are accepted and not shown. */
public interface Component {
    /** The text, translation keys left as they are. */
    String getString();

    MutableComponent copy();

    static MutableComponent literal(String text) {
        return new MutableComponent(text);
    }

    static MutableComponent translatable(String key) {
        return new MutableComponent(key);
    }

    static MutableComponent translatable(String key, Object... args) {
        StringBuilder b = new StringBuilder(key);
        for (Object a : args) {
            b.append(' ').append(a instanceof Component c ? c.getString() : String.valueOf(a));
        }
        return new MutableComponent(b.toString());
    }

    static MutableComponent empty() {
        return new MutableComponent("");
    }

    static Component nullToEmpty(String text) {
        return text == null ? empty() : literal(text);
    }
}
