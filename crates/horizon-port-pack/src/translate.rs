//! The Java mod translation layer (`docs/java-mod-translation.md`): the Fabric, Quilt, NeoForge
//! and Forge jars a player puts in the game folder, as they would for Minecraft, become content
//! mods the game loads. Horizon runs no Java, so a jar's code never runs; what it carries is
//! translated, in three steps:
//!
//! 1. **Layout.** Jars made for Minecraft 1.13 to 1.20 name their data folders in the plural
//!    (`recipes/`, `loot_tables/`, `tags/blocks/`) and their recipe results `item`: they are
//!    staged with 1.21's names, which carry-over reads. A mod id the game refuses (`my-mod`, as
//!    Fabric allows) becomes one it takes (`my_mod`), and the namespace of that id with it.
//! 2. **Content.** The staged jar is carried over and its classes read ([`crate::check`]):
//!    blocks, items, recipes, loot, tags, worldgen, models and textures port; what its code does
//!    is listed as what doesn't.
//! 3. **Install.** The folder mod is copied into `horizon-mods/<id>/` with a marker naming its
//!    jar ([`MARKER`]), and turned on in `horizon-mods.txt` the first time (a mod that only partly
//!    ports when [`Settings::partial`] allows). After that the player's switch stays. A jar
//!    removed takes its translation with it; a folder without the marker (the Mods page's, or the
//!    player's own) is never touched.
//!
//! [`sync`] does this for every jar of a game folder: the game runs it when it starts, the
//! launcher's Settings > Java mods when asked. Each jar's translation is kept by its SHA-1 under
//! the game folder's cache, so a jar is read once.
use crate::{
    check::{self, Check, Verdict},
    jar,
    pipeline::{Log, read_json, write_json},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// The game's content mod folder, beside `options.txt` (`horizon_client::mod_list::DIR`).
pub const MODS_DIR: &str = "horizon-mods";
/// The game's on-list for content mods (`horizon_client::mod_list::FILE`).
const MODS_LIST: &str = "horizon-mods.txt";
/// The folders a player puts Java jars in, beside `options.txt`: Minecraft's, and Horizon's own.
pub const JAR_DIRS: [&str; 2] = ["mods", MODS_DIR];
/// The translation's settings, beside `options.txt`.
pub const SETTINGS: &str = "horizon-java-mods.txt";
/// The translations and each jar's cached one, in the game folder's cache.
const STATE: &str = ".horizon-cache/java-mods";
const REGISTRY: &str = "translations.json";
/// Inside an installed folder mod: the jar it was translated from. Only a folder holding it is
/// the translation's to replace or remove.
pub const MARKER: &str = ".java-translation.json";
/// Changes when the translation does more or says it differently, so jars translated before are
/// translated again (2: the summaries name Rusty Minecraft).
const VERSION: u32 = 4;
/// Namespaces that are never a mod's own: vanilla's, the convention tags', the loaders'.
const NOT_OWN: [&str; 7] = [
    "minecraft",
    "c",
    "horizon",
    "forge",
    "neoforge",
    "fabric",
    "quilt",
];
/// Data folders 1.21 renamed to the singular: (before, after), under `data/<namespace>/`.
const LEGACY_FOLDERS: [(&str, &str); 13] = [
    ("recipes", "recipe"),
    ("loot_tables", "loot_table"),
    ("advancements", "advancement"),
    ("structures", "structure"),
    ("functions", "function"),
    ("item_modifiers", "item_modifier"),
    ("predicates", "predicate"),
    ("tags/blocks", "tags/block"),
    ("tags/items", "tags/item"),
    ("tags/fluids", "tags/fluid"),
    ("tags/entity_types", "tags/entity_type"),
    ("tags/game_events", "tags/game_event"),
    ("tags/functions", "tags/function"),
];
/// A jar entry bigger than this isn't staged (no mod's data file is near it).
const MAX_ENTRY: u64 = 64 << 20;

/// What the player chose, in [`SETTINGS`]: `translate:true` lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Translate the jars when the game starts.
    pub translate: bool,
    /// Turn on a mod that only partly ports (some of what its code does doesn't).
    pub partial: bool,
    /// Run the supported part of the mods' code in the Java layer (docs/java-mod-runtime.md),
    /// on the player's own Java. Off by default; only games built with the Java layer read it.
    pub code: bool,
    /// The Java to run it on (a JDK or JRE folder, its `java`, or its `libjvm`); `` finds one.
    pub java: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            translate: true,
            partial: true,
            code: false,
            java: String::new(),
        }
    }
}

impl Settings {
    pub fn load(home: &Path) -> Self {
        let mut settings = Settings::default();
        let text = fs::read_to_string(home.join(SETTINGS)).unwrap_or_default();
        for (key, value) in text.lines().filter_map(|l| l.split_once(':')) {
            let on = value.trim() == "true";
            match key.trim() {
                "translate" => settings.translate = on,
                "partial" => settings.partial = on,
                "code" => settings.code = on,
                "java" => settings.java = value.trim().to_owned(),
                _ => {}
            }
        }
        settings
    }

    pub fn save(&self, home: &Path) -> Result<(), String> {
        let path = home.join(SETTINGS);
        let mut text = format!(
            "translate:{}\npartial:{}\ncode:{}\n",
            self.translate, self.partial, self.code
        );
        if !self.java.is_empty() {
            text.push_str(&format!("java:{}\n", self.java));
        }
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Where a jar's translation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// In `horizon-mods/<id>/`.
    Installed,
    /// Read, but nothing of it is installed: the summary says why.
    NotInstalled,
    /// It couldn't be read: the summary says why.
    Failed,
}

/// One jar of the game folder and its translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The jar, from the game folder (`mods/ruby-1.2.jar`).
    pub jar: String,
    pub sha1: String,
    /// The content mod's id: the Java mod's, made one the game takes.
    pub mod_id: String,
    /// The Java mod's own id.
    pub java_id: String,
    pub name: String,
    pub version: String,
    /// `fabric`, `quilt`, `neoforge`, `forge`, or `` when the jar names none.
    pub loader: String,
    pub state: State,
    pub verdict: Option<Verdict>,
    /// One line: the verdict's reason, or why it isn't installed.
    pub summary: String,
    /// What ports: "12 blocks", "40 recipes"...
    pub ports: Vec<String>,
    /// What doesn't, with why.
    pub skipped: Vec<String>,
    /// What the translation changed: a 1.20 layout, a renamed id.
    pub notes: Vec<String>,
    /// What of its code the Java layer could run, in one line: "3 of 5 mixins
    /// supported; 1 needs more of the Java layer; needs GeckoLib (no Rust version)" (`` for a jar
    /// with no code to speak of).
    #[serde(default)]
    pub code: String,
    /// Each mixin with its group and why, then the libraries it needs.
    #[serde(default)]
    pub mixins: Vec<crate::mixins::Line>,
}

