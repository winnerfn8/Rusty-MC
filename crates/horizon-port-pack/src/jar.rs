//! What a mod jar holds, read without decompiling: its metadata
//! (`neoforge.mods.toml`, `mods.toml`, `fabric.mod.json`) and a census of
//! its data, assets and classes.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct JarInfo {
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    /// Required dependencies by mod id (loaders and `minecraft` left out).
    pub depends: Vec<String>,
    pub census: Census,
    /// The Minecraft versions it says it runs on, as written (`~1.21.1`,
    /// `[26.1,)`), or `` when it doesn't say.
    #[serde(default)]
    pub minecraft: String,
    /// Every required mod and library, loader APIs included (Fabric API,
    /// NeoForge, Architectury...); Minecraft, Java and the bare loaders left out.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Its mixin configs, as its metadata names them.
    #[serde(default)]
    pub mixin_configs: Vec<String>,
    /// Its entry points (`fabric.mod.json`'s `entrypoints`): kind (`main`, `client`, `server`)
    /// and the class (or `Class::field`) to run, for the optional Java layer.
    #[serde(default)]
    pub entrypoints: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Census {
    pub blockstates: usize,
    pub item_models: usize,
    pub recipes: usize,
    pub loot_tables: usize,
    pub tags: usize,
    pub worldgen: usize,
    pub structures: usize,
    pub languages: usize,
    pub textures: usize,
    pub sounds: usize,
    pub classes: usize,
    pub mixin_configs: usize,
}

const NOT_MODS: [&str; 5] = ["minecraft", "neoforge", "forge", "fabricloader", "java"];
/// Not libraries: the game, Java and the loaders' own bootstrap.
const NOT_LIBRARIES: [&str; 5] = [
    "minecraft",
    "java",
    "fabricloader",
    "quilt_loader",
    "javafml",
];

fn library(id: &str) -> bool {
    !id.is_empty() && !NOT_LIBRARIES.contains(&id)
}

pub fn read(path: &Path) -> Result<JarInfo, String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut info = read_archive(&mut zip)?;
    if info.mod_id.is_empty() {
        info.mod_id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(info)
}

/// [`read`] from an open archive (a jar bundled in another one); the mod id
/// is `` when its metadata names none.
pub fn read_archive<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<JarInfo, String> {
    let text = |zip: &mut zip::ZipArchive<R>, name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let mut info = if let Some(t) =
        text(zip, "META-INF/neoforge.mods.toml").or_else(|| text(zip, "META-INF/mods.toml"))
    {
        mods_toml(&t)?
    } else if let Some(t) = text(zip, "fabric.mod.json") {
        fabric(&t)?
    } else if let Some(t) = text(zip, "quilt.mod.json") {
        quilt(&t)?
    } else {
        JarInfo::default()
    };
    // `version = "${file.jarVersion}"` is filled in from the manifest at load.
    if info.version.starts_with("${") {
        info.version = text(zip, "META-INF/MANIFEST.MF")
            .and_then(|m| manifest_value(&m, "Implementation-Version"))
            .unwrap_or_default();
    }
    // Forge before NeoForge named its configs in the manifest.
    if info.mixin_configs.is_empty()
        && let Some(list) =
            text(zip, "META-INF/MANIFEST.MF").and_then(|m| manifest_value(&m, "MixinConfigs"))
    {
        info.mixin_configs = list
            .split(',')
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect();
    }
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    info.census = census(&names);
    Ok(info)
}

/// A main-section attribute of `META-INF/MANIFEST.MF`.
fn manifest_value(manifest: &str, key: &str) -> Option<String> {
    manifest
        .lines()
        .take_while(|l| !l.trim().is_empty())
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// `[[mods]]` (the first one is the jar's mod), the top-level `license`, and
/// `[[dependencies.<id>]]` with `type = "required"` (or the older
/// `mandatory = true`).
pub fn mods_toml(text: &str) -> Result<JarInfo, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| format!("mods.toml: {e}"))?;
    let first = doc
        .get("mods")
        .and_then(|m| m.as_array())
        .and_then(|m| m.first())
        .and_then(|m| m.as_table());
    let get = |t: Option<&toml::Table>, k: &str| {
        t.and_then(|t| t.get(k))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned()
    };
    let mod_id = get(first, "modId");
    let (mut depends, mut requires, mut minecraft) = (Vec::new(), Vec::new(), String::new());
    if let Some(deps) = doc
        .get("dependencies")
        .and_then(|d| d.get(&mod_id))
        .and_then(|d| d.as_array())
    {
        for d in deps.iter().filter_map(|d| d.as_table()) {
            let required = d.get("type").and_then(|t| t.as_str()).map_or_else(
                || d.get("mandatory").and_then(|m| m.as_bool()) == Some(true),
                |t| t.eq_ignore_ascii_case("required"),
            );
            let id = get(Some(d), "modId");
            if id == "minecraft" {
                minecraft = get(Some(d), "versionRange");
            }
            if required && !NOT_MODS.contains(&id.as_str()) && !id.is_empty() {
                depends.push(id.clone());
            }
            if required && library(&id) {
                requires.push(id);
            }
        }
    }
    // `[[mixins]] config = "..."` (NeoForge).
    let mixin_configs = doc
        .get("mixins")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("config").and_then(|c| c.as_str()))
        .map(str::to_owned)
        .collect();
    Ok(JarInfo {
        name: get(first, "displayName"),
        version: get(first, "version"),
        license: get(Some(&doc), "license"),
        mod_id,
        depends,
        census: Census::default(),
        minecraft,
        requires,
        mixin_configs,
        entrypoints: Vec::new(),
    })
}

