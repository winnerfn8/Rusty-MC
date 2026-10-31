//! A Java mod's mixins, read from its jar without running anything: its
//! mixin configs (`mixins` in `fabric.mod.json`, `[[mixins]]` in
//! `neoforge.mods.toml`, `mixin` in `quilt.mod.json`), each mixin class's
//! annotations, and the libraries it needs.
//!
//! Rusty Minecraft has no vanilla bytecode to weave mixins into. A mixin can
//! run only where the Rust port of its target method calls a hook point
//! (`native-hooks.json` entries with a `vanilla` method, `docs/native-mods.md`), and
//! only in the Java layer (docs/java-mod-runtime.md). Each mixin member falls
//! in one of three groups:
//!
//! - **supported**: a start, end or cancel injection (`@Inject` at `HEAD`,
//!   `RETURN` or `TAIL`, MixinExtras' `@ModifyReturnValue`) on a vanilla
//!   method that has a hook point there, a `@Shadow` of a method the Java
//!   API layer has ([`crate::api`]), and the mod's own `@Unique` members;
//! - **needs more of the Java layer**: `@Accessor`, `@Invoker`, and `@Shadow`
//!   of a field or of a method the API layer doesn't have yet;
//! - **not supported**: everything else, with why: a change inside a method
//!   (`@Redirect`, `@ModifyArg`, `@ModifyVariable`, `@ModifyConstant`,
//!   `@WrapOperation`...), a whole method replaced (`@Overwrite`), a method
//!   with no hook point yet, a target that isn't vanilla.
//!
//! Only mods made for 26.x get code support: older ones name the game in
//! intermediary or SRG and keep the content-only translation.
use crate::{classes, jar::JarInfo};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Seek},
    sync::OnceLock,
};

const MIXIN: &str = "org/spongepowered/asm/mixin/";
const EXTRAS: &str = "com/llamalad7/mixinextras/";

/// Where a mixin member stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Group {
    Supported,
    NeedsJava,
    Unsupported,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Supported => "supported",
            Group::NeedsJava => "needs more of the Java layer",
            Group::Unsupported => "not supported",
        }
    }
}

/// One mixin member: an injection, accessor, shadow or own member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Injection {
    /// The mixin class, dotted: `com.example.mixin.ServerMixin`.
    pub mixin: String,
    /// The class it applies to, internal name: `net/minecraft/server/MinecraftServer`.
    pub target: String,
    /// The annotation's simple name: `Inject`, `Accessor`, `Shadow`...
    pub kind: String,
    /// The member of the mixin class: the handler, accessor or shadow.
    pub member: String,
    /// Its descriptor, to call it: `(Lorg/.../CallbackInfo;)V`.
    pub descriptor: String,
    pub is_static: bool,
    /// The target method's or field's name (`tickServer`), as the annotation names it.
    pub method: String,
    /// The injection point: `HEAD`, `RETURN`, `TAIL`, `INVOKE`... (`` for none).
    pub at: String,
    pub cancellable: bool,
    pub group: Group,
    /// Why it is in its group.
    pub reason: String,
    /// The hook point it binds to, when supported (`mixin_minecraft_server_tick_server_head`).
    pub hook: String,
    /// The mixin config's list it is in: `client` or `server` (`` for `mixins`, both sides).
    #[serde(default)]
    pub side: String,
}

impl Injection {
    /// The target class's simple name, nested classes dotted: `BlockBehaviour.BlockStateBase`.
    pub fn target_name(&self) -> String {
        simple(&self.target)
    }

    /// One line: what it is, its group and why.
    pub fn line(&self) -> String {
        let mut what = format!("{} @{}", self.target_name(), self.kind);
        if !self.method.is_empty() {
            what = format!("{}.{} @{}", self.target_name(), self.method, self.kind);
        }
        if !self.at.is_empty() {
            what.push_str(&format!(" at {}", self.at));
        }
        if self.cancellable {
            what.push_str(" (cancellable)");
        }
        format!("{what}: {}, {}", self.group.label(), self.reason)
    }
}

/// What a library the mod needs is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryKind {
    /// The loader's API (Fabric API, NeoForge, QSL): it touches Minecraft.
    LoaderApi,
    /// A library that touches Minecraft (Architectury, GeckoLib, Cloth Config).
    Minecraft,
    /// Another mod it needs.
    Mod,
    /// Bundled in the jar and never touching Minecraft: it runs as it is in the JVM.
    Plain,
}

/// A library or mod the jar needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub kind: LibraryKind,
    /// Its Rust version in this repository, when there is one.
    pub rust: Option<String>,
}

impl Library {
    /// Whether the Java layer can run it: plain Java, or a library with a Rust version.
    pub fn runs(&self) -> bool {
        self.kind == LibraryKind::Plain || self.rust.is_some()
    }

    pub fn line(&self) -> String {
        match (self.kind, &self.rust) {
            (LibraryKind::Plain, _) => {
                format!(
                    "{}: plain Java (never touches Minecraft), runs as it is",
                    self.name
                )
            }
            (_, Some(rust)) => format!("{}: Rust version in {rust}", self.name),
            (LibraryKind::Mod, None) => format!("{}: another mod, no Rust version", self.name),
            (_, None) => format!("{}: touches Minecraft, no Rust version", self.name),
        }
    }
}

/// Which Minecraft the mod is made for, as far as code support goes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "version")]
pub enum Era {
    /// 26.x: vanilla's own names, so mixins can bind.
    V26(String),
    /// Older: intermediary or SRG names; content-only.
    Older(String),
    /// It doesn't say, and its classes don't tell.
    #[default]
    Unknown,
}

/// Everything the mod's code needs, read from its jar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Its mixin configs that were found.
    pub configs: Vec<String>,
    /// Mixin classes read.
    pub classes: usize,
    pub injections: Vec<Injection>,
    pub libraries: Vec<Library>,
    pub era: Era,
    /// Mixin config plugins: code that decides at load which mixins apply.
    pub plugins: Vec<String>,
    /// Configs or classes the metadata names that the jar doesn't have.
    pub missing: Vec<String>,
    /// It calls OpenGL itself (LWJGL's `org/lwjgl/opengl`).
    pub opengl: bool,
    /// What its code uses of vanilla's, Fabric's and Mixin's API that the Java layer doesn't have
    /// yet (`SoundType`, `BlockBehaviour.Properties.sound()`), for a 26.x mod ([`crate::api`]).
    #[serde(default)]
    pub api_missing: Vec<String>,
    /// The Java version its classes need (25 for 26.x), 0 when it has none.
    #[serde(default)]
    pub java: u32,
}

