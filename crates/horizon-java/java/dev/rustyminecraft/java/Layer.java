package dev.rustyminecraft.java;

import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;

/** The Java layer's helpers, called from Rust through JNI (crates/horizon-java/src/jvm.rs). */
public final class Layer {
    private Layer() {}

    /** A class loader over the mod jars, below the one holding these classes. */
    public static ClassLoader loader(String[] jars) throws Exception {
        URL[] urls = new URL[jars.length];
        for (int i = 0; i < jars.length; i++) {
            urls[i] = new File(jars[i]).toURI().toURL();
        }
        return new URLClassLoader("rusty-minecraft-mods", urls, Layer.class.getClassLoader());
    }

    /** A class file's bytes as the loader finds it (`a/b/C`), or null. */
    public static byte[] classBytes(ClassLoader loader, String name) throws Exception {
        try (InputStream in = loader.getResourceAsStream(name + ".class")) {
            return in == null ? null : in.readAllBytes();
        }
    }

    /**
     * Runs a mod's entry point: {@code spec} is a class (made with its no-argument constructor) or
     * {@code Class::field}; {@code kind} is {@code main}, {@code client} or {@code server}. Null when
     * it ran, else why not.
     */
    public static String entrypoint(ClassLoader loader, String spec, String kind) {
        try {
            Object target;
            int at = spec.indexOf("::");
            if (at >= 0) {
                Class<?> owner = Class.forName(spec.substring(0, at), true, loader);
                target = owner.getField(spec.substring(at + 2)).get(null);
            } else {
                target = Class.forName(spec, true, loader).getDeclaredConstructor().newInstance();
            }
            switch (kind) {
                case "main" -> ((net.fabricmc.api.ModInitializer) target).onInitialize();
                case "client" -> ((net.fabricmc.api.ClientModInitializer) target).onInitializeClient();
                case "server" -> ((net.fabricmc.api.DedicatedServerModInitializer) target).onInitializeServer();
                default -> {
                    return "unknown entry point kind " + kind;
                }
            }
            return null;
        } catch (java.lang.reflect.InvocationTargetException e) {
            return String.valueOf(e.getCause());
        } catch (Throwable e) {
            return String.valueOf(e);
        }
    }

    /** Does nothing: the Rust-to-Java measurement calls it. */
    public static void empty() {}

    /** A call into Rust that does nothing (registered by jvm.rs). */
    public static native void rust();

    /** Calls {@link #rust} {@code n} times; the nanoseconds it took. */
    public static long callRust(int n) {
        long start = System.nanoTime();
        for (int i = 0; i < n; i++) {
            rust();
        }
        return System.nanoTime() - start;
    }
}
