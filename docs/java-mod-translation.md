# Java mod translation

Rusty Minecraft runs no Java, so a Fabric, Quilt, NeoForge or Forge jar can't run as it is ([mods.md](mods.md)). The
translation layer gets the most out of one anyway: put the jar where Minecraft would want it, start the game, and
what the jar carries plays as a Rusty Minecraft content mod.

<p align="center">
  <img src="media/launcher-java-mods.webp" alt="The launcher's Settings, Java mods section: the Translate Java mods switch with Add mod jar, Translate now and Open folder, the switch for mods that only partly translate, the Run their code with Java switch with its full-access notice, the Java found and Choose Java, and translated jars with their verdicts and Code lines, the Tick Counter test mod's three supported mixins and its Fabric API line listed" width="90%">
</p>

## Using it

1. Put the jar in the instance's `mods/` folder (or `horizon-mods/`), as you would for Minecraft. Or, in the
   launcher, open **Settings > Java mods** and press **Add mod jar…**, or drop jars on that section.
2. Start the game. New and changed jars are translated before the game reads its mods, and a jar's mod is turned on
   the first time. After that, its switch (Options > Mods in the game, or the row in Settings > Java mods) is yours.
3. Remove the jar to remove its mod: **Remove** in Settings > Java mods moves the jar to the trash, or delete it by
   hand. The next start (or **Translate now**) uninstalls the mod.

Settings > Java mods lists every jar of the selected instance with its verdict:

| Verdict | Means |
|---|---|
| **Works** | Content only: its blocks, items, recipes, loot, tags, worldgen, models and textures all port. |
| **Partly** | Its content ports; some of what its code does doesn't (listed under *Missing*). Turned on only while **Turn on mods that only partly translate** is on (it is by default). |
| **Doesn't run** | Nothing to port: everything it does is Java code (a performance mod, a library, a minimap), or it was made for Minecraft 1.12 or older. |
| **Not installed** | It ports, but a mod with the same id is already in `horizon-mods/` (from the Mods page, by hand, or another jar of the same mod). The row says which. |
| **Failed** | The jar couldn't be read. |

**Translate Java mods** turns the translation at game start off; **Translate now** still runs it. A Java instance
(one that plays Minecraft: Java Edition with a loader) runs its Java mods as they are, so the section only says so.

**Run their code with Java (experimental)**, off by default, runs the entry points, supported mixins and Fabric
API listeners of mods made for 26.x on your own Java, with a part of vanilla's and Fabric API's API
([java-mod-runtime.md](java-mod-runtime.md)), in games built with the Java layer (the `java-mods` build option;
official builds don't have it yet). The row says that Java mods' code runs with full access to the computer, like
native mods, and shows the Java it would use (the newest found, or the one picked with **Choose Java…**; **Find
Java automatically** goes back to the newest). What a mod registers from Java (a block's hardness and light, an
item's stack size and rarity) then applies to its translated content.

## What is translated

The steps run on the player's own computer, on the jars they put there, and nothing is uploaded.

1. **Layout.** Jars made for Minecraft 1.13 to 1.20 name their data folders in the plural (`recipes/`,
   `loot_tables/`, `advancements/`, `structures/`, `functions/`, `tags/blocks/`, `tags/items/`...) and recipe
   results as `item` (or a bare id). The jar is staged with 1.21's names (`recipe/`, `loot_table/`, `tags/block/`,
   `"result": {"id": ...}`), which the carry-over reads. A jar that keeps a stale plural folder beside the new one keeps
   the new one.
2. **Mod id.** A content mod's id is `[a-z0-9_]{1,64}`. Fabric and Quilt allow `-` (`copper-lanterns`): the id
   becomes `copper_lanterns`, and the files and ids in the namespace `copper-lanterns` are renamed with it (paths,
   `copper-lanterns:` ids, `#copper-lanterns:` tags, translation keys such as `block.copper-lanterns.x`). A jar
   without metadata takes the namespace holding most of its files.
3. **Content.** The staged jar goes through the same carry-over and class reading as the launcher's mod browser
   (`horizon-port-pack check`, [modpacks.md](modpacks.md)): recipes upgraded to 26.3, item definitions written,
   each block given the closest vanilla template, and the classes read for what can't port (block entities,
   entities, menus, Mixins, event handlers...).
4. **Install.** The folder mod is copied into `horizon-mods/<id>/`, its manifest's description saying which jar
   and verdict it comes from (Options > Mods shows it), with a marker file `.java-translation.json` naming the jar.
   Only a folder holding that marker is ever replaced or removed: a mod from the Mods page or put there by hand is
   left alone.

Each jar's translation is kept by its SHA-1 in `.horizon-cache/java-mods/` in the game folder, so a jar is
translated once; `translations.json` beside the cache is what Settings > Java mods lists.

## What its code needs

The translation also reads what the jar's code would need to run in the optional Java layer
([java-mod-runtime.md](java-mod-runtime.md)), without running anything (`crates/horizon-port-pack/src/mixins.rs`).
Each row shows it as **Code:**, for example *4 of 9 mixins supported; 2 need more of the Java layer; needs GeckoLib
(no Rust version)*, and **Show each mixin and library** lists them, coloured by group:

