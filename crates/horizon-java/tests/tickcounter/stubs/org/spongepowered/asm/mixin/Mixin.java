package org.spongepowered.asm.mixin;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/** Compile-time stand-in for Mixin's annotation (its name and members only). */
@Target(ElementType.TYPE)
@Retention(RetentionPolicy.CLASS)
public @interface Mixin {
    Class<?>[] value() default {};

    String[] targets() default {};
}
