package com.llamalad7.mixinextras.injector;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import org.spongepowered.asm.mixin.injection.At;

/** Compile-time stand-in for MixinExtras' annotation (its name and members only). */
@Target(ElementType.METHOD)
@Retention(RetentionPolicy.CLASS)
public @interface ModifyReturnValue {
    String[] method();

    At[] at();
}