pub fn fabric(text: &str) -> Result<JarInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("fabric.mod.json: {e}"))?;
    let s = |k: &str| v[k].as_str().unwrap_or("").to_owned();
    let license = match &v["license"] {
        serde_json::Value::Array(l) => l
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(" OR "),
        l => l.as_str().unwrap_or("").to_owned(),
    };
    let deps = v["depends"].as_object();
    let depends = deps
        .into_iter()
        .flatten()
        .map(|(id, _)| id.clone())
        .filter(|id| !NOT_MODS.contains(&id.as_str()) && !id.starts_with("fabric"))
        .collect();
    let requires = deps
        .into_iter()
        .flatten()
        .map(|(id, _)| id.clone())
        .filter(|id| library(id))
        .collect();
    let minecraft = match deps.and_then(|d| d.get("minecraft")) {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(" || "),
        Some(m) => m.as_str().unwrap_or("").to_owned(),
        None => String::new(),
    };
    // Each entry a config's name, or `{"config": ..., "environment": ...}`.
    let mixin_configs = v["mixins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().or_else(|| m["config"].as_str()))
        .map(str::to_owned)
        .collect();
    // Each kind's entries a class name, or `{"adapter": ..., "value": ...}` (only Java's
    // default adapter runs: a Kotlin one is kept and fails to run, saying so).
    let mut entrypoints = Vec::new();
    for (kind, list) in v["entrypoints"].as_object().into_iter().flatten() {
        for e in list.as_array().into_iter().flatten() {
            if let Some(spec) = e.as_str().or_else(|| e["value"].as_str()) {
                entrypoints.push((kind.clone(), spec.to_owned()));
            }
        }
    }
    Ok(JarInfo {
        mod_id: s("id"),
        name: s("name"),
        version: s("version"),
        license,
        depends,
        census: Census::default(),
        minecraft,
        requires,
        mixin_configs,
        entrypoints,
    })
}

/// `quilt.mod.json`: `quilt_loader` holds the id, version, `metadata` (name,
/// licence) and `depends`, each a mod id or `{"id": ...}` (optional ones
/// left out).
pub fn quilt(text: &str) -> Result<JarInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("quilt.mod.json: {e}"))?;
    let q = &v["quilt_loader"];
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_owned();
    let license = match &q["metadata"]["license"] {
        serde_json::Value::Array(l) => l
            .iter()
            .filter_map(|x| x.as_str().or_else(|| x["id"].as_str()))
            .collect::<Vec<_>>()
            .join(" OR "),
        serde_json::Value::Object(o) => o.get("id").map(s).unwrap_or_default(),
        l => s(l),
    };
    let required = || {
        q["depends"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["optional"].as_bool() != Some(true))
    };
    let minecraft = required()
        .find(|d| d["id"].as_str() == Some("minecraft"))
        .map(|d| match &d["versions"] {
            serde_json::Value::Array(a) => a
                .iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" || "),
            v => s(v),
        })
        .unwrap_or_default();
    let requires = required()
        .filter_map(|d| d.as_str().or_else(|| d["id"].as_str()))
        .filter(|id| library(id))
        .map(str::to_owned)
        .collect();
    // `"mixin"`: one config's name or a list of them.
    let mixin_configs = match &v["mixin"] {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str())
            .map(str::to_owned)
            .collect(),
        serde_json::Value::String(one) => vec![one.clone()],
        _ => Vec::new(),
    };
    let depends = required()
        .filter_map(|d| d.as_str().or_else(|| d["id"].as_str()))
        .filter(|id| {
            !NOT_MODS.contains(id)
                && !id.starts_with("quilt")
                && !id.starts_with("fabric")
                && !id.starts_with("qsl")
        })
        .map(str::to_owned)
        .collect();
    Ok(JarInfo {
        mod_id: s(&q["id"]),
        name: s(&q["metadata"]["name"]),
        version: s(&q["version"]),
        license,
        depends,
        census: Census::default(),
        minecraft,
        requires,
        mixin_configs,
        entrypoints: Vec::new(),
    })
}

