//! Mods in the game (`docs/mods.md`): content mods' blocks, items, data and
//! assets (`content_root`), and the Mods page's rows. Code mods are native
//! (`native_hooks.rs`); WebAssembly code mods don't run any more.
use super::Game;
use horizon_client::{
    mod_list::{self, Installed},
    text::Text,
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ModsState {
    /// `--dev-mod PATH`: always on.
    pub(super) dev: Vec<PathBuf>,
    /// The WebAssembly mods found that no longer run, for the one-time notice.
    stopped: Vec<String>,
}

/// The Java mod jars the player put in the game folder (`mods/`, `horizon-mods/`) translated into
/// content mods in `horizon-mods/` (docs/java-mod-translation.md): a new jar's mod is turned on,
/// a removed jar's goes. Before the mods' switches are read. Off in the launcher's Settings > Java
/// mods (`horizon-java-mods.txt`).
pub(super) fn translate_java_mods(home: &std::path::Path) {
    use horizon_port_pack::translate;
    if !translate::Settings::load(home).translate {
        return;
    }
    let log = |line: String| log::info!("Horizon: Java mod {line}");
    match translate::sync(home, &log) {
        Ok(out) if !out.translated.is_empty() || !out.removed.is_empty() => log::info!(
            "Horizon: Java mods: {} translated, {} removed, {} turned on",
            out.translated.len(),
            out.removed.len(),
            out.enabled.len()
        ),
        Ok(_) => {}
        Err(e) => log::warn!("Horizon: Java mods not translated: {e}"),
    }
}

/// The Java layer (docs/java-mod-runtime.md), when Settings > Java mods > Run their code is on: a
/// JVM from the player's own Java loads the translated mods that are on (and code-only jars, which
/// have no content mod to switch), runs their entry points (what they register from Java applies
/// to their translated content before the game reads it), and their supported injections and
/// Fabric API listeners run from the hook points. Only games built with the `java-mods` feature
/// have it; others say so in the log.
pub(super) fn start_java_layer(home: &std::path::Path, dedicated: bool) {
    let settings = horizon_port_pack::translate::Settings::load(home);
    if !settings.code {
        return;
    }
    java_layer(home, &settings, dedicated);
}

#[cfg(not(feature = "java-mods"))]
fn java_layer(_: &std::path::Path, _: &horizon_port_pack::translate::Settings, _: bool) {
    log::warn!(
        "Horizon: Java mods' code is on in Settings, but this build has no Java layer (cargo feature java-mods)"
    );
}

#[cfg(feature = "java-mods")]
fn java_layer(
    home: &std::path::Path,
    settings: &horizon_port_pack::translate::Settings,
    dedicated: bool,
) {
    use horizon_java::{
        fabric::Entrypoint,
        hooks::{Hook, Side},
        jvm, locate,
    };
    use horizon_port_pack::{check, jar, mixins, translate};
    let libjvm = match locate::libjvm(&settings.java) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Horizon: Java layer not started: {e}");
            return;
        }
    };
    let java = locate::home_of(&libjvm).and_then(|h| locate::version_of(&h));
    let mut mods = jvm::Mods {
        game_dir: home.to_path_buf(),
        dedicated,
        ..jvm::Mods::default()
    };
    // The mods that are on and made for 26.x (their classes name vanilla's real classes), with
    // what their code needs.
    let mut candidates = Vec::new();
    for e in translate::entries(home) {
        // A mod with content is on by its switch; a code-only mod has no content mod to switch,
        // so its jar in the folder is what turns it on (as in Minecraft).
        let on = match e.state {
            translate::State::Installed => translate::is_enabled(home, &e.mod_id),
            translate::State::NotInstalled => e.verdict == Some(check::Verdict::No),
            translate::State::Failed => false,
        };
        if !on {
            continue;
        }
        let path = home.join(&e.jar);
        let read = || -> Result<(jar::JarInfo, mixins::Report), String> {
            let info = jar::read(&path)?;
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
            let report = mixins::read(&mut zip, &info, check::loader(&names));
            Ok((info, report))
        };
        match read() {
            Ok((info, report)) if matches!(report.era, mixins::Era::V26(_)) => {
                candidates.push((e.jar.clone(), path, info, report));
            }
            Ok(_) => {}
            Err(why) => log::warn!("Horizon: Java layer: {}: {why}", e.jar),
        }
    }
    let ids: Vec<String> = candidates.iter().map(|c| c.2.mod_id.clone()).collect();
    for (jar_name, path, info, report) in candidates {
        // A mod needs a Java as new as its classes, and every library it requires: a Rust
        // version of it, or another mod that loads here.
        if let Some(v) = java
            && report.java > v
        {
            log::warn!(
                "Horizon: Java layer: {jar_name} needs Java {} and {} is Java {v}: its code doesn't run",
                report.java,
                libjvm.display()
            );
            continue;
        }
        let missing: Vec<&str> = report
            .missing_libraries()
            .into_iter()
            .filter(|l| !(l.kind == mixins::LibraryKind::Mod && ids.contains(&l.id)))
            .map(|l| l.name.as_str())
            .collect();
        if !missing.is_empty() {
            log::warn!(
                "Horizon: Java layer: {jar_name} needs {} (no Rust version): its code doesn't run",
                missing.join(", ")
            );
            continue;
        }
        mods.mod_ids.push(info.mod_id.clone());
        for (kind, spec) in &info.entrypoints {
            mods.entrypoints.push(Entrypoint {
                jar: path.clone(),
                mod_id: info.mod_id.clone(),
                kind: kind.clone(),
                spec: spec.clone(),
            });
        }
        for i in &report.injections {
            if i.group != mixins::Group::Supported {
                continue;
            }
            let Some(hook) = Hook::from_name(&i.hook) else {
                continue;
            };
            mods.bindings.push(jvm::Binding {
                hook,
                jar: path.clone(),
                class: i.mixin.replace('.', "/"),
                target: i.target.clone(),
                member: i.member.clone(),
                descriptor: i.descriptor.clone(),
                is_static: i.is_static,
                cancellable: i.cancellable,
                method: i.method.clone(),
                modifies: i.kind == "ModifyReturnValue",
                side: match i.side.as_str() {
                    "client" => Some(Side::Client),
                    "server" => Some(Side::Server),
                    _ => None,
                },
            });
        }
    }
    if mods.bindings.is_empty() && mods.entrypoints.is_empty() {
        log::info!("Horizon: Java layer: no code to run in the mods that are on");
        return;
    }
    let started = std::time::Instant::now();
    let result = jvm::start(&libjvm).and_then(|vm| {
        let (layer, problems) = jvm::Layer::load(vm, &mods)?;
        for p in problems {
            log::warn!("Horizon: Java layer: {p}");
        }
        let n = layer.handlers();
        layer.install();
        Ok(n)
    });
    match result {
        Ok(n) => log::info!(
            "Horizon: Java layer: {n} handlers bound and {} entry points run on {} in {:.0} ms",
            mods.entrypoints
                .iter()
                .filter(|e| e.runs(dedicated))
                .count(),
            libjvm.display(),
            started.elapsed().as_secs_f64() * 1e3
        ),
        Err(e) => {
            log::warn!("Horizon: Java layer not started: {e}");
            return;
        }
    }
    // What the entry points registered becomes the translated content's, before it is read.
    let registered: Vec<translate::Registered> = jvm::take_registrations()
        .into_iter()
        .map(|r| translate::Registered {
            registry: r.registry,
            id: r.id,
            properties: r.properties,
        })
        .collect();
    for line in translate::apply_registrations(home, &registered) {
        log::info!("Horizon: Java layer: registered {line}");
    }
}

