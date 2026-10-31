package net.minecraft.resources;

/** A namespaced id, {@code namespace:path} (the default namespace is {@code minecraft}). */
public final class Identifier implements Comparable<Identifier> {
    public static final char NAMESPACE_SEPARATOR = ':';
    public static final String DEFAULT_NAMESPACE = "minecraft";

    private final String namespace;
    private final String path;

    private Identifier(String namespace, String path) {
        if (!isValidNamespace(namespace)) {
            throw new IllegalArgumentException("Non [a-z0-9_.-] character in namespace of id: " + namespace + ":" + path);
        }
        if (!isValidPath(path)) {
            throw new IllegalArgumentException("Non [a-z0-9/._-] character in path of id: " + namespace + ":" + path);
        }
        this.namespace = namespace;
        this.path = path;
    }

    public static Identifier fromNamespaceAndPath(String namespace, String path) {
        return new Identifier(namespace, path);
    }

    public static Identifier withDefaultNamespace(String path) {
        return new Identifier(DEFAULT_NAMESPACE, path);
    }

    /** {@code ns:path}, or {@code path} in the default namespace. */
    public static Identifier parse(String id) {
        int at = id.indexOf(NAMESPACE_SEPARATOR);
        return at < 0 ? new Identifier(DEFAULT_NAMESPACE, id) : new Identifier(at == 0 ? DEFAULT_NAMESPACE : id.substring(0, at), id.substring(at + 1));
    }

    /** {@link #parse}, or null when it isn't a valid id. */
    public static Identifier tryParse(String id) {
        try {
            return parse(id);
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    public static Identifier tryBuild(String namespace, String path) {
        return isValidNamespace(namespace) && isValidPath(path) ? new Identifier(namespace, path) : null;
    }

    public static boolean isValidNamespace(String namespace) {
        for (int i = 0; i < namespace.length(); i++) {
            char c = namespace.charAt(i);
            if (!(c == '_' || c == '-' || c == '.' || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9'))) {
                return false;
            }
        }
        return true;
    }

    public static boolean isValidPath(String path) {
        for (int i = 0; i < path.length(); i++) {
            char c = path.charAt(i);
            if (!(c == '_' || c == '-' || c == '.' || c == '/' || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9'))) {
                return false;
            }
        }
        return true;
    }

    public String getNamespace() {
        return namespace;
    }

    public String getPath() {
        return path;
    }

    public Identifier withPath(String path) {
        return new Identifier(namespace, path);
    }

    public Identifier withPrefix(String prefix) {
        return withPath(prefix + path);
    }

    public Identifier withSuffix(String suffix) {
        return withPath(path + suffix);
    }

    /** {@code type.namespace.path}, with the path's slashes as dots: {@code block.minecraft.stone}. */
    public String toLanguageKey(String type) {
        return type + "." + namespace + "." + path.replace('/', '.');
    }

    public String toLanguageKey() {
        return namespace + "." + path.replace('/', '.');
    }

    public String toShortLanguageKey() {
        return DEFAULT_NAMESPACE.equals(namespace) ? path : toLanguageKey();
    }

    @Override
    public String toString() {
        return namespace + NAMESPACE_SEPARATOR + path;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Identifier other && namespace.equals(other.namespace) && path.equals(other.path);
    }

    @Override
    public int hashCode() {
        return 31 * namespace.hashCode() + path.hashCode();
    }

    @Override
    public int compareTo(Identifier other) {
        int c = path.compareTo(other.path);
        return c != 0 ? c : namespace.compareTo(other.namespace);
    }
}