- **Mixins.** The mixin configs the metadata names (`mixins` in `fabric.mod.json`, `[[mixins]]` in
  `neoforge.mods.toml`, `mixin` in `quilt.mod.json`, Forge's `MixinConfigs` manifest line), and in each mixin class
  the annotations `@Mixin`, `@Inject` (its `at` and `cancellable`), `@Redirect`, `@ModifyArg`, `@ModifyVariable`,
  `@ModifyConstant`, `@Overwrite`, `@Accessor`, `@Invoker`, `@Shadow`, `@Unique` and MixinExtras' `@WrapOperation`,
  `@ModifyExpressionValue`, `@ModifyReturnValue`, `@WrapWithCondition`. Each falls in one group, with its reason:
  - *supported* (green): a start, end or cancel injection (`@Inject` at `HEAD`, `RETURN` or `TAIL`;
    `@ModifyReturnValue`) on a vanilla method with a mixin hook point there (`native-hooks.json` entries with a
    `vanilla` method), a `@Shadow` of a method the Java API layer has, and the mod's own `@Unique` members;
  - *needs more of the Java layer* (yellow): `@Accessor`, `@Invoker`, a `@Shadow` field, and a `@Shadow` of a
    method the Java API layer doesn't have yet;
  - *not supported* (red): everything else, said why: a change inside a method (`@Redirect`, `@ModifyArg`,
    `@ModifyVariable`, `@ModifyConstant`, `@WrapOperation`...), a whole method replaced (`@Overwrite`), a method with
    no hook point yet, a class that isn't vanilla's.
- **Libraries.** Every required mod and library: loader APIs (Fabric API and its modules, NeoForge, QSL) and
  libraries that touch Minecraft (Architectury, GeckoLib, Cloth Config...) run only once this repository has a Rust
  version of them, and the row says which have one: Fabric API's `fabric-api-base` and `fabric-lifecycle-events-v1`
  so far. Jars bundled inside (`META-INF/jars/`, `META-INF/jarjar/`) whose classes never name Minecraft or a loader
  are plain Java and run as they are.
- **The Java API layer.** For a 26.x mod, every class of the jar is read for what it uses of vanilla's, Fabric's
  and Mixin's API: what the Java layer doesn't have yet is counted (*uses 14 vanilla or Fabric API classes or
  members the Java layer doesn't have yet*) and the first twelve listed (`SoundType`,
  `BlockBehaviour.Properties.sound()`), with the Java its classes need (25 for 26.x).
- **OpenGL.** 26.x draws through Blaze3D (`RenderPipeline`, `RenderType`) and Rusty Minecraft through wgpu: a
  jar calling `org.lwjgl.opengl` itself says so.

Only mods made for 26.x get code support: 26.x ships unobfuscated, so their mixins name vanilla's real classes and
methods. An older mod (its metadata names 1.x, or its classes use intermediary or SRG names) keeps the content-only
translation, and its **Code:** line says why. `horizon-port-pack check <jar> <out>` and `horizon-port-pack translate
<game folder>` print the same lines.

## Files

All in the game folder (the one holding `options.txt`):

| File | What |
|---|---|
| `mods/*.jar`, `horizon-mods/*.jar` | The Java jars the player put there (`.jar.disabled` ones are skipped). |
| `horizon-mods/<id>/` | A translated mod, with `.java-translation.json`. |
| `horizon-mods.txt` | The game's on-list (`mod.<id>:true`), shared with every other content mod. |
| `horizon-java-mods.txt` | `translate:true` (translate at game start) and `partial:true` (turn on mods that partly translate), both `true` by default; `code:false` (run their supported code in the Java layer) and `java:<path>` (the Java to run it on; none finds one). |
| `.horizon-cache/java-mods/` | `translations.json` and the cache of each jar's translation. Safe to delete. |

## From the command line

```sh
horizon-port-pack translate ~/.local/share/horizon      # the default instance's game folder on Linux
```

It does what the game does at start and prints each jar's verdict and state.

## Limits

- Java code doesn't run, except 26.x mods' entry points, supported mixins and Fabric API listeners in the optional
  Java layer ([java-mod-runtime.md](java-mod-runtime.md)).
  A machine's block is placed and mined like its template, but it doesn't process anything; new mobs, dimensions,
  keybinds, screens and most mixins are not there. The verdict lists what is missing.
- Jars made before 1.13 (`mcmod.info`, `textures/blocks/`) are not translated.
- Libraries a mod needs aren't required: their code wouldn't run either. A content add-on whose recipes use another
  mod's items needs that mod's jar translated too.
- Two jars of the same mod: the one installed before stays (else the first by file path), and the other says why it
is not installed.

The code: `crates/horizon-port-pack/src/translate.rs` (the layer and `horizon-port-pack translate`),
`crates/horizon-client/src/game/mods.rs` (`translate_java_mods`, run at game start) and `launcher/src/java_mods.rs`
(Settings > Java mods).