/// `root` with the content mods that are on layered over it
/// (docs/modpacks.md): their blocks, items, data and assets. Read once at
/// start, before the resources load; a mod whose content is refused is left
/// out here and its row on the Mods page says why.
pub(super) fn content_root(
    root: &std::path::Path,
    enabled: &std::collections::BTreeSet<String>,
    dev: &[std::path::PathBuf],
) -> std::path::PathBuf {
    use horizon_client::mod_content::{self, ContentMod};
    use horizon_mods::{Capability, Content, Package};
    let here = std::env::current_dir().unwrap_or_default();
    let mut paths = horizon_mods::find(&here.join(mod_list::DIR));
    paths.retain(|p| !dev.contains(p));
    paths.extend(dev.iter().cloned());
    let mut mods: Vec<ContentMod> = Vec::new();
    for path in &paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        let on = enabled.contains(&m.id) || dev.contains(path);
        if !on
            || !m.capabilities.contains(&Capability::Registries)
            || mods.iter().any(|c| c.id == m.id)
        {
            continue;
        }
        let content = Content::read(path, m).and_then(|content| {
            // The first of two mods owning one namespace stays on.
            let own = (m.id.as_str(), content.namespaces.as_slice());
            match mods
                .iter()
                .find_map(|c| horizon_mods::content::clash((&c.id, &c.content.namespaces), own))
            {
                Some(e) => Err(e),
                None => Ok(content),
            }
        });
        match content {
            Ok(content) => mods.push(ContentMod {
                id: m.id.clone(),
                content,
            }),
            Err(e) => log::warn!("Horizon: mod {} content not loaded: {e}", m.id),
        }
    }
    match mod_content::resource_root(root, &mods, &here.join(".horizon-cache")) {
        Ok(out) => {
            if out != root {
                log::info!(
                    "Horizon: content from {} mod(s) in {}",
                    mods.len(),
                    out.display()
                );
            }
            register_mob_effects(&mods);
            out
        }
        Err(e) => {
            log::warn!("Horizon: content mods not loaded: {e}");
            register_mob_effects(&[]);
            root.to_owned()
        }
    }
}

