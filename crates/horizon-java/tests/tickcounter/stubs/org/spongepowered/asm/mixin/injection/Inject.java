package org.spongepowered.asm.mixin.injection;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/** Compile-time stand-in for Mixin's annotation (its name and members only). */
@Target(ElementType.METHOD)
@Retention(RetentionPolicy.CLASS)
public @interface Inject {
    String[] method() default {};

    At[] at();

    boolean cancellable() default false;
}