impl Report {
    pub fn count(&self, group: Group) -> usize {
        self.injections.iter().filter(|i| i.group == group).count()
    }

    /// Libraries the Java layer can't run yet.
    pub fn missing_libraries(&self) -> Vec<&Library> {
        self.libraries.iter().filter(|l| !l.runs()).collect()
    }

    /// Nothing to say: no mixins, no libraries, no OpenGL.
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
            && self.injections.is_empty()
            && self.libraries.is_empty()
            && self.missing.is_empty()
            && !self.opengl
    }

    /// One line: how much of its code the Java layer could run.
    /// `12 of 15 mixins supported; 2 need more of the Java layer; needs GeckoLib (no Rust version);
    /// uses 3 vanilla or Fabric API classes or members the Java layer doesn't have yet`.
    pub fn headline(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        if let Era::Older(v) = &self.era {
            return format!(
                "Made for Minecraft {v}: code support is for mods made for 26.x, so only its content is translated"
            );
        }
        let mut parts = Vec::new();
        let total = self.injections.len();
        if total > 0 {
            parts.push(format!(
                "{} of {total} {} supported",
                self.count(Group::Supported),
                if total == 1 { "mixin" } else { "mixins" }
            ));
            let java = self.count(Group::NeedsJava);
            if java > 0 {
                parts.push(format!(
                    "{java} {} more of the Java layer",
                    if java == 1 { "needs" } else { "need" }
                ));
            }
        }
        if !self.api_missing.is_empty() {
            let n = self.api_missing.len();
            parts.push(format!(
                "uses {n} {} the Java layer doesn't have yet",
                if n == 1 {
                    "vanilla or Fabric API class or member"
                } else {
                    "vanilla or Fabric API classes or members"
                }
            ));
        }
        let missing = self.missing_libraries();
        if !missing.is_empty() {
            let names: Vec<&str> = missing.iter().map(|l| l.name.as_str()).collect();
            parts.push(format!("needs {} (no Rust version)", list(&names)));
        }
        if self.opengl {
            parts.push("calls OpenGL itself".into());
        }
        if self.era == Era::Unknown && total > 0 {
            parts.push("it doesn't say which Minecraft it is for".into());
        }
        let mut line = parts.join("; ");
        if let Some(first) = line.get(..1) {
            line = first.to_uppercase() + &line[1..];
        }
        line
    }

    /// Every mixin member and library, one line each, mixins grouped
    /// (supported first). A library's group says whether it can run.
    pub fn lines(&self) -> Vec<Line> {
        let line = |group, text| Line { group, text };
        let mut out = Vec::new();
        let mut sorted: Vec<&Injection> = self.injections.iter().collect();
        sorted.sort_by_key(|i| i.group);
        out.extend(sorted.iter().map(|i| line(i.group, i.line())));
        out.extend(self.libraries.iter().map(|l| {
            let group = if l.runs() {
                Group::Supported
            } else {
                Group::Unsupported
            };
            line(group, l.line())
        }));
        out.extend(self.plugins.iter().map(|p| {
            line(
                Group::NeedsJava,
                format!("Mixin plugin {p}: decides at load which mixins apply; runs only in the Java layer"),
            )
        }));
        out.extend(
            self.missing
                .iter()
                .map(|m| line(Group::Unsupported, format!("{m}: named but not in the jar"))),
        );
        const SHOWN: usize = 12;
        out.extend(self.api_missing.iter().take(SHOWN).map(|m| {
            line(
                Group::Unsupported,
                format!("Not in the Java layer yet: {m}"),
            )
        }));
        if self.api_missing.len() > SHOWN {
            out.push(line(
                Group::Unsupported,
                format!(
                    "And {} more not in the Java layer yet",
                    self.api_missing.len() - SHOWN
                ),
            ));
        }
        if self.java > 0 && matches!(self.era, Era::V26(_)) {
            out.push(line(
                Group::NeedsJava,
                format!("Needs Java {} or newer to run its code", self.java),
            ));
        }
        if self.opengl {
            out.push(line(
                Group::Unsupported,
                "Calls OpenGL itself: 26.x draws through Blaze3D (RenderPipeline) and Rusty Minecraft through wgpu, so direct OpenGL calls can't run"
                    .into(),
            ));
        }
        out
    }
}

/// One line of a [`Report`], with the group that colours it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub group: Group,
    pub text: String,
}

fn list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A class's simple name, nested classes dotted.
fn simple(internal: &str) -> String {
    internal
        .rsplit('/')
        .next()
        .unwrap_or(internal)
        .replace('$', ".")
}

/// A hook point a mixin can bind to: a vanilla method's start or end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPoint {
    /// The catalog name: `mixin_minecraft_server_tick_server_head`.
    pub name: String,
    /// The class, internal name.
    pub class: String,
    pub method: String,
    /// `head` or `return`.
    pub at: String,
}

/// The mixin hook points in `native-hooks.json` (entries with `vanilla`, `class` and `at`).
pub fn hook_points() -> &'static [HookPoint] {
    static POINTS: OnceLock<Vec<HookPoint>> = OnceLock::new();
    POINTS.get_or_init(|| hook_points_in(include_str!("../../../native-hooks.json")))
}

pub fn hook_points_in(catalog: &str) -> Vec<HookPoint> {
    let v: serde_json::Value = serde_json::from_str(catalog).unwrap_or_default();
    v["hooks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let vanilla = h["vanilla"].as_str()?;
            let method = vanilla.rsplit('.').next()?.to_owned();
            Some(HookPoint {
                name: h["name"].as_str()?.to_owned(),
                class: h["class"].as_str()?.to_owned(),
                method,
                at: h["at"].as_str()?.to_owned(),
            })
        })
        .collect()
}

fn find_hook<'a>(
    points: &'a [HookPoint],
    class: &str,
    method: &str,
    at: &str,
) -> Option<&'a HookPoint> {
    points
        .iter()
        .find(|p| p.class == class && p.method == method && p.at == at)
}

