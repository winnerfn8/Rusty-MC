package org.spongepowered.asm.mixin;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

/** Compile-time stand-in for Mixin's annotation (its name only). */
@Retention(RetentionPolicy.CLASS)
public @interface Shadow {}
