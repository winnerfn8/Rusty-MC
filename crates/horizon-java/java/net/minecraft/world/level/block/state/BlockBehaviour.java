package net.minecraft.world.level.block.state;

import java.util.function.ToIntFunction;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.block.Block;

/** What every block shares: its properties. */
public abstract class BlockBehaviour {
    protected final Properties properties;

    protected BlockBehaviour(Properties properties) {
        this.properties = properties;
    }

    public Properties properties() {
        return properties;
    }

    /** A block's properties, set by the mod that adds it. */
    public static class Properties {
        Float destroyTime;
        Float explosionResistance;
        ToIntFunction<BlockState> lightEmission;
        boolean requiresCorrectToolForDrops;
        Float friction;
        boolean hasCollision = true;
        boolean canOcclude = true;
        boolean isRandomlyTicking;
        boolean noLootTable;
        boolean replaceable;
        ResourceKey<Block> id;

        private Properties() {}

        public static Properties of() {
            return new Properties();
        }

        public static Properties ofFullCopy(BlockBehaviour block) {
            Properties p = new Properties();
            Properties o = block.properties;
            if (o != null) {
                p.destroyTime = o.destroyTime;
                p.explosionResistance = o.explosionResistance;
                p.lightEmission = o.lightEmission;
                p.requiresCorrectToolForDrops = o.requiresCorrectToolForDrops;
                p.friction = o.friction;
                p.hasCollision = o.hasCollision;
                p.canOcclude = o.canOcclude;
                p.isRandomlyTicking = o.isRandomlyTicking;
                p.replaceable = o.replaceable;
            }
            return p;
        }

        public static Properties ofLegacyCopy(BlockBehaviour block) {
            return ofFullCopy(block);
        }

        /** The id {@link #setId} gave, if any. */
        public net.minecraft.resources.Identifier rm$id() {
            return id == null ? null : id.identifier();
        }

        public Properties setId(ResourceKey<Block> id) {
            this.id = id;
            return this;
        }

        public Properties destroyTime(float time) {
            destroyTime = time;
            return this;
        }

        public Properties explosionResistance(float resistance) {
            explosionResistance = Math.max(0.0F, resistance);
            return this;
        }

        public Properties strength(float destroyTime, float explosionResistance) {
            return destroyTime(destroyTime).explosionResistance(explosionResistance);
        }

        public Properties strength(float strength) {
            return strength(strength, strength);
        }

        public Properties instabreak() {
            return strength(0.0F);
        }

        public Properties lightLevel(ToIntFunction<BlockState> light) {
            lightEmission = light;
            return this;
        }

        public Properties requiresCorrectToolForDrops() {
            requiresCorrectToolForDrops = true;
            return this;
        }

        public Properties friction(float friction) {
            this.friction = friction;
            return this;
        }

        public Properties noCollision() {
            hasCollision = false;
            canOcclude = false;
            return this;
        }

        /** {@link #noCollision}'s older spelling. */
        public Properties noCollission() {
            return noCollision();
        }

        public Properties noOcclusion() {
            canOcclude = false;
            return this;
        }

        public Properties randomTicks() {
            isRandomlyTicking = true;
            return this;
        }

        public Properties noLootTable() {
            noLootTable = true;
            return this;
        }

        public Properties replaceable() {
            replaceable = true;
            return this;
        }

        /** What the game applies to the mod's block (its content's {@code properties}). */
        public String rm$describe(BlockState defaultState) {
            StringBuilder b = new StringBuilder();
            if (destroyTime != null) {
                b.append("hardness=").append(destroyTime).append('\n');
            }
            if (explosionResistance != null) {
                b.append("explosion_resistance=").append(explosionResistance).append('\n');
            }
            if (lightEmission != null) {
                int light;
                try {
                    light = lightEmission.applyAsInt(defaultState);
                } catch (RuntimeException e) {
                    light = 0;
                }
                b.append("light=").append(Math.max(0, Math.min(15, light))).append('\n');
            }
            if (requiresCorrectToolForDrops) {
                b.append("requires_tool=true\n");
            }
            if (friction != null) {
                b.append("friction=").append(friction).append('\n');
            }
            if (!hasCollision) {
                b.append("collision=false\n");
            }
            return b.toString();
        }
    }
}