/// Known loader APIs and libraries that touch Minecraft, by mod id. None has
/// a Rust version yet; one that gets one names it here.
const KNOWN: &[(&str, &str, LibraryKind, Option<&str>)] = &[
    ("fabric-api", "Fabric API", LibraryKind::LoaderApi, None),
    (
        "quilted_fabric_api",
        "Quilted Fabric API",
        LibraryKind::LoaderApi,
        None,
    ),
    (
        "qsl",
        "Quilt Standard Libraries",
        LibraryKind::LoaderApi,
        None,
    ),
    ("neoforge", "NeoForge", LibraryKind::LoaderApi, None),
    ("forge", "Forge", LibraryKind::LoaderApi, None),
    (
        "fabric-language-kotlin",
        "Fabric Language Kotlin",
        LibraryKind::LoaderApi,
        None,
    ),
    (
        "kotlinforforge",
        "Kotlin for Forge",
        LibraryKind::LoaderApi,
        None,
    ),
    (
        "architectury",
        "Architectury API",
        LibraryKind::Minecraft,
        None,
    ),
    ("geckolib", "GeckoLib", LibraryKind::Minecraft, None),
    ("cloth-config", "Cloth Config", LibraryKind::Minecraft, None),
    ("cloth_config", "Cloth Config", LibraryKind::Minecraft, None),
    (
        "yet_another_config_lib_v3",
        "YetAnotherConfigLib",
        LibraryKind::Minecraft,
        None,
    ),
    ("owo", "oωo", LibraryKind::Minecraft, None),
    ("modmenu", "Mod Menu", LibraryKind::Minecraft, None),
    ("trinkets", "Trinkets", LibraryKind::Minecraft, None),
    ("curios", "Curios API", LibraryKind::Minecraft, None),
    (
        "cardinal-components-base",
        "Cardinal Components",
        LibraryKind::Minecraft,
        None,
    ),
    ("puzzleslib", "Puzzles Lib", LibraryKind::Minecraft, None),
    ("balm", "Balm", LibraryKind::Minecraft, None),
    ("moonlight", "Moonlight Lib", LibraryKind::Minecraft, None),
    (
        "resourcefullib",
        "Resourceful Lib",
        LibraryKind::Minecraft,
        None,
    ),
    (
        "resourcefulconfig",
        "Resourceful Config",
        LibraryKind::Minecraft,
        None,
    ),
    ("terrablender", "TerraBlender", LibraryKind::Minecraft, None),
    ("midnightlib", "MidnightLib", LibraryKind::Minecraft, None),
    (
        "forgeconfigapiport",
        "Forge Config API Port",
        LibraryKind::Minecraft,
        None,
    ),
    ("fzzy_config", "Fzzy Config", LibraryKind::Minecraft, None),
    ("collective", "Collective", LibraryKind::Minecraft, None),
];

/// A required id as a library: Fabric API's modules (`fabric-lifecycle-events-v1`)
/// and QSL's count as Fabric API and QSL.
fn library_of(id: &str) -> Library {
    let canonical =
        if id == "fabric" || id.starts_with("fabric-") && !id.starts_with("fabric-language") {
            "fabric-api"
        } else if id.starts_with("quilt_") && id != "quilt_loader" || id.starts_with("qsl") {
            "qsl"
        } else if id.starts_with("cloth-config") {
            "cloth-config"
        } else if id.starts_with("geckolib") {
            "geckolib"
        } else if id.starts_with("cardinal-components") {
            "cardinal-components-base"
        } else {
            id
        };
    // Fabric API's modules with a Rust version here (crates/horizon-java): each its own line,
    // and Fabric API as a whole has that part.
    let modules = crate::api::api().modules();
    if modules.iter().any(|m| m == id) {
        return Library {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: LibraryKind::LoaderApi,
            rust: Some("crates/horizon-java".into()),
        };
    }
    if canonical == "fabric-api" {
        return Library {
            id: canonical.to_owned(),
            name: "Fabric API".into(),
            kind: LibraryKind::LoaderApi,
            rust: Some(format!(
                "crates/horizon-java for its part {} (what else a mod uses of it is listed)",
                modules.join(", ")
            )),
        };
    }
    match KNOWN.iter().find(|k| k.0 == canonical) {
        Some((id, name, kind, rust)) => Library {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            kind: *kind,
            rust: rust.map(str::to_owned),
        },
        None => Library {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: LibraryKind::Mod,
            rust: None,
        },
    }
}

/// The era a version requirement (`~1.21.1`, `[26.1,27)`, `>=26.2`) names.
pub fn era_of(requirement: &str) -> Era {
    let mut majors = Vec::new();
    let mut lowest: Option<(u32, String)> = None;
    for token in
        requirement.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'x' || c == '*'))
    {
        let token = token.trim_matches('.');
        let Some(major) = token.split('.').next().and_then(|m| m.parse::<u32>().ok()) else {
            continue;
        };
        majors.push(major);
        if lowest.as_ref().is_none_or(|(m, _)| major < *m) {
            lowest = Some((major, token.to_owned()));
        }
    }
    match lowest {
        None => Era::Unknown,
        // 1.x anywhere in the range: built against an older game.
        Some((1, v)) => Era::Older(v),
        Some((major, v)) if major >= 26 => Era::V26(v),
        Some((_, v)) => Era::Older(v),
    }
}

/// Intermediary (`net/minecraft/class_1234`) or SRG (`m_1234_`) names: built for an older game.
fn obfuscated(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    name.rsplit('/')
        .next()
        .and_then(|s| s.strip_prefix("class_"))
        .is_some_and(digits)
        || name.strip_prefix("method_").is_some_and(digits)
        || name
            .strip_prefix("m_")
            .and_then(|s| s.strip_suffix('_'))
            .is_some_and(digits)
}

/// A vanilla method as mixins target it across several mods: which methods most deserve hook
/// points (`horizon-port-pack mixin-targets`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// `Class.method`, the class's simple name.
    pub method: String,
    /// The class, internal name.
    pub class: String,
    /// Mods with at least one injection into it.
    pub mods: usize,
    /// Start injections (`HEAD`).
    pub starts: usize,
    /// End injections (`RETURN`, `TAIL`, `@ModifyReturnValue`).
    pub ends: usize,
    /// Changes inside it (`INVOKE`, `@Redirect`, `@ModifyArg`...) and `@Overwrite`.
    pub inside: usize,
    /// It has a hook point.
    pub hooked: bool,
}