impl Entry {
    /// The name a list shows: the mod's, else its jar's file name.
    pub fn title(&self) -> &str {
        if self.name.is_empty() {
            self.jar.rsplit('/').next().unwrap_or(&self.jar)
        } else {
            &self.name
        }
    }

    /// The loader as players write it.
    pub fn loader_name(&self) -> &str {
        match self.loader.as_str() {
            "fabric" => "Fabric",
            "quilt" => "Quilt",
            "neoforge" => "NeoForge",
            "forge" => "Forge",
            _ => "Java",
        }
    }
}

/// Every jar's translation, as the last [`sync`] left them.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    pub entries: Vec<Entry>,
}

impl Registry {
    pub fn load(home: &Path) -> Self {
        read_json(&home.join(STATE).join(REGISTRY)).unwrap_or_default()
    }

    fn save(&self, home: &Path) -> Result<(), String> {
        let dir = home.join(STATE);
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        write_json(&dir.join(REGISTRY), self)
    }
}

/// The translations of the game folder at `home`, as the last [`sync`] left them.
pub fn entries(home: &Path) -> Vec<Entry> {
    Registry::load(home).entries
}

/// What a [`sync`] did.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Jars translated this time (new or changed), by file.
    pub translated: Vec<String>,
    /// Jars removed since the last time, by file; their mods were uninstalled with them.
    pub removed: Vec<String>,
    /// Mods turned on.
    pub enabled: Vec<String>,
}

/// The marker in an installed folder mod.
#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    jar: String,
    sha1: String,
}

fn marker(dir: &Path) -> Option<Marker> {
    read_json(&dir.join(MARKER)).ok()
}

/// A jar's translation, kept in the cache by its SHA-1.
#[derive(Debug, Serialize, Deserialize)]
struct Translation {
    version: u32,
    java_id: String,
    check: Check,
    notes: Vec<String>,
}

/// The Java jars of the game folder at `home` (not the turned-off `.jar.disabled` ones), as
/// (path from the game folder, path).
pub fn find(home: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    for dir in JAR_DIRS {
        let Ok(read) = fs::read_dir(home.join(dir)) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.to_ascii_lowercase().ends_with(".jar") && entry.path().is_file() {
                found.push((format!("{dir}/{name}"), entry.path()));
            }
        }
    }
    found.sort();
    found
}

/// Translates the Java jars of the game folder at `home` that are new or changed since the last
/// time, installs what ports and uninstalls the translations of jars removed. Writes nothing in
/// a game folder that never had a jar.
pub fn sync(home: &Path, log: Log) -> Result<Outcome, String> {
    let settings = Settings::load(home);
    let jars = find(home);
    let mut registry = Registry::load(home);
    let mut out = Outcome::default();
    if jars.is_empty() && registry.entries.is_empty() {
        return Ok(out);
    }
    // A jar gone takes its translation with it.
    for e in &registry.entries {
        if !jars.iter().any(|(rel, _)| *rel == e.jar) {
            log(if uninstall(home, e) {
                format!("{}: removed, and {} with it", e.jar, e.mod_id)
            } else {
                format!("{}: removed", e.jar)
            });
            out.removed.push(e.jar.clone());
        }
    }
    let mut entries: Vec<Entry> = Vec::new();
    for (rel, path) in &jars {
        let before = registry.entries.iter().find(|e| e.jar == *rel);
        let sha1 = match sha1_of(path) {
            Ok(h) => h,
            Err(e) => {
                // Unreadable for now (being copied, locked): what was installed from it stays.
                log(format!("{rel}: {e}"));
                entries.push(
                    before
                        .cloned()
                        .unwrap_or_else(|| failed(rel, String::new(), e)),
                );
                continue;
            }
        };
        // Unchanged, and installed as it was or with nothing to install: nothing to do. A jar
        // left out (another mod in its folder, the same mod twice) is tried again.
        let settled = |b: &&Entry| {
            b.sha1 == sha1
                && match b.state {
                    State::Installed => in_place(home, b),
                    State::NotInstalled => b.verdict == Some(Verdict::No),
                    State::Failed => false,
                }
        };
        if let Some(b) = before.filter(settled)
            && cached(home, &sha1).is_some()
        {
            entries.push(b.clone());
            continue;
        }
        let entry = translate_one(
            home, rel, path, &sha1, before, &settings, &entries, &mut out,
        );
        log(
            match entry.verdict.filter(|_| entry.state != State::Failed) {
                Some(v) => format!("{rel}: {} ({}). {}", v.label(), entry.mod_id, entry.summary),
                None => format!("{rel}: {}", entry.summary),
            },
        );
        out.translated.push(rel.clone());
        entries.push(entry);
    }
    registry.entries = entries;
    registry.save(home)?;
    prune_cache(home, &registry);
    Ok(out)
}

