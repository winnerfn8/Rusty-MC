package dev.example.tickcounter;

import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Rarity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;

/** The mod's entry point: a block and an item, and listeners for the server's ticks and start. */
public class TickCounter implements ModInitializer {
    public static final String MOD_ID = "tickcounter";
    public static int endTicks;
    public static int started;
    public static boolean loaded;
    public static Block RUBY_BLOCK;

    private static Identifier id(String path) {
        return Identifier.fromNamespaceAndPath(MOD_ID, path);
    }

    @Override
    public void onInitialize() {
        ResourceKey<Block> block = ResourceKey.create(Registries.BLOCK, id("ruby_block"));
        RUBY_BLOCK = Registry.register(BuiltInRegistries.BLOCK, block,
                new Block(BlockBehaviour.Properties.of().setId(block).strength(1.5F, 6.0F).lightLevel(state -> 7).requiresCorrectToolForDrops()));
        ResourceKey<Item> blockItem = ResourceKey.create(Registries.ITEM, id("ruby_block"));
        Registry.register(BuiltInRegistries.ITEM, blockItem, new BlockItem(RUBY_BLOCK, new Item.Properties().setId(blockItem)));
        ResourceKey<Item> ruby = ResourceKey.create(Registries.ITEM, id("ruby"));
        Registry.register(BuiltInRegistries.ITEM, ruby, new Item(new Item.Properties().setId(ruby).stacksTo(16).rarity(Rarity.RARE)));
        loaded = FabricLoader.getInstance().isModLoaded(MOD_ID) && FabricLoader.getInstance().isModLoaded("fabric-lifecycle-events-v1");

        ServerTickEvents.END_SERVER_TICK.register(server -> {
            endTicks++;
            if (server.getTickCount() % 20 == 0) {
                for (ServerPlayer player : server.getPlayerList().getPlayers()) {
                    player.sendSystemMessage(Component.literal("Tick " + server.getTickCount() + ", " + player.getName().getString()));
                }
            }
        });
        ServerLifecycleEvents.SERVER_STARTED.register(server -> started++);
    }
}