/// The vanilla methods `reports` inject into, most mods first (then most injections). Accessors,
/// invokers, shadows and own members reach fields and methods without a hook point: left out.
pub fn rank(reports: &[Report]) -> Vec<Target> {
    use std::collections::BTreeMap;
    let mut by: BTreeMap<(String, String), (BTreeSet<usize>, Target)> = BTreeMap::new();
    for (n, r) in reports.iter().enumerate() {
        for i in &r.injections {
            let vanilla =
                i.target.starts_with("net/minecraft/") || i.target.starts_with("com/mojang/");
            if !vanilla
                || i.method.is_empty()
                || matches!(
                    i.kind.as_str(),
                    "Accessor" | "Invoker" | "Shadow" | "Unique"
                )
            {
                continue;
            }
            let (mods, t) = by
                .entry((i.target.clone(), i.method.clone()))
                .or_insert_with(|| {
                    (
                        BTreeSet::new(),
                        Target {
                            method: format!("{}.{}", i.target_name(), i.method),
                            class: i.target.clone(),
                            mods: 0,
                            starts: 0,
                            ends: 0,
                            inside: 0,
                            hooked: false,
                        },
                    )
                });
            mods.insert(n);
            match (i.kind.as_str(), i.at.as_str()) {
                ("Inject", "HEAD") => t.starts += 1,
                ("Inject", "RETURN" | "TAIL") | ("ModifyReturnValue", _) => t.ends += 1,
                _ => t.inside += 1,
            }
            t.hooked |= !i.hook.is_empty()
                || hook_points()
                    .iter()
                    .any(|p| p.class == i.target && p.method == i.method);
        }
    }
    let mut out: Vec<Target> = by
        .into_values()
        .map(|(mods, mut t)| {
            t.mods = mods.len();
            t
        })
        .collect();
    out.sort_by(|a, b| {
        (b.mods, b.starts + b.ends + b.inside)
            .cmp(&(a.mods, a.starts + a.ends + a.inside))
            .then_with(|| a.method.cmp(&b.method))
    });
    out
}

/// Reads the mod's mixins and libraries from its jar.
pub fn read<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, info: &JarInfo, loader: &str) -> Report {
    read_with(zip, info, loader, hook_points())
}

/// [`read`] against the given hook points.
pub fn read_with<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    info: &JarInfo,
    loader: &str,
    points: &[HookPoint],
) -> Report {
    let mut report = Report {
        era: era_of(&info.minecraft),
        ..Report::default()
    };
    let mut seen = BTreeSet::new();
    for id in &info.requires {
        let lib = library_of(id);
        if seen.insert(lib.id.clone()) {
            report.libraries.push(lib);
        }
    }
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    report.libraries.extend(bundled(zip, &names, &mut seen));
    // Its own classes (not bundled jars'): OpenGL, the API it uses, the Java it needs.
    let mut used_classes = BTreeSet::new();
    let mut used_members = BTreeSet::new();
    for n in names.iter().filter(|n| n.ends_with(".class")) {
        let Some(c) = bytes(zip, n).and_then(|b| classes::parse(&b)) else {
            continue;
        };
        report.opengl |= c.refs.iter().any(|r| r.starts_with("org/lwjgl/opengl/"));
        report.java = report.java.max(c.java);
        used_classes.extend(c.refs.into_iter().filter(|r| crate::api::on_surface(r)));
        used_members.extend(
            c.members
                .into_iter()
                .filter(|m| crate::api::on_surface(&m.0)),
        );
    }

    let mut mixin_classes = Vec::new();
    for config in &info.mixin_configs {
        let Some(text) = bytes(zip, config) else {
            report.missing.push(config.clone());
            continue;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&text) else {
            report.missing.push(format!("{config} (not valid JSON)"));
            continue;
        };
        report.configs.push(config.clone());
        let package = v["package"].as_str().unwrap_or("").replace('.', "/");
        if let Some(p) = v["plugin"].as_str() {
            report.plugins.push(p.to_owned());
        }
        for side in ["mixins", "client", "server"] {
            for name in v[side]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| n.as_str())
            {
                let path = if package.is_empty() {
                    name.replace('.', "/")
                } else {
                    format!("{package}/{}", name.replace('.', "/"))
                };
                let side = if side == "mixins" { "" } else { side };
                mixin_classes.push((path, side));
            }
        }
    }
    let mut parsed = Vec::new();
    for (path, side) in mixin_classes {
        match bytes(zip, &format!("{path}.class")).and_then(|b| classes::parse_annotated(&b)) {
            Some(c) => parsed.push((c, side)),
            None => report.missing.push(format!("{}.class", path)),
        }
    }
    report.classes = parsed.len();
    // A Fabric or Quilt jar names the game in intermediary unless it is for 26.x.
    let targets = || parsed.iter().flat_map(|(c, _)| mixin_targets(c));
    if targets().any(|t| obfuscated(&t)) {
        if !matches!(report.era, Era::Older(_)) {
            report.era =
                Era::Older("1.21 or older (its classes use intermediary or SRG names)".into());
        }
    } else if report.era == Era::Unknown
        && matches!(loader, "fabric" | "quilt")
        && targets().any(|t| t.starts_with("net/minecraft/"))
    {
        report.era = Era::V26("26.x (its classes use vanilla's names)".into());
    }
    if matches!(report.era, Era::V26(_)) {
        report.api_missing = crate::api::missing(
            used_classes.iter().map(String::as_str),
            used_members
                .iter()
                .map(|(c, n, d)| (c.as_str(), n.as_str(), d.as_str())),
        );
    }
    for (c, side) in &parsed {
        let from = report.injections.len();
        members(c, &report.era, points, &mut report.injections);
        for i in &mut report.injections[from..] {
            i.side = (*side).to_owned();
        }
    }
    report
}

fn bytes<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut e = zip.by_name(name).ok()?;
    if e.size() > 1 << 24 {
        return None;
    }
    let mut b = Vec::new();
    e.read_to_end(&mut b).ok()?;
    Some(b)
}