pub fn census(names: &[String]) -> Census {
    let mut c = Census::default();
    for n in names {
        let parts: Vec<&str> = n.split('/').collect();
        if n.ends_with(".class") {
            c.classes += 1;
        } else if n.ends_with(".mixins.json") || (parts.len() == 1 && n.contains("mixins")) {
            c.mixin_configs += 1;
        }
        // assets/<ns>/<kind>/... and data/<ns>/<kind>/...
        let (Some(root), Some(kind)) = (parts.first(), parts.get(2)) else {
            continue;
        };
        let json = n.ends_with(".json");
        match (*root, *kind) {
            ("assets", "blockstates") if json => c.blockstates += 1,
            ("assets", "models") if json && parts.get(3) == Some(&"item") => c.item_models += 1,
            ("assets", "lang") => c.languages += 1,
            ("assets", "textures") if n.ends_with(".png") => c.textures += 1,
            ("assets", "sounds.json") => c.sounds += 1,
            ("data", "recipe" | "recipes") if json => c.recipes += 1,
            ("data", "loot_table" | "loot_tables") if json => c.loot_tables += 1,
            ("data", "tags") if json => c.tags += 1,
            ("data", "worldgen") if json => c.worldgen += 1,
            ("data", "structure" | "structures") if n.ends_with(".nbt") => c.structures += 1,
            _ => {}
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_neoforge_metadata() {
        let info = mods_toml(
            r#"modLoader = "javafml"
license = "MIT"
[[mods]]
modId = "ruby"
version = "1.2.0"
displayName = "Ruby"
[[dependencies.ruby]]
modId = "neoforge"
type = "required"
[[dependencies.ruby]]
modId = "geckolib"
type = "required"
[[dependencies.ruby]]
modId = "jei"
type = "optional"
[[dependencies.ruby]]
modId = "curios"
mandatory = true
"#,
        )
        .unwrap();
        assert_eq!(
            (
                info.mod_id.as_str(),
                info.version.as_str(),
                info.license.as_str()
            ),
            ("ruby", "1.2.0", "MIT")
        );
        assert_eq!(info.depends, ["geckolib", "curios"]);
    }

    #[test]
    fn reads_fabric_metadata() {
        let info = fabric(
            r#"{"id": "gem", "version": "3", "name": "Gem", "license": ["MIT", "Apache-2.0"],
                "depends": {"fabricloader": "*", "fabric-api": "*", "minecraft": "1.21.1", "cloth-config": "*"}}"#,
        )
        .unwrap();
        assert_eq!(info.license, "MIT OR Apache-2.0");
        assert_eq!(info.depends, ["cloth-config"]);
    }

    #[test]
    fn reads_quilt_metadata() {
        let info = quilt(
            r#"{"schema_version": 1, "quilt_loader": {"id": "opal", "version": "2.1",
                "metadata": {"name": "Opal", "license": "MIT"},
                "depends": ["quilt_loader", {"id": "quilted_fabric_api"}, "geckolib",
                            {"id": "emi", "optional": true}, {"id": "minecraft"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            (
                info.mod_id.as_str(),
                info.name.as_str(),
                info.version.as_str()
            ),
            ("opal", "Opal", "2.1")
        );
        assert_eq!(info.license, "MIT");
        assert_eq!(info.depends, ["geckolib"]);
    }

    #[test]
    fn counts_files_by_kind() {
        let names = [
            "assets/ruby/blockstates/ruby_block.json",
            "assets/ruby/models/item/ruby.json",
            "assets/ruby/models/block/ruby_block.json",
            "assets/ruby/textures/item/ruby.png",
            "assets/ruby/lang/en_us.json",
            "data/ruby/recipe/ruby.json",
            "data/ruby/loot_table/blocks/ruby_block.json",
            "data/minecraft/tags/block/mineable/pickaxe.json",
            "data/ruby/structure/hut.nbt",
            "com/example/Ruby.class",
            "ruby.mixins.json",
        ]
        .map(String::from);
        let c = census(&names);
        assert_eq!(
            (c.blockstates, c.item_models, c.textures, c.languages),
            (1, 1, 1, 1)
        );
        assert_eq!(
            (c.recipes, c.loot_tables, c.tags, c.structures),
            (1, 1, 1, 1)
        );
        assert_eq!((c.classes, c.mixin_configs), (1, 1));
    }
}