/// The mods' mob effects join vanilla's (`brewing::effect`), once the root
/// holding their network ids is built. The startup root's effects, none
/// included, stay until restart: a pack reload after a Mods switch builds a
/// root with others.
fn register_mob_effects(mods: &[horizon_client::mod_content::ContentMod]) {
    use horizon_client::brewing::{self, Category};
    use horizon_mods::content::EffectCategory;
    let effects = horizon_client::mod_content::mob_effects(mods);
    let ids: Vec<String> = effects.iter().map(|e| e.id.clone()).collect();
    let set = brewing::register_mod_effects(effects.into_iter().map(|e| {
        let category = match e.category {
            EffectCategory::Beneficial => Category::Beneficial,
            EffectCategory::Harmful => Category::Harmful,
            EffectCategory::Neutral => Category::Neutral,
        };
        (e.id.clone(), e.color, category)
    }));
    if !set
        && brewing::mod_effects()
            .iter()
            .map(|e| e.id)
            .ne(ids.iter().map(String::as_str))
    {
        log::warn!("Horizon: the mods' mob effects changed; restart the game to use them");
    }
}

/// What the launcher writes beside a game it built with native mods: their ids under `mods`.
const BUILD_INFO: &str = "native-mods.json";

/// A row for each native mod the launcher installed in `dir` (`<id>.hnmod`) that this game wasn't
/// built with (`built`: its [`BUILD_INFO`], empty for a release or Nightly) and that has no row
/// yet. The page said "No mods" before, and players copied the files into `horizon-mods` (#4770).
/// Its toggle is the `mod.<id>` line the launcher's next build follows.
fn not_built(dir: &std::path::Path, built: &str, rows: &[Installed]) -> Vec<Installed> {
    let info: serde_json::Value = serde_json::from_str(built).unwrap_or_default();
    let in_build = |id: &str| {
        info["mods"]
            .as_array()
            .is_some_and(|mods| mods.iter().any(|m| m["id"] == id))
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "hnmod"));
    paths.sort();
    let mut out: Vec<Installed> = Vec::new();
    for path in paths {
        let Ok(bundle) = horizon_mods::index::native::Bundle::read(&path) else {
            continue;
        };
        let m = bundle.manifest;
        let listed = rows.iter().chain(&out).any(|row| row.id == m.id);
        if !listed && !in_build(&m.id) {
            out.push(Installed {
                title: format!("{} {}", m.id, m.version),
                id: m.id,
                description: String::new(),
                problem: Some(mod_list::NOT_BUILT.to_owned()),
                dev: false,
                native: None,
            });
        }
    }
    out
}

/// A WebAssembly code mod's row on the Mods page.
const STOPPED: &str = "Doesn't run any more: needs a native port";
/// The player was told once about the WebAssembly mods that stopped (`horizon-mods/<this>`).
const NOTICE_SEEN: &str = ".native-notice";