/// Jars bundled inside (Fabric's `META-INF/jars/`, NeoForge's `META-INF/jarjar/`):
/// plain Java unless their classes name Minecraft or a loader.
fn bundled<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    names: &[String],
    seen: &mut BTreeSet<String>,
) -> Vec<Library> {
    let mut out = Vec::new();
    for n in names.iter().filter(|n| {
        (n.starts_with("META-INF/jars/") || n.starts_with("META-INF/jarjar/"))
            && n.ends_with(".jar")
    }) {
        let Some(b) = bytes(zip, n) else { continue };
        let Ok(mut inner) = zip::ZipArchive::new(std::io::Cursor::new(b)) else {
            continue;
        };
        let meta = crate::jar::read_archive(&mut inner).unwrap_or_default();
        let file = n.rsplit('/').next().unwrap_or(n).trim_end_matches(".jar");
        let id = if meta.mod_id.is_empty() {
            file.to_owned()
        } else {
            meta.mod_id.clone()
        };
        let touches = inner
            .file_names()
            .filter(|f| f.ends_with(".class"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .iter()
            .any(|f| {
                bytes(&mut inner, f)
                    .and_then(|b| classes::parse(&b))
                    .is_some_and(|c| {
                        c.refs.iter().chain([&c.superclass]).any(|r| {
                            r.starts_with("net/minecraft/")
                                || r.starts_with("com/mojang/")
                                || r.starts_with("net/fabricmc/")
                                || r.starts_with("net/neoforged/")
                                || r.starts_with("org/quiltmc/")
                        })
                    })
            });
        let mut lib = library_of(&id);
        if !touches {
            lib = Library {
                id: id.clone(),
                name: if meta.name.is_empty() {
                    file.to_owned()
                } else {
                    meta.name.clone()
                },
                kind: LibraryKind::Plain,
                rust: None,
            };
        } else if lib.kind == LibraryKind::Mod {
            lib.kind = LibraryKind::Minecraft;
            if !meta.name.is_empty() {
                lib.name = meta.name.clone();
            }
        }
        if seen.insert(lib.id.clone()) {
            out.push(lib);
        }
    }
    out
}

/// The classes a mixin applies to: `@Mixin(value = {...}, targets = {...})`.
fn mixin_targets(c: &classes::Annotated) -> Vec<String> {
    let Some(m) = c
        .annotations
        .iter()
        .find(|a| a.kind == format!("L{MIXIN}Mixin;"))
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = m
        .get("value")
        .map(|v| v.classes())
        .unwrap_or_default()
        .into_iter()
        .map(str::to_owned)
        .collect();
    out.extend(
        m.strings("targets")
            .into_iter()
            .map(|t| t.replace('.', "/")),
    );
    out
}

/// A target method as written (`tickServer`, `tickServer(Ljava/...;)V`,
/// `Lnet/.../MinecraftServer;tickServer(...)V`, `tick*`): its name.
fn method_name(written: &str) -> String {
    let after_owner = match written.find(';') {
        Some(i) if written.starts_with('L') && written[..i].contains('/') => &written[i + 1..],
        _ => written,
    };
    let name = after_owner.split(['(', ':']).next().unwrap_or(after_owner);
    name.trim_end_matches('*').to_owned()
}

/// Accessor and invoker names say their target: `getFoodData` → `foodData`.
fn accessor_target(member: &str, prefixes: &[&str]) -> String {
    for p in prefixes {
        if let Some(rest) = member.strip_prefix(p)
            && let Some(first) = rest.chars().next()
            && first.is_uppercase()
        {
            return first.to_lowercase().chain(rest.chars().skip(1)).collect();
        }
    }
    member.to_owned()
}

/// Each annotated member of a mixin class, grouped.
fn members(c: &classes::Annotated, era: &Era, points: &[HookPoint], out: &mut Vec<Injection>) {
    let targets = mixin_targets(c);
    let target = targets.first().cloned().unwrap_or_default();
    let mixin = c.name.replace('/', ".");
    let all = c
        .methods
        .iter()
        .map(|m| (m, true))
        .chain(c.fields.iter().map(|f| (f, false)));
    for (member, is_method) in all {
        for a in &member.annotations {
            let mixin_ann = a.kind.starts_with(&format!("L{MIXIN}"))
                || a.kind.starts_with(&format!("L{EXTRAS}"));
            if !mixin_ann {
                continue;
            }
            let kind = a.name().to_owned();
            if matches!(
                kind.as_str(),
                "Final"
                    | "Mutable"
                    | "Dynamic"
                    | "Debug"
                    | "Intrinsic"
                    | "Implements"
                    | "Interface"
                    | "Coerce"
                    | "Local"
                    | "Share"
                    | "Cancellable"
                    | "Group"
                    | "Slice"
                    | "At"
                    | "Desc"
                    | "Definition"
                    | "Expression"
            ) {
                continue;
            }
            let ats: Vec<String> = a
                .get("at")
                .map(|v| {
                    v.annotations()
                        .iter()
                        .flat_map(|at| at.strings("value"))
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let mut methods: Vec<String> =
                a.strings("method").iter().map(|m| method_name(m)).collect();
            match kind.as_str() {
                "Accessor" => {
                    methods = vec![
                        a.strings("value")
                            .first()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                accessor_target(&member.name, &["get", "set", "is"])
                            }),
                    ]
                }
                "Invoker" => {
                    methods = vec![
                        a.strings("value")
                            .first()
                            .map(|s| method_name(s))
                            .unwrap_or_else(|| accessor_target(&member.name, &["invoke", "call"])),
                    ]
                }
                "Shadow" | "Unique" | "Overwrite" => methods = vec![member.name.clone()],
                _ => {}
            }
            if methods.is_empty() {
                methods.push(String::new());
            }
            for method in methods {
                for t in &targets {
                    let at = ats.first().cloned().unwrap_or_default();
                    let (group, reason, hook) = classify(
                        &kind,
                        t,
                        &method,
                        &member.descriptor,
                        &at,
                        is_method,
                        era,
                        points,
                    );
                    out.push(Injection {
                        mixin: mixin.clone(),
                        target: t.clone(),
                        kind: kind.clone(),
                        member: member.name.clone(),
                        descriptor: member.descriptor.clone(),
                        is_static: member.is_static(),
                        method: method.clone(),
                        at,
                        cancellable: a.flag("cancellable"),
                        group,
                        reason,
                        hook,
                        side: String::new(),
                    });
                }
                if targets.is_empty() {
                    out.push(Injection {
                        mixin: mixin.clone(),
                        target: target.clone(),
                        kind: kind.clone(),
                        member: member.name.clone(),
                        descriptor: member.descriptor.clone(),
                        is_static: member.is_static(),
                        method: method.clone(),
                        at: ats.first().cloned().unwrap_or_default(),
                        cancellable: a.flag("cancellable"),
                        group: Group::Unsupported,
                        reason: "its @Mixin names no class".into(),
                        hook: String::new(),
                        side: String::new(),
                    });
                }
            }
        }
    }
}

/// A member's group, why, and the hook point when supported.
#[allow(clippy::too_many_arguments)]
fn classify(
    kind: &str,
    target: &str,
    method: &str,
    descriptor: &str,
    at: &str,
    is_method: bool,
    era: &Era,
    points: &[HookPoint],
) -> (Group, String, String) {
    let no = |why: String| (Group::Unsupported, why, String::new());
    if let Era::Older(v) = era {
        return no(format!(
            "made for Minecraft {v}: code support is for 26.x mods only"
        ));
    }
    if *era == Era::Unknown {
        return no(
            "the mod doesn't say which Minecraft it is for, and code support is for 26.x mods only"
                .into(),
        );
    }
    let vanilla = target.starts_with("net/minecraft/") || target.starts_with("com/mojang/");
    if !vanilla {
        return no(format!(
            "it changes {}, not a vanilla class: only vanilla methods have hook points",
            target.replace('/', ".")
        ));
    }
    let java = |why: &str| (Group::NeedsJava, why.to_owned(), String::new());
    let name = format!("{}.{method}", simple(target));
    match kind {
        "Accessor" => {
            java("reads or writes vanilla's field: the Java layer doesn't make accessors yet")
        }
        "Invoker" => java("calls vanilla's method: the Java layer doesn't make invokers yet"),
        "Shadow" if is_method && crate::api::api().has_method(target, method, descriptor) => (
            Group::Supported,
            format!("calls the Java layer's {name}"),
            String::new(),
        ),
        "Shadow" if is_method => java(&format!(
            "calls vanilla's {name}, which the Java layer doesn't have yet"
        )),
        "Shadow" => java(
            "reads vanilla's field: the Java layer keeps it as the mixin's own, not the game's value",
        ),
        "Unique" => (
            Group::Supported,
            "the mod's own member, kept on the object standing for the target".into(),
            String::new(),
        ),
        "Inject" | "ModifyReturnValue" => {
            let point = match (kind, at) {
                (_, "HEAD") => "head",
                (_, "RETURN") | (_, "TAIL") | ("ModifyReturnValue", _) => "return",
                (_, other) => {
                    return no(format!(
                        "it injects at {} inside {name}: only a method's start and end have hook points",
                        if other.is_empty() {
                            "a point it doesn't name"
                        } else {
                            other
                        }
                    ));
                }
            };
            match find_hook(points, target, method, point) {
                Some(h) => {
                    let mut why = format!("binds to hook point {}", h.name);
                    if at == "TAIL" {
                        why.push_str(" (it also runs after early returns, where TAIL wouldn't)");
                    }
                    (Group::Supported, why, h.name.clone())
                }
                None => no(format!(
                    "no hook point at the {} of {name} yet",
                    if point == "head" { "start" } else { "end" }
                )),
            }
        }
        "Overwrite" => no(format!("it replaces the whole of {name}")),
        "Redirect" | "ModifyArg" | "ModifyArgs" | "WrapOperation" | "WrapWithCondition"
        | "ModifyReceiver" => no(format!(
            "it changes a call inside {name}, and there is no vanilla bytecode to change"
        )),
        "ModifyVariable" => no(format!("it changes a local variable inside {name}")),
        "ModifyConstant" => no(format!("it changes a constant inside {name}")),
        "ModifyExpressionValue" => no(format!("it changes a value computed inside {name}")),
        "WrapMethod" => no(format!("it wraps the whole of {name}")),
        other => no(format!("@{other} isn't read yet")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::{
        Value,
        tests_support::{ann, annotated_class, class_with, member, s},
    };
    use std::io::Write;

    const INJECT: &str = "org/spongepowered/asm/mixin/injection/Inject";
    const AT: &str = "org/spongepowered/asm/mixin/injection/At";
    const CI: &str = "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V";

    fn at(value: &str) -> Value {
        Value::Array(vec![Value::Annotation(ann(AT, vec![("value", s(value))]))])
    }

    fn mixin_of(class: &str) -> crate::classes::Annotation {
        ann(
            "org/spongepowered/asm/mixin/Mixin",
            vec![(
                "value",
                Value::Array(vec![Value::Class(format!("L{class};"))]),
            )],
        )
    }

    fn zip_of(files: &[(&str, Vec<u8>)]) -> zip::ZipArchive<std::io::Cursor<Vec<u8>>> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(bytes).unwrap();
        }
        zip::ZipArchive::new(std::io::Cursor::new(w.finish().unwrap().into_inner())).unwrap()
    }

    fn points() -> Vec<HookPoint> {
        let point = |method: &str, class: &str, at: &str| HookPoint {
            name: format!("mixin_{method}_{at}"),
            class: class.into(),
            method: method.into(),
            at: at.into(),
        };
        vec![
            point("tickServer", "net/minecraft/server/MinecraftServer", "head"),
            point(
                "tickServer",
                "net/minecraft/server/MinecraftServer",
                "return",
            ),
            point(
                "getDestroyProgress",
                "net/minecraft/world/level/block/state/BlockBehaviour",
                "return",
            ),
        ]
    }

    /// A small 26.x Fabric mod: one mixin of each kind, Fabric API, a bundled plain library, a GL call.
    fn fabric_mod(minecraft: &str, server: &str) -> Vec<(&'static str, Vec<u8>)> {
        let meta = format!(
            r#"{{"schemaVersion": 1, "id": "tweaks", "version": "1.0", "name": "Tweaks",
                "depends": {{"fabricloader": "*", "minecraft": "{minecraft}", "fabric-lifecycle-events-v1": "*", "java": ">=21"}},
                "mixins": ["tweaks.mixins.json", {{"config": "tweaks.client.mixins.json", "environment": "client"}}, "gone.mixins.json"]}}"#
        );
        let config = br#"{"package": "tw.mixin", "plugin": "tw.Plugin", "mixins": ["ServerMixin", "PlayerAccessor", "OtherMixin"], "server": [], "client": ["client.BlockMixin"]}"#;
        let client = br#"{"package": "tw.mixin", "client": []}"#;
        let server_mixin = annotated_class(
            "tw/mixin/ServerMixin",
            0x0421,
            &[mixin_of(server)],
            &[member(
                "ticks",
                "I",
                vec![ann("org/spongepowered/asm/mixin/Shadow", vec![])],
            )],
            &[
                member(
                    "onTick",
                    &format!("(Ljava/util/function/BooleanSupplier;{}", &CI[1..]),
                    vec![ann(
                        INJECT,
                        vec![
                            (
                                "method",
                                Value::Array(vec![s(
                                    "tickServer(Ljava/util/function/BooleanSupplier;)V",
                                )]),
                            ),
                            ("at", at("HEAD")),
                            ("cancellable", Value::Int(1)),
                        ],
                    )],
                ),
                member(
                    "afterTick",
                    CI,
                    vec![ann(
                        INJECT,
                        vec![
                            ("method", Value::Array(vec![s("tickServer")])),
                            ("at", at("TAIL")),
                        ],
                    )],
                ),
                member(
                    "midTick",
                    CI,
                    vec![ann(
                        INJECT,
                        vec![
                            ("method", Value::Array(vec![s("tickServer")])),
                            ("at", at("INVOKE")),
                        ],
                    )],
                ),
                member(
                    "noSave",
                    "()V",
                    vec![ann(
                        "org/spongepowered/asm/mixin/injection/Redirect",
                        vec![
                            ("method", Value::Array(vec![s("saveEverything")])),
                            (
                                "at",
                                Value::Annotation(ann(AT, vec![("value", s("INVOKE"))])),
                            ),
                        ],
                    )],
                ),
                member(
                    "own",
                    "()V",
                    vec![ann("org/spongepowered/asm/mixin/Unique", vec![])],
                ),
            ],
        );
        let accessor = annotated_class(
            "tw/mixin/PlayerAccessor",
            0x0601,
            &[mixin_of("net/minecraft/world/entity/player/Player")],
            &[],
            &[member(
                "getFoodData",
                "()Lnet/minecraft/world/food/FoodData;",
                vec![ann("org/spongepowered/asm/mixin/gen/Accessor", vec![])],
            )],
        );
        let other = annotated_class(
            "tw/mixin/OtherMixin",
            0x0421,
            &[ann(
                "org/spongepowered/asm/mixin/Mixin",
                vec![("targets", Value::Array(vec![s("com.other.Thing")]))],
            )],
            &[],
            &[member(
                "x",
                CI,
                vec![ann(
                    INJECT,
                    vec![("method", Value::Array(vec![s("run")])), ("at", at("HEAD"))],
                )],
            )],
        );
        let block = annotated_class(
            "tw/mixin/client/BlockMixin",
            0x0421,
            &[mixin_of(
                "net/minecraft/world/level/block/state/BlockBehaviour",
            )],
            &[],
            &[member(
                "faster",
                "(F)F",
                vec![ann(
                    "com/llamalad7/mixinextras/injector/ModifyReturnValue",
                    vec![
                        ("method", Value::Array(vec![s("getDestroyProgress")])),
                        (
                            "at",
                            Value::Annotation(ann(AT, vec![("value", s("RETURN"))])),
                        ),
                    ],
                )],
            )],
        );
        let gl = class_with(
            "tw/Draw",
            "java/lang/Object",
            &[],
            &["org/lwjgl/opengl/GL11"],
            false,
        );
        let plain_lib = {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            w.start_file(
                "tiny/json/Json.class",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            w.write_all(&class_with(
                "tiny/json/Json",
                "java/lang/Object",
                &[],
                &["java/util/Map"],
                false,
            ))
            .unwrap();
            w.finish().unwrap().into_inner()
        };
        vec![
            ("fabric.mod.json", meta.into_bytes()),
            ("tweaks.mixins.json", config.to_vec()),
            ("tweaks.client.mixins.json", client.to_vec()),
            ("tw/mixin/ServerMixin.class", server_mixin),
            ("tw/mixin/PlayerAccessor.class", accessor),
            ("tw/mixin/OtherMixin.class", other),
            ("tw/mixin/client/BlockMixin.class", block),
            ("tw/Draw.class", gl),
            ("META-INF/jars/tiny-json-2.0.jar", plain_lib),
        ]
    }

    fn report_of(files: Vec<(&'static str, Vec<u8>)>, loader: &str) -> Report {
        let mut zip = zip_of(&files);
        let info = crate::jar::read_archive(&mut zip).unwrap();
        read_with(&mut zip, &info, loader, &points())
    }

    #[test]
    fn groups_each_mixin_with_a_reason() {
        let r = report_of(
            fabric_mod(">=26.1", "net/minecraft/server/MinecraftServer"),
            "fabric",
        );
        assert_eq!(r.era, Era::V26("26.1".into()));
        assert_eq!(
            r.configs,
            ["tweaks.mixins.json", "tweaks.client.mixins.json"]
        );
        assert_eq!(r.missing, ["gone.mixins.json"]);
        assert_eq!(r.plugins, ["tw.Plugin"]);
        assert_eq!(r.classes, 4);
        let find = |member: &str| r.injections.iter().find(|i| i.member == member).unwrap();

        let head = find("onTick");
        assert_eq!(
            (head.group, head.at.as_str(), head.cancellable),
            (Group::Supported, "HEAD", true)
        );
        assert_eq!(head.method, "tickServer");
        assert_eq!(head.hook, "mixin_tickServer_head");
        assert_eq!(
            head.descriptor,
            "(Ljava/util/function/BooleanSupplier;Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V"
        );
        let tail = find("afterTick");
        assert_eq!(
            (tail.group, tail.hook.as_str()),
            (Group::Supported, "mixin_tickServer_return")
        );
        assert!(tail.reason.contains("early returns"), "{}", tail.reason);
        let faster = find("faster");
        assert_eq!(
            (faster.group, faster.kind.as_str()),
            (Group::Supported, "ModifyReturnValue")
        );

        for (m, kind, why) in [
            (
                "midTick",
                Group::Unsupported,
                "inside MinecraftServer.tickServer",
            ),
            (
                "noSave",
                Group::Unsupported,
                "changes a call inside MinecraftServer.saveEverything",
            ),
            (
                "x",
                Group::Unsupported,
                "com.other.Thing, not a vanilla class",
            ),
            ("ticks", Group::NeedsJava, "field"),
            ("getFoodData", Group::NeedsJava, "field"),
            ("own", Group::Supported, "own member"),
        ] {
            let i = find(m);
            assert_eq!(i.group, kind, "{m}: {}", i.reason);
            assert!(i.reason.contains(why), "{m}: {}", i.reason);
        }
        assert_eq!(find("getFoodData").method, "foodData");

        assert!(r.opengl);
        // Fabric API's lifecycle events module has a Rust version (crates/horizon-java).
        let events = r
            .libraries
            .iter()
            .find(|l| l.id == "fabric-lifecycle-events-v1")
            .unwrap();
        assert!(events.runs());
        let json = r
            .libraries
            .iter()
            .find(|l| l.id == "tiny-json-2.0")
            .unwrap();
        assert_eq!(json.kind, LibraryKind::Plain);
        assert!(json.runs());
        assert_eq!(
            r.headline(),
            "4 of 9 mixins supported; 2 need more of the Java layer; calls OpenGL itself"
        );
        let lines: Vec<String> = r.lines().into_iter().map(|l| l.text).collect();
        assert!(lines[0].starts_with("MinecraftServer.tickServer @Inject at HEAD (cancellable): supported, binds to hook point"), "{lines:#?}");
        assert!(
            lines
                .iter()
                .any(|l| l == "fabric-lifecycle-events-v1: Rust version in crates/horizon-java"),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("tiny-json-2.0: plain Java")),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("Mixin plugin tw.Plugin")),
            "{lines:#?}"
        );
    }

    #[test]
    fn older_mods_keep_the_content_only_translation() {
        // Fabric before 26.1: the game in intermediary.
        let r = report_of(fabric_mod("~1.21.1", "net/minecraft/class_3176"), "fabric");
        assert_eq!(r.era, Era::Older("1.21.1".into()));
        assert_eq!(r.count(Group::Supported) + r.count(Group::NeedsJava), 0);
        assert!(
            r.injections
                .iter()
                .all(|i| i.reason.contains("code support is for 26.x"))
        );
        assert!(
            r.headline()
                .starts_with("Made for Minecraft 1.21.1: code support is for mods made for 26.x"),
            "{}",
            r.headline()
        );
        // Saying nothing, but naming the game in intermediary, is older too.
        let r = report_of(fabric_mod("*", "net/minecraft/class_3176"), "fabric");
        assert!(matches!(r.era, Era::Older(_)), "{:?}", r.era);
        // Saying nothing with vanilla's names: a Fabric jar for 26.x.
        let r = report_of(
            fabric_mod("*", "net/minecraft/server/MinecraftServer"),
            "fabric",
        );
        assert!(matches!(r.era, Era::V26(_)), "{:?}", r.era);
    }

    #[test]
    fn neoforge_names_its_configs_and_game_in_mods_toml() {
        let toml = br#"modLoader = "javafml"
license = "MIT"
[[mods]]
modId = "nf"
version = "1.0"
[[mixins]]
config = "nf.mixins.json"
[[dependencies.nf]]
modId = "neoforge"
type = "required"
versionRange = "[26.3,)"
[[dependencies.nf]]
modId = "minecraft"
type = "required"
versionRange = "[26.3,27)"
[[dependencies.nf]]
modId = "geckolib"
type = "optional"
"#;
        let mixin = annotated_class(
            "nf/M",
            0x0421,
            &[mixin_of("net/minecraft/server/MinecraftServer")],
            &[],
            &[member(
                "t",
                CI,
                vec![ann(
                    INJECT,
                    vec![
                        ("method", Value::Array(vec![s("tickServer")])),
                        ("at", at("RETURN")),
                    ],
                )],
            )],
        );
        let r = report_of(
            vec![
                ("META-INF/neoforge.mods.toml", toml.to_vec()),
                (
                    "nf.mixins.json",
                    br#"{"package": "nf", "mixins": ["M"]}"#.to_vec(),
                ),
                ("nf/M.class", mixin),
            ],
            "neoforge",
        );
        assert_eq!(r.era, Era::V26("26.3".into()));
        assert_eq!(r.injections[0].group, Group::Supported);
        assert_eq!(
            r.headline(),
            "1 of 1 mixin supported; needs NeoForge (no Rust version)"
        );
    }

    /// Two mods into `tickServer` rank it above a method one mod changes from inside.
    #[test]
    fn ranks_the_most_targeted_methods_first() {
        let a = report_of(
            fabric_mod(">=26.1", "net/minecraft/server/MinecraftServer"),
            "fabric",
        );
        let b = report_of(
            fabric_mod("26.3", "net/minecraft/server/MinecraftServer"),
            "fabric",
        );
        let ranked = rank(&[a, b]);
        let top = &ranked[0];
        assert_eq!(top.method, "MinecraftServer.tickServer");
        assert_eq!((top.mods, top.starts, top.ends, top.inside), (2, 2, 2, 2));
        let save = ranked
            .iter()
            .find(|t| t.method == "MinecraftServer.saveEverything")
            .unwrap();
        assert_eq!((save.mods, save.inside, save.hooked), (2, 2, false));
        // Accessors, shadows and other mods' classes are not hook point candidates.
        assert!(ranked.iter().all(|t| t.class.starts_with("net/minecraft/")));
        assert!(!ranked.iter().any(|t| t.method.ends_with(".foodData")));
    }

    /// The hook points of `native-hooks.json` itself: `tickServer`'s start and end, so a real
    /// mod's injection there is supported.
    #[test]
    fn the_catalog_has_mixin_hook_points() {
        let points = hook_points();
        assert!(
            find_hook(
                points,
                "net/minecraft/server/MinecraftServer",
                "tickServer",
                "head"
            )
            .is_some()
        );
        assert!(
            find_hook(
                points,
                "net/minecraft/server/MinecraftServer",
                "tickServer",
                "return"
            )
            .is_some()
        );
        assert!(
            points
                .iter()
                .all(|p| p.name.starts_with("mixin_") && (p.at == "head" || p.at == "return"))
        );
    }

    #[test]
    fn reads_versions_and_method_names_as_mods_write_them() {
        assert_eq!(era_of("~1.21.1"), Era::Older("1.21.1".into()));
        assert_eq!(era_of(">=1.21.4 <=26.1"), Era::Older("1.21.4".into()));
        assert_eq!(era_of("[26.1,27)"), Era::V26("26.1".into()));
        assert_eq!(era_of(">=26.2"), Era::V26("26.2".into()));
        assert_eq!(era_of("26.3.x"), Era::V26("26.3.x".into()));
        assert_eq!(era_of("*"), Era::Unknown);
        assert_eq!(era_of(""), Era::Unknown);
        assert_eq!(method_name("tickServer"), "tickServer");
        assert_eq!(
            method_name("tickServer(Ljava/util/function/BooleanSupplier;)V"),
            "tickServer"
        );
        assert_eq!(
            method_name(
                "Lnet/minecraft/server/MinecraftServer;tickServer(Ljava/util/function/BooleanSupplier;)V"
            ),
            "tickServer"
        );
        assert_eq!(method_name("tick*"), "tick");
        assert_eq!(
            accessor_target("getFoodData", &["get", "set", "is"]),
            "foodData"
        );
        assert_eq!(
            accessor_target("invokeDropAll", &["invoke", "call"]),
            "dropAll"
        );
        assert!(obfuscated("net/minecraft/class_3176"));
        assert!(obfuscated("m_6074_"));
        assert!(!obfuscated("net/minecraft/server/MinecraftServer"));
        let points = hook_points_in(
            r#"{"hooks": [
            {"name": "tick", "version": 1, "target": "x"},
            {"name": "mixin_a_b_head", "version": 1, "target": "y", "vanilla": "A.b", "class": "net/minecraft/A", "at": "head"}]}"#,
        );
        assert_eq!(points.len(), 1);
        assert_eq!(
            (points[0].method.as_str(), points[0].at.as_str()),
            ("b", "head")
        );
    }
}
