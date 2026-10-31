package org.spongepowered.asm.mixin.injection;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

/** Compile-time stand-in for Mixin's annotation (its name and members only). */
@Retention(RetentionPolicy.CLASS)
public @interface At {
    String value();
}