impl Game<'_> {
    /// Lists the installed mods on the Mods page. Content mods load with the resources
    /// (`content_root`); native ones are built into the game (`native_hooks.rs`). WebAssembly code
    /// mods don't run any more: their rows say so, and the player hears it once in the chat
    /// (`tell_native_now`).
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let mut paths = horizon_mods::find(&dir);
        paths.retain(|p| !self.mods.dev.contains(p));
        paths.extend(self.mods.dev.iter().cloned());
        self.settings.mods.installed = paths
            .iter()
            .map(|path| {
                let file = path.file_name().unwrap_or_default().to_string_lossy();
                let package = match horizon_mods::Package::read(path) {
                    Ok(package) => package,
                    Err(e) => {
                        return Installed {
                            id: String::new(),
                            title: file.into_owned(),
                            description: String::new(),
                            problem: Some(format!("Can't be read: {e}")),
                            dev: false,
                            native: None,
                        };
                    }
                };
                let m = &package.manifest;
                let problem = package.wasm.is_some().then(|| STOPPED.to_owned());
                Installed {
                    id: m.id.clone(),
                    title: format!("{} {}", m.name, m.version),
                    description: m.description.clone(),
                    problem,
                    dev: self.mods.dev.contains(path),
                    native: None,
                }
            })
            .collect();
        // A mod with a native version built in says so instead, and isn't news.
        self.list_native_mods();
        let built = std::env::current_exe()
            .ok()
            .and_then(|exe| std::fs::read_to_string(exe.with_file_name(BUILD_INFO)).ok())
            .unwrap_or_default();
        let native_dir = Self::options_path().with_file_name(mod_list::NATIVE_DIR);
        let unbuilt = not_built(&native_dir, &built, &self.settings.mods.installed);
        self.settings.mods.installed.extend(unbuilt);
        let installed = &self.settings.mods.installed;
        self.mods.stopped = installed
            .iter()
            .filter(|row| row.problem.as_deref() == Some(STOPPED))
            .map(|row| row.title.clone())
            .collect();
    }
    /// Once, in the first world after the update: which WebAssembly mods stopped.
    pub(super) fn tell_native_now(&mut self) {
        if self.mods.stopped.is_empty() || self.player.is_none() {
            return;
        }
        let names = std::mem::take(&mut self.mods.stopped).join(", ");
        let seen = Self::options_path()
            .with_file_name(mod_list::DIR)
            .join(NOTICE_SEEN);
        if seen.exists() {
            return;
        }
        self.system_message(Text::literal(format!(
            "Rusty Minecraft mods are native now: {names} stopped running until they have a native version. The launcher's Mods page has the native ones."
        )));
        let _ = std::fs::write(&seen, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal `.hnmod` the registry accepts: its manifest and its mixin's.
    fn hnmod(dir: &std::path::Path, id: &str) {
        let horizon = "a".repeat(40);
        let manifest = serde_json::json!({
            "schema": 1, "id": id, "version": "0.3.0", "game": "26.3",
            "horizon": horizon, "mixins": "b".repeat(40),
        });
        let mixin = format!(
            "[mod]\nid = \"{id}\"\nversion = \"0.3.0\"\n[compatibility]\nhorizon_revision = \"{horizon}\"\n"
        );
        let file = std::fs::File::create(dir.join(format!("{id}.hnmod"))).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("horizon-native.json", options).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file(format!("mixins/{id}/horizon-mixin.toml"), options)
            .unwrap();
        zip.write_all(mixin.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn native_mods_installed_but_not_built_get_a_row_saying_so() {
        let dir = std::env::temp_dir().join(format!("horizon-not-built-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for id in ["veinminer", "zoom", "minimap"] {
            hnmod(&dir, id);
        }
        std::fs::write(dir.join("notes.txt"), "not a mod").unwrap();
        std::fs::write(dir.join("broken.hnmod"), "not a zip").unwrap();
        // A release: none is built in; the page lists them, sorted, instead of "No mods".
        let rows = not_built(&dir, "", &[]);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["minimap", "veinminer", "zoom"]);
        assert_eq!(rows[1].title, "veinminer 0.3.0");
        assert_eq!(rows[1].problem.as_deref(), Some(mod_list::NOT_BUILT));
        assert!(rows.iter().all(|r| r.native.is_none() && !r.dev));
        // A native build with zoom, and minimap's row already listed by its own hook.
        let built =
            r#"{"base":"nightly-2026-10-30-1da354b","mods":[{"id":"zoom","version":"0.3.0"}]}"#;
        let listed = Installed {
            id: "minimap".into(),
            ..rows[0].clone()
        };
        let rows = not_built(&dir, built, &[listed]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "veinminer");
        // No folder: no rows.
        assert!(not_built(&dir.join("missing"), "", &[]).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