/// Copies the jar at `file` into the game folder's `mods/` (where [`sync`] finds it) and
/// returns its path from the game folder. A jar of the same name is replaced.
pub fn add(home: &Path, file: &Path) -> Result<String, String> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.to_ascii_lowercase().ends_with(".jar") {
        return Err(format!("{name}: not a mod jar (.jar)"));
    }
    let opened = fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    zip::ZipArchive::new(opened).map_err(|e| format!("{name}: not a jar: {e}"))?;
    let dir = home.join(JAR_DIRS[0]);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(&name);
    if fs::canonicalize(file).ok() != fs::canonicalize(&dest).ok() {
        fs::copy(file, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(format!("{}/{name}", JAR_DIRS[0]))
}

/// Turns every installed mod that only partly ports on (or off): [`Settings::partial`] changed.
pub fn apply_partial(home: &Path, on: bool) -> Result<(), String> {
    for e in entries(home) {
        if e.state == State::Installed && e.verdict == Some(Verdict::Partly) {
            set_enabled(home, &e.mod_id, on)?;
        }
    }
    Ok(())
}

/// Whether mod `id` is on in `horizon-mods.txt`.
pub fn is_enabled(home: &Path, id: &str) -> bool {
    let key = format!("mod.{id}");
    fs::read_to_string(home.join(MODS_LIST))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(':'))
        .any(|(k, v)| k.trim() == key && v.trim() == "true")
}

/// Turns mod `id` on (or off) in `horizon-mods.txt`, keeping the other lines.
pub fn set_enabled(home: &Path, id: &str, on: bool) -> Result<(), String> {
    let path = home.join(MODS_LIST);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let key = format!("mod.{id}");
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once(':').is_none_or(|(k, _)| k.trim() != key))
        .map(str::to_owned)
        .collect();
    if on {
        lines.push(format!("{key}:true"));
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn sha1_of(path: &Path) -> Result<String, String> {
    use sha1::Digest;
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn failed(rel: &str, sha1: String, why: String) -> Entry {
    Entry {
        jar: rel.to_owned(),
        sha1,
        mod_id: String::new(),
        java_id: String::new(),
        name: String::new(),
        version: String::new(),
        loader: String::new(),
        state: State::Failed,
        verdict: None,
        summary: format!("Can't be read: {why}"),
        ports: Vec::new(),
        skipped: Vec::new(),
        notes: Vec::new(),
        code: String::new(),
        mixins: Vec::new(),
    }
}

fn mod_folder(home: &Path, id: &str) -> PathBuf {
    home.join(MODS_DIR).join(id)
}

/// Whether an installed translation is still as it was installed (the player may have deleted
/// its folder).
fn in_place(home: &Path, e: &Entry) -> bool {
    e.state != State::Installed
        || marker(&mod_folder(home, &e.mod_id)).is_some_and(|m| m.jar == e.jar && m.sha1 == e.sha1)
}

/// Removes the folder mod `e` installed, if it is still the translation's. Whether it was.
fn uninstall(home: &Path, e: &Entry) -> bool {
    let dir = mod_folder(home, &e.mod_id);
    if e.state != State::Installed
        || e.mod_id.is_empty()
        || marker(&dir).is_none_or(|m| m.jar != e.jar)
    {
        return false;
    }
    let _ = set_enabled(home, &e.mod_id, false);
    fs::remove_dir_all(&dir).is_ok()
}

fn cache_dir(home: &Path, sha1: &str) -> PathBuf {
    home.join(STATE).join("cache").join(sha1)
}

/// The translation of the jar with `sha1`, when made before by this version.
fn cached(home: &Path, sha1: &str) -> Option<Translation> {
    let dir = cache_dir(home, sha1);
    let t: Translation = read_json(&dir.join("translation.json")).ok()?;
    (t.version == VERSION && (t.check.verdict == Verdict::No || dir.join("carry").is_dir()))
        .then_some(t)
}

/// The jar at `path` translated: the cached translation, else a new one into the cache.
fn translation(home: &Path, path: &Path, sha1: &str) -> Result<Translation, String> {
    if let Some(t) = cached(home, sha1) {
        return Ok(t);
    }
    let dir = cache_dir(home, sha1);
    let part = dir.with_extension("part");
    let _ = fs::remove_dir_all(&part);
    fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let t = match translate_jar(path, &part) {
        Ok(t) => t,
        Err(e) => {
            let _ = fs::remove_dir_all(&part);
            return Err(e);
        }
    };
    if t.check.verdict == Verdict::No {
        for d in ["carry", "outside"] {
            let _ = fs::remove_dir_all(part.join(d));
        }
    }
    write_json(&part.join("translation.json"), &t)?;
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&part, &dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(t)
}

/// Drops the cached translations of jars no longer in the game folder.
fn prune_cache(home: &Path, registry: &Registry) {
    let keep: BTreeSet<&str> = registry.entries.iter().map(|e| e.sha1.as_str()).collect();
    let Ok(read) = fs::read_dir(home.join(STATE).join("cache")) else {
        return;
    };
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(name.as_str()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Translates and installs one jar. `done` are the jars this sync already went through.
fn translate_one(
    home: &Path,
    rel: &str,
    path: &Path,
    sha1: &str,
    before: Option<&Entry>,
    settings: &Settings,
    done: &[Entry],
    out: &mut Outcome,
) -> Entry {
    let t = match translation(home, path, sha1) {
        Ok(t) => t,
        Err(e) => {
            if let Some(b) = before {
                uninstall(home, b);
            }
            return failed(rel, sha1.to_owned(), e);
        }
    };
    let c = &t.check;
    let mut entry = Entry {
        jar: rel.to_owned(),
        sha1: sha1.to_owned(),
        mod_id: c.mod_id.clone(),
        java_id: t.java_id.clone(),
        name: c.name.clone(),
        version: c.version.clone(),
        loader: c.loader.clone(),
        state: State::NotInstalled,
        verdict: Some(c.verdict),
        summary: c.summary.clone(),
        ports: c.ports.clone(),
        skipped: c.skipped.clone(),
        notes: t.notes.clone(),
        code: c.code_line(),
        mixins: c.mixins.lines(),
    };
    let dest = mod_folder(home, &entry.mod_id);
    // Installed before from this jar (also when the cache was cleared): the player's switch stays.
    let was_installed = before
        .is_some_and(|b| b.state == State::Installed && b.mod_id == entry.mod_id)
        || marker(&dest).is_some_and(|m| m.jar == rel);
    if let Some(b) = before.filter(|b| b.mod_id != entry.mod_id || c.verdict == Verdict::No) {
        uninstall(home, b);
    }
    if c.verdict == Verdict::No {
        return entry;
    }
    if let Some(other) = done
        .iter()
        .find(|e| e.state == State::Installed && e.mod_id == entry.mod_id)
    {
        entry.summary = format!(
            "Not installed: {} is the same mod ({}). Remove one of the two jars.",
            other.jar, entry.mod_id
        );
        return entry;
    }
    if let Err(why) = install(home, &cache_dir(home, sha1).join("carry"), &entry) {
        entry.summary = why;
        return entry;
    }
    entry.state = State::Installed;
    let wanted = c.verdict == Verdict::Works || settings.partial;
    if !was_installed && wanted && set_enabled(home, &entry.mod_id, true).is_ok() {
        out.enabled.push(entry.mod_id.clone());
    }
    entry
}

/// Copies the carried-over folder mod at `carry` into `horizon-mods/<id>/`, the manifest saying
/// where it comes from, and marks it as the translation's.
fn install(home: &Path, carry: &Path, entry: &Entry) -> Result<(), String> {
    let dest = mod_folder(home, &entry.mod_id);
    if dest.exists() {
        match marker(&dest) {
            // Another jar's translation, while that jar is still there.
            Some(m) if m.jar != entry.jar && home.join(&m.jar).is_file() => {
                return Err(format!(
                    "Not installed: {} is the same mod ({}). Remove one of the two jars.",
                    m.jar, entry.mod_id
                ));
            }
            Some(_) => fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?,
            None => {
                return Err(format!(
                    "Not installed: {MODS_DIR}/{} is already there, from the Mods page or put there by hand. Remove it to use this jar's translation.",
                    entry.mod_id
                ));
            }
        }
    }
    copy_dir(carry, &dest)?;
    let manifest = dest.join(horizon_mods::manifest::MANIFEST);
    let mut text =
        fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let file = entry.jar.rsplit('/').next().unwrap_or(&entry.jar);
    let label = entry.verdict.map_or("", Verdict::label);
    let description = format!(
        "Translated from the {} mod {file}. {label}: {}",
        entry.loader_name(),
        entry.summary
    );
    text.push_str(&format!(
        "description = {}\n",
        toml::Value::String(description)
    ));
    fs::write(&manifest, text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    write_json(
        &dest.join(MARKER),
        &Marker {
            jar: entry.jar.clone(),
            sha1: entry.sha1.clone(),
        },
    )
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
    }
    Ok(())
}

/// The jar at `path` translated into `out/` (`carry/`, `check.json`): staged with 1.21's layout
/// and a mod id the game takes when it needs either, then checked.
fn translate_jar(path: &Path, out: &Path) -> Result<Translation, String> {
    let mut info = jar::read(path)?;
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let java_id = info.mod_id.clone();
    let loader = check::loader(&names);
    if too_old(&names) {
        let summary =
            "Made for Minecraft 1.12 or older, whose files nothing reads now: Rusty Minecraft \
                       translates mods made for 1.13 and later."
                .to_owned();
        return Ok(Translation {
            version: VERSION,
            java_id,
            check: refused(&info, loader, summary),
            notes: Vec::new(),
        });
    }
    let id = horizon_id(&java_id, loader, &names);
    let folders = legacy_folders(&names);
    // The jar's namespace named as its refused id, renamed with it.
    let namespace = (id != java_id && namespaces(&names).iter().any(|(ns, _)| *ns == java_id))
        .then_some((java_id.as_str(), id.as_str()));
    let mut notes = Vec::new();
    if !folders.is_empty() {
        let list: Vec<&str> = folders.iter().map(|(before, _)| *before).collect();
        notes.push(format!(
            "Made for Minecraft 1.20 or older: its data folders ({}) and recipe results were translated to 1.21's names.",
            list.join(", ")
        ));
    }
    if id != java_id {
        notes.push(format!(
            "Its mod id {java_id:?} isn't one Rusty Minecraft takes: it plays as {id}."
        ));
    }
    let staged = (!folders.is_empty() || namespace.is_some()).then(|| {
        out.join("staged")
            .join(path.file_name().unwrap_or("mod.jar".as_ref()))
    });
    if let Some(staged) = &staged {
        stage(path, staged, &folders, namespace)?;
    }
    info.mod_id = id;
    let check = check::check_info(staged.as_deref().unwrap_or(path), info, out);
    let _ = fs::remove_dir_all(out.join("staged"));
    Ok(Translation {
        version: VERSION,
        java_id,
        check: check?,
        notes,
    })
}

/// A check for a jar that isn't carried at all.
fn refused(info: &jar::JarInfo, loader: &str, summary: String) -> Check {
    Check {
        mod_id: info.mod_id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        loader: loader.to_owned(),
        license: info.license.clone(),
        verdict: Verdict::No,
        summary,
        ports: Vec::new(),
        skipped: Vec::new(),
        depends: info.depends.clone(),
        namespaces: Vec::new(),
        code: Default::default(),
        mixins: Default::default(),
    }
}

/// Made for Minecraft 1.12 or older: Forge's `mcmod.info` and none of the metadata 1.13 on
/// reads, or textures in `textures/blocks/` and none in `textures/block/`.
fn too_old(names: &[String]) -> bool {
    let has = |n: &str| names.iter().any(|x| x == n);
    let any = |part: &str| {
        names
            .iter()
            .any(|n| n.starts_with("assets/") && n.contains(part))
    };
    let modern = has("META-INF/mods.toml")
        || has("META-INF/neoforge.mods.toml")
        || has("fabric.mod.json")
        || has("quilt.mod.json");
    !modern && (has("mcmod.info") || (any("/textures/blocks/") && !any("/textures/block/")))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The namespaces of a jar's `assets/` and `data/` that could be its own, with how many files
/// each holds.
fn namespaces(names: &[String]) -> Vec<(&str, usize)> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for n in names {
        let mut parts = n.split('/');
        let (Some("assets" | "data"), Some(ns), Some(_)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if NOT_OWN.contains(&ns) {
            continue;
        }
        match counts.iter_mut().find(|(x, _)| *x == ns) {
            Some((_, c)) => *c += 1,
            None => counts.push((ns, 1)),
        }
    }
    counts
}

/// The content mod's id: the Java mod's when the game takes it (`[a-z0-9_]{1,64}`), else the
/// same made one (`my-mod` → `my_mod`). A jar naming no mod is named after its file
/// (`Ruby-1.2+fabric`): the namespace holding most of its files names it better.
fn horizon_id(java_id: &str, loader: &str, names: &[String]) -> String {
    if valid_id(java_id) {
        return java_id.to_owned();
    }
    let mut made: String = java_id
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    made = made.trim_matches('_').chars().take(64).collect();
    let found = namespaces(names);
    if loader.is_empty()
        && !found.iter().any(|(ns, _)| *ns == made)
        && let Some((ns, _)) = found
            .iter()
            .filter(|(ns, _)| valid_id(ns))
            .max_by_key(|(ns, n)| (*n, std::cmp::Reverse(*ns)))
    {
        return (*ns).to_owned();
    }
    if made.is_empty() {
        "java_mod".to_owned()
    } else {
        made
    }
}

/// The plural data folders a jar uses with none of their singular names: it was made before
/// 1.21 (some later jars keep a stale plural folder beside the new one, which stays as it is).
fn legacy_folders(names: &[String]) -> Vec<(&'static str, &'static str)> {
    let kind_used = |kind: &str| {
        names.iter().any(|n| {
            let mut parts = n.splitn(3, '/');
            parts.next() == Some("data")
                && parts.next().is_some()
                && parts
                    .next()
                    .is_some_and(|rest| rest.starts_with(&format!("{kind}/")))
        })
    };
    LEGACY_FOLDERS
        .into_iter()
        .filter(|(before, after)| kind_used(before) && !kind_used(after))
        .collect()
}

/// An entry's name in the staged jar: its data folder renamed, its namespace renamed.
fn staged_name(name: &str, folders: &[(&str, &str)], namespace: Option<(&str, &str)>) -> String {
    let mut parts: Vec<String> = name.splitn(3, '/').map(str::to_owned).collect();
    if parts.len() == 3 && (parts[0] == "assets" || parts[0] == "data") {
        if let Some((old, new)) = namespace
            && parts[1] == old
        {
            parts[1] = new.to_owned();
        }
        if parts[0] == "data"
            && let Some((before, after)) = folders
                .iter()
                .find(|(before, _)| parts[2].starts_with(&format!("{before}/")))
        {
            parts[2] = format!("{after}{}", &parts[2][before.len()..]);
        }
    }
    parts.join("/")
}

/// A recipe made before 1.20.5 in 1.21's shape, in place: its result an object with `id`
/// (`{"item": ...}`, a bare id, or a stonecutter's bare id beside its `count`).
fn legacy_recipe(recipe: &mut Value) -> bool {
    let Some(o) = recipe.as_object_mut() else {
        return false;
    };
    match o.get_mut("result") {
        Some(Value::String(id)) => {
            let mut result = serde_json::json!({ "id": id.clone() });
            if let Some(count) = o.remove("count") {
                result["count"] = count;
            }
            o.insert("result".into(), result);
            true
        }
        Some(Value::Object(result)) if !result.contains_key("id") => match result.remove("item") {
            Some(item) => {
                result.insert("id".into(), item);
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// Every id in namespace `old` named in `new` instead, in place: strings (`old:x`, `#old:x`)
/// and keys, translation keys too (`block.old.x`). Whether any was.
fn rename_namespace(v: &mut Value, old: &str, new: &str) -> bool {
    let rename = |s: &str| -> Option<String> {
        let (from, to) = (format!("{old}:"), format!("{new}:"));
        if let Some(rest) = s.strip_prefix(&from) {
            Some(format!("{to}{rest}"))
        } else if let Some(rest) = s.strip_prefix(&format!("#{from}")) {
            Some(format!("#{to}{rest}"))
        } else {
            let dotted = format!(".{old}.");
            s.contains(&dotted)
                .then(|| s.replace(&dotted, &format!(".{new}.")))
        }
    };
    match v {
        Value::String(s) => match rename(s) {
            Some(r) => {
                *s = r;
                true
            }
            None => false,
        },
        Value::Array(a) => a
            .iter_mut()
            .fold(false, |c, x| rename_namespace(x, old, new) | c),
        Value::Object(o) => {
            let mut changed = false;
            let entries = std::mem::take(o);
            for (k, mut x) in entries {
                changed |= rename_namespace(&mut x, old, new);
                let k = match rename(&k) {
                    Some(r) => {
                        changed = true;
                        r
                    }
                    None => k,
                };
                o.insert(k, x);
            }
            changed
        }
        _ => false,
    }
}

/// Writes the jar at `path` to `staged` with its data folders and namespace renamed
/// ([`staged_name`]) and its JSON rewritten to match.
fn stage(
    path: &Path,
    staged: &Path,
    folders: &[(&str, &str)],
    namespace: Option<(&str, &str)>,
) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(dir) = staged.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let out = fs::File::create(staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    let mut writer = zip::ZipWriter::new(out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut written = BTreeSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() || entry.size() > MAX_ENTRY {
            continue;
        }
        let name = staged_name(entry.name(), folders, namespace);
        if !written.insert(name.clone()) {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        let rewrite = name.ends_with(".json") || name.ends_with(".mcmeta");
        if rewrite && let Ok(mut v) = serde_json::from_slice::<Value>(&bytes) {
            let mut changed =
                namespace.is_some_and(|(old, new)| rename_namespace(&mut v, old, new));
            let recipe = name.split('/').nth(2) == Some("recipe") && name.starts_with("data/");
            if recipe && !folders.is_empty() {
                changed |= legacy_recipe(&mut v);
            }
            if changed {
                bytes = serde_json::to_vec_pretty(&v).map_err(|e| format!("{name}: {e}"))?;
            }
        }
        writer
            .start_file(name.as_str(), options)
            .map_err(|e| format!("{name}: {e}"))?;
        writer
            .write_all(&bytes)
            .map_err(|e| format!("{name}: {e}"))?;
    }
    writer
        .finish()
        .map_err(|e| format!("{}: {e}", staged.display()))?;
    Ok(())
}

/// A block or item a mod registered from Java code in the optional Java layer
/// (docs/java-mod-runtime.md): `registry` (`block` or `item`), its id and its properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Registered {
    pub registry: String,
    pub id: String,
    pub properties: Vec<(String, String)>,
}

/// Applies what mods registered from Java to the blocks and items of their translated content
/// mods (installed ones, holding [`MARKER`]): a block's hardness, explosion resistance, light,
/// tool requirement and friction; an item's stack size, rarity, durability, fire resistance and
/// crafting remainder. A registration with nothing to apply it to (a block the jar has no
/// blockstate for) is left out. Lines saying what changed.
pub fn apply_registrations(home: &Path, registered: &[Registered]) -> Vec<String> {
    let installed: Vec<PathBuf> = entries(home)
        .into_iter()
        .filter(|e| e.state == State::Installed && !e.mod_id.is_empty())
        .map(|e| mod_folder(home, &e.mod_id))
        .filter(|dir| marker(dir).is_some())
        .collect();
    let mut changed = Vec::new();
    for r in registered {
        let Some((ns, path)) = r.id.split_once(':') else {
            continue;
        };
        // The translation renamed a namespace with dashes, as it renamed the mod's id.
        let ns = ns.replace('-', "_");
        let kind = match r.registry.as_str() {
            "block" => "block",
            "item" => "item",
            _ => continue,
        };
        let rel = format!("data/{ns}/horizon/{kind}/{path}.json");
        let Some(file) = installed.iter().map(|d| d.join(&rel)).find(|f| f.is_file()) else {
            continue;
        };
        let Ok(mut v) = read_json::<serde_json::Value>(&file) else {
            continue;
        };
        let before = v.clone();
        let get = |k: &str| {
            r.properties
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        let num = |k: &str| get(k).and_then(|x| x.parse::<f64>().ok());
        if kind == "block" {
            let props = &mut v["properties"];
            if !props.is_object() {
                *props = serde_json::json!({});
            }
            for key in ["hardness", "explosion_resistance", "friction"] {
                if let Some(x) = num(key) {
                    props[key] = serde_json::json!(x);
                }
            }
            if let Some(light) = num("light") {
                props["light"] = serde_json::json!(light.clamp(0.0, 15.0) as u8);
            }
            if get("requires_tool") == Some("true") {
                props["requires_tool"] = serde_json::json!(true);
            }
        } else {
            if let Some(n) = num("max_stack_size") {
                v["max_stack_size"] = serde_json::json!(n.clamp(1.0, 99.0) as u8);
            }
            if let Some(rarity) = get("rarity") {
                v["rarity"] = serde_json::json!(rarity);
            }
            if let Some(remainder) = get("crafting_remainder") {
                v["crafting_remainder"] = serde_json::json!(remainder.replace('-', "_"));
            }
            if !v["components"].is_object() {
                v["components"] = serde_json::json!({});
            }
            if let Some(n) = num("durability") {
                v["components"]["minecraft:max_damage"] = serde_json::json!(n as i64);
                v["max_stack_size"] = serde_json::json!(1);
            }
            if get("fire_resistant") == Some("true") {
                v["components"]["minecraft:damage_resistant"] =
                    serde_json::json!({"types": "#minecraft:is_fire"});
            }
            if v["components"].as_object().is_some_and(|c| c.is_empty())
                && before.get("components").is_none()
            {
                v.as_object_mut().map(|o| o.remove("components"));
            }
        }
        if v != before && write_json(&file, &v).is_ok() {
            changed.push(format!("{kind} {}: {}", r.id, describe(&r.properties)));
        }
    }
    changed
}

fn describe(properties: &[(String, String)]) -> String {
    properties
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("port-pack-translate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn jar(path: &Path, files: &[(&str, &[u8])]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    const BLOCKSTATE: &[u8] = br#"{"variants": {"": {"model": "ruby:block/ruby_block"}}}"#;

    fn fabric(id: &str) -> Vec<u8> {
        format!(r#"{{"schemaVersion": 1, "id": "{id}", "version": "1.0", "name": "Ruby"}}"#)
            .into_bytes()
    }

    fn quiet() -> impl Fn(String) {
        |_| {}
    }

    #[test]
    fn a_jar_in_mods_is_translated_installed_and_turned_on() {
        let home = temp("install");
        let recipe = br#"{"type": "minecraft:crafting_shapeless", "ingredients": ["minecraft:red_dye"], "result": {"id": "ruby:ruby"}}"#;
        let meta = fabric("ruby");
        jar(
            &home.join("mods/ruby-1.0.jar"),
            &[
                ("fabric.mod.json", &meta),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
                ("assets/ruby/models/item/ruby.json", b"{}"),
                ("data/ruby/recipe/ruby.json", recipe),
            ],
        );
        let out = sync(&home, &quiet()).unwrap();
        assert_eq!(out.translated, ["mods/ruby-1.0.jar"]);
        assert_eq!(out.enabled, ["ruby"]);
        let e = &entries(&home)[0];
        assert_eq!(
            (e.state, e.verdict),
            (State::Installed, Some(Verdict::Works))
        );
        assert_eq!(e.loader_name(), "Fabric");
        let folder = home.join("horizon-mods/ruby");
        assert!(folder.join("data/ruby/recipe/ruby.json").is_file());
        let manifest = fs::read_to_string(folder.join("horizon-mod.toml")).unwrap();
        let m = horizon_mods::Manifest::parse(&manifest).unwrap();
        assert!(
            m.description
                .starts_with("Translated from the Fabric mod ruby-1.0.jar. Works:"),
            "{}",
            m.description
        );
        assert!(is_enabled(&home, "ruby"));

        // Nothing changed: nothing translated again, and the player's switch stays.
        set_enabled(&home, "ruby", false).unwrap();
        assert_eq!(sync(&home, &quiet()).unwrap(), Outcome::default());
        assert!(!is_enabled(&home, "ruby"));

        // The jar removed: its translation goes with it.
        fs::remove_file(home.join("mods/ruby-1.0.jar")).unwrap();
        let out = sync(&home, &quiet()).unwrap();
        assert_eq!(out.removed, ["mods/ruby-1.0.jar"]);
        assert!(!folder.exists());
        assert!(entries(&home).is_empty());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn what_a_mod_registers_from_java_applies_to_its_translated_content() {
        let home = temp("registered");
        let meta = fabric("ruby");
        jar(
            &home.join("mods/ruby-1.0.jar"),
            &[
                ("fabric.mod.json", &meta),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
                ("assets/ruby/models/item/ruby.json", b"{}"),
            ],
        );
        sync(&home, &quiet()).unwrap();
        let data = home.join("horizon-mods/ruby/data/ruby/horizon");
        let block = data.join("block/ruby_block.json");
        let item = data.join("item/ruby.json");
        assert!(block.is_file() && item.is_file());
        let r = |registry: &str, id: &str, props: &[(&str, &str)]| Registered {
            registry: registry.into(),
            id: id.into(),
            properties: props
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let changed = apply_registrations(
            &home,
            &[
                r(
                    "block",
                    "ruby:ruby_block",
                    &[
                        ("hardness", "1.5"),
                        ("light", "7"),
                        ("requires_tool", "true"),
                    ],
                ),
                r(
                    "item",
                    "ruby:ruby",
                    &[("max_stack_size", "16"), ("rarity", "rare")],
                ),
                r("item", "ruby:no_such_item", &[("max_stack_size", "1")]),
                r("block", "other:thing", &[("hardness", "9")]),
            ],
        );
        assert_eq!(changed.len(), 2, "{changed:?}");
        let block: serde_json::Value = read_json(&block).unwrap();
        assert_eq!(block["properties"]["hardness"], 1.5);
        assert_eq!(block["properties"]["light"], 7);
        assert_eq!(block["properties"]["requires_tool"], true);
        let item: serde_json::Value = read_json(&item).unwrap();
        assert_eq!(item["max_stack_size"], 16);
        assert_eq!(item["rarity"], "rare");
        // The content still reads as a content mod's.
        let manifest = fs::read_to_string(home.join("horizon-mods/ruby/horizon-mod.toml")).unwrap();
        let m = horizon_mods::Manifest::parse(&manifest).unwrap();
        horizon_mods::Content::read(&home.join("horizon-mods/ruby"), &m).unwrap();
        // Applied again, nothing changes.
        let again = apply_registrations(
            &home,
            &[r(
                "item",
                "ruby:ruby",
                &[("max_stack_size", "16"), ("rarity", "rare")],
            )],
        );
        assert!(again.is_empty());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_game_folder_without_jars_is_left_alone() {
        let home = temp("empty");
        assert_eq!(sync(&home, &quiet()).unwrap(), Outcome::default());
        assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_folder_the_translation_did_not_install_is_never_replaced() {
        let home = temp("foreign");
        let meta = fabric("ruby");
        jar(
            &home.join("mods/ruby.jar"),
            &[
                ("fabric.mod.json", &meta),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
            ],
        );
        let mine = home.join("horizon-mods/ruby");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("horizon-mod.toml"), "id = \"ruby\"").unwrap();
        sync(&home, &quiet()).unwrap();
        let e = &entries(&home)[0];
        assert_eq!(e.state, State::NotInstalled);
        assert!(e.summary.contains("already there"), "{}", e.summary);
        assert_eq!(
            fs::read_to_string(mine.join("horizon-mod.toml")).unwrap(),
            "id = \"ruby\""
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_partly_porting_mod_waits_for_the_setting() {
        let home = temp("partial");
        Settings {
            translate: true,
            partial: false,
            ..Settings::default()
        }
        .save(&home)
        .unwrap();
        let tile = crate::classes::tests_support::class_file(
            "ruby/Tile",
            "net/minecraft/world/level/block/entity/BlockEntity",
        );
        let meta = fabric("ruby");
        jar(
            &home.join("horizon-mods/ruby.jar"),
            &[
                ("fabric.mod.json", &meta),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
                ("ruby/Tile.class", &tile),
            ],
        );
        let out = sync(&home, &quiet()).unwrap();
        assert!(out.enabled.is_empty());
        let e = &entries(&home)[0];
        assert_eq!(
            (e.state, e.verdict),
            (State::Installed, Some(Verdict::Partly))
        );
        assert!(!is_enabled(&home, "ruby"));
        apply_partial(&home, true).unwrap();
        assert!(is_enabled(&home, "ruby"));
        fs::remove_dir_all(&home).unwrap();
    }

    /// A 26.x mod with a mixin: its entry says what of its code could run and lists the mixin.
    #[test]
    fn a_mods_mixins_and_libraries_are_listed() {
        use crate::classes::{
            Value,
            tests_support::{ann, annotated_class, member, s},
        };
        let home = temp("mixins");
        let meta = br#"{"schemaVersion": 1, "id": "ruby", "version": "1.0", "name": "Ruby",
            "depends": {"minecraft": "~26.3", "fabric-api": "*"}, "mixins": ["ruby.mixins.json"]}"#;
        let at = ann(
            "org/spongepowered/asm/mixin/injection/At",
            vec![("value", s("HEAD"))],
        );
        let mixin = annotated_class(
            "ruby/mixin/HurtMixin",
            0x0421,
            &[ann(
                "org/spongepowered/asm/mixin/Mixin",
                vec![(
                    "value",
                    Value::Class("Lnet/minecraft/world/entity/LivingEntity;".into()),
                )],
            )],
            &[],
            &[member(
                "onHurt",
                "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable;)V",
                vec![ann(
                    "org/spongepowered/asm/mixin/injection/Inject",
                    vec![
                        ("method", s("hurtServer")),
                        ("at", Value::Array(vec![Value::Annotation(at)])),
                    ],
                )],
            )],
        );
        jar(
            &home.join("mods/ruby.jar"),
            &[
                ("fabric.mod.json", meta),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
                (
                    "ruby.mixins.json",
                    br#"{"package": "ruby.mixin", "mixins": ["HurtMixin"]}"#,
                ),
                ("ruby/mixin/HurtMixin.class", &mixin),
            ],
        );
        sync(&home, &quiet()).unwrap();
        let e = &entries(&home)[0];
        assert_eq!(e.code, "0 of 1 mixin supported");
        let lines: Vec<&str> = e.mixins.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            lines,
            [
                "LivingEntity.hurtServer @Inject at HEAD: not supported, no hook point at the start of LivingEntity.hurtServer yet",
                "Fabric API: Rust version in crates/horizon-java for its part fabric-api-base, fabric-lifecycle-events-v1 (what else a mod uses of it is listed)",
                "Needs Java 21 or newer to run its code",
            ]
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_1_20_jar_is_translated_to_the_1_21_layout() {
        let home = temp("legacy");
        let toml = br#"modLoader = "javafml"
[[mods]]
modId = "ruby"
version = "0.9"
displayName = "Ruby"
"#;
        let recipe = br#"{"type": "minecraft:crafting_shaped", "pattern": ["XX"], "key": {"X": {"item": "minecraft:red_dye"}}, "result": {"item": "ruby:ruby", "count": 2}}"#;
        let cut = br#"{"type": "minecraft:stonecutting", "ingredient": {"item": "minecraft:stone"}, "result": "ruby:ruby", "count": 3}"#;
        jar(
            &home.join("mods/ruby-forge.jar"),
            &[
                ("META-INF/mods.toml", toml),
                ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
                ("assets/ruby/models/item/ruby.json", b"{}"),
                ("data/ruby/recipes/ruby.json", recipe),
                ("data/ruby/recipes/cut.json", cut),
                (
                    "data/minecraft/tags/blocks/mineable/pickaxe.json",
                    br#"{"values": ["ruby:ruby_block"]}"#,
                ),
            ],
        );
        sync(&home, &quiet()).unwrap();
        let e = &entries(&home)[0];
        assert_eq!(e.state, State::Installed, "{e:?}");
        assert_eq!(e.loader, "forge");
        assert!(
            e.notes[0].starts_with("Made for Minecraft 1.20 or older"),
            "{:?}",
            e.notes
        );
        let folder = home.join("horizon-mods/ruby");
        let read = |p: &str| -> Value {
            serde_json::from_slice(&fs::read(folder.join(p)).unwrap()).unwrap()
        };
        let r = read("data/ruby/recipe/ruby.json");
        assert_eq!(
            r["result"],
            serde_json::json!({"id": "ruby:ruby", "count": 2})
        );
        assert_eq!(r["key"]["X"], "minecraft:red_dye");
        let c = read("data/ruby/recipe/cut.json");
        assert_eq!(
            c["result"],
            serde_json::json!({"id": "ruby:ruby", "count": 3})
        );
        assert!(
            folder
                .join("data/minecraft/tags/block/mineable/pickaxe.json")
                .is_file()
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_mod_id_the_game_refuses_is_renamed_with_its_namespace() {
        let home = temp("rename");
        let meta = fabric("ruby-ores");
        let state = br#"{"variants": {"": {"model": "ruby-ores:block/ruby_ore"}}}"#;
        let lang = br#"{"block.ruby-ores.ruby_ore": "Ruby Ore"}"#;
        jar(
            &home.join("mods/ruby-ores.jar"),
            &[
                ("fabric.mod.json", &meta),
                ("assets/ruby-ores/blockstates/ruby_ore.json", state),
                ("assets/ruby-ores/lang/en_us.json", lang),
            ],
        );
        sync(&home, &quiet()).unwrap();
        let e = &entries(&home)[0];
        assert_eq!(
            (e.java_id.as_str(), e.mod_id.as_str()),
            ("ruby-ores", "ruby_ores")
        );
        assert_eq!(e.state, State::Installed, "{e:?}");
        let folder = home.join("horizon-mods/ruby_ores");
        let state: Value = serde_json::from_slice(
            &fs::read(folder.join("assets/ruby_ores/blockstates/ruby_ore.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["variants"][""]["model"], "ruby_ores:block/ruby_ore");
        let lang = fs::read_to_string(folder.join("assets/ruby_ores/lang/en_us.json")).unwrap();
        assert!(lang.contains("block.ruby_ores.ruby_ore"), "{lang}");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_same_mod_twice_installs_once_and_old_jars_are_refused() {
        let home = temp("twice");
        let meta = fabric("ruby");
        let files: [(&str, &[u8]); 2] = [
            ("fabric.mod.json", &meta),
            ("assets/ruby/blockstates/ruby_block.json", BLOCKSTATE),
        ];
        jar(&home.join("mods/a.jar"), &files);
        jar(&home.join("horizon-mods/b.jar"), &files);
        jar(
            &home.join("mods/old.jar"),
            &[
                ("mcmod.info", b"[]"),
                ("assets/old/textures/blocks/x.png", b""),
            ],
        );
        sync(&home, &quiet()).unwrap();
        let list = entries(&home);
        let state = |jar: &str| list.iter().find(|e| e.jar == jar).unwrap();
        assert_eq!(state("horizon-mods/b.jar").state, State::Installed);
        assert_eq!(state("mods/a.jar").state, State::NotInstalled);
        assert!(state("mods/a.jar").summary.contains("same mod"));
        let old = state("mods/old.jar");
        assert_eq!(old.verdict, Some(Verdict::No));
        assert!(old.summary.contains("1.12 or older"), "{}", old.summary);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn names_are_staged_with_1_21_folders() {
        let folders = [("recipes", "recipe"), ("tags/blocks", "tags/block")];
        assert_eq!(
            staged_name("data/x/tags/blocks/a.json", &folders, None),
            "data/x/tags/block/a.json"
        );
        assert_eq!(
            staged_name("data/x/recipes/a.json", &folders, Some(("x", "y"))),
            "data/y/recipe/a.json"
        );
        assert_eq!(
            staged_name("assets/x/recipes/a.json", &folders, None),
            "assets/x/recipes/a.json"
        );
        assert_eq!(
            staged_name("fabric.mod.json", &folders, None),
            "fabric.mod.json"
        );
        let settings = Settings::default();
        assert!(settings.translate && settings.partial);
    }
}
