//! `horizon-port-pack`: port a Java modpack to Horizon (`docs/modpacks.md`).
//!
//! - `import <pack> <work>`: a `.mrpack`, CurseForge zip or instance folder
//!   to `<work>/pack.json`.
//! - `inventory <work> [--offline]`: Modrinth metadata (cached in
//!   `<work>/modrinth.json`), the jars in the instance or `<work>/jars/`,
//!   classes, tiers and dependency layers in `<work>/inventory.json` and
//!   `inventory.csv`.
//! - `fetch <work> [--from <folder>] [<mod id>...]`: the jars of the
//!   portable mods (content, data tier; or the ones named) into
//!   `<work>/jars/`: copied from the player's folder, or downloaded from the
//!   pack's own links or Modrinth's CDN, hash checked. What only CurseForge
//!   has needs a key and is listed in `<work>/fetch.json`.
//! - `carry <work> [--resources <prepared>] [<mod id>...]`: the content
//!   mods' data and assets into `<work>/mods/<id>/carry/`, a folder mod for
//!   local use. With the NeoForge oracle's `<work>/oracle/facts.json`
//!   (`tools/neoforge_oracle.py`), blocks and items come from its facts;
//!   `--resources` limits templates to the blocks Horizon's tables have.
//! - `report <work>`: the coverage report, per mod ported, partial, not yet
//!   or not needed with the reason, in `<work>/coverage.json` and
//!   `coverage.md`.
//! - `check <jar> <out>`: one mod jar on its own, as the launcher's mod
//!   browser installs it: carried over into `<out>/carry/`, its classes
//!   read, and the verdict (Works, Partly, Doesn't run) in
//!   `<out>/check.json`.
//! - `port <pack> <work> [--from <folder>] [--resources <prepared>]`: every
//!   step for the portable mods, as the launcher's "Import modpack" runs it.
//! - `translate <game folder>`: the Java mod translation layer
//!   (`docs/java-mod-translation.md`): the Fabric, Quilt, NeoForge and Forge
//!   jars in the game folder's `mods/` and `horizon-mods/` translated into
//!   content mods in `horizon-mods/`, as the game does when it starts.
//! - `mixin-targets <folder>`: the vanilla methods the mixins of the jars
//!   in a folder inject into, most-targeted first, each with whether it maps
//!   to one Rust function (`data/vanilla-one-to-one.tsv`, made by
//!   `tools/mixin_hook_table.py`) and has a hook point: which methods to give
//!   hook points next (`docs/java-mod-runtime.md`).
//! - `upgrade-datapack <pack> <out> | --world <world>`: a 26.2 data pack
//!   (zip or folder) with its world generation rewritten in 26.3's formats
//!   (`docs/datapack-upgrade.md`), into the folder `<out>`, or into
//!   `<world>/datapacks/<pack name>-26.3`.
//!
//! The work folder belongs to the user: jars and carried-over files stay
//! there and are never uploaded or committed.
use horizon_port_pack::{fetch, inventory::Record, pipeline, translate, upgrade};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "usage: horizon-port-pack import <pack> <work> | inventory <work> [--offline] | fetch <work> [--from <folder>] [<mod id>...] | carry <work> [--resources <prepared>] [<mod id>...] | report <work> | check <jar> <out> | port <pack> <work> [--from <folder>] [--resources <prepared>] | translate <game folder> | mixin-targets <folder> | upgrade-datapack <pack> (<out> | --world <world>)";

/// Vanilla methods that map to exactly one Rust function (`tools/mixin_hook_table.py`).
const ONE_TO_ONE: &str = include_str!("../data/vanilla-one-to-one.tsv");

/// `mixin-targets <folder>`: what the jars' mixins inject into, most-targeted first.
fn mixin_targets(folder: &Path) -> Result<(), String> {
    use horizon_port_pack::{check, jar, mixins};
    let mut reports = Vec::new();
    let mut jars: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| format!("{}: {e}", folder.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jar")))
        .collect();
    jars.sort();
    for path in &jars {
        let read = || -> Result<mixins::Report, String> {
            let info = jar::read(path)?;
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
            Ok(mixins::read(&mut zip, &info, check::loader(&names)))
        };
        match read() {
            Ok(r) => reports.push(r),
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
    let one: std::collections::HashMap<&str, &str> = ONE_TO_ONE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t');
            Some((cols.next()?, cols.next()?))
        })
        .collect();
    let era = |r: &mixins::Report| matches!(r.era, mixins::Era::V26(_));
    println!(
        "{} jars, {} made for 26.x; vanilla methods their mixins inject into, most mods first:",
        reports.len(),
        reports.iter().filter(|r| era(r)).count()
    );
    let v26: Vec<mixins::Report> = reports.into_iter().filter(era).collect();
    for t in mixins::rank(&v26).iter().take(60) {
        let rust = one.get(t.method.as_str()).map_or_else(
            || "no single Rust function".to_owned(),
            |f| format!("one Rust function: {f}"),
        );
        let hooked = if t.hooked { ", has a hook point" } else { "" };
        println!(
            "{:>3} mods  {}  (start {}, end {}, inside {}) {rust}{hooked}",
            t.mods, t.method, t.starts, t.ends, t.inside
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let result = (|| {
        let from = option(&mut args, "--from")?;
        let resources = option(&mut args, "--resources")?;
        let world = option(&mut args, "--world")?;
        let offline = take_flag(&mut args, "--offline");
        let path = |i: usize| args.get(i).map(Path::new).ok_or(USAGE.to_owned());
        let rest = || args.get(2..).unwrap_or_default().to_vec();
        let log = &|line: String| println!("{line}");
        match args.first().map(String::as_str) {
            Some("import") => pipeline::import(path(1)?, path(2)?, log).map(drop),
            Some("inventory") => pipeline::inventory(path(1)?, offline, log).map(drop),
            Some("fetch") => {
                let work = path(1)?;
                let records: Vec<Record> = pipeline::read_json(&work.join("inventory.json"))?;
                let only = rest();
                let want = |r: &Record| {
                    if only.is_empty() {
                        pipeline::portable(r)
                    } else {
                        only.contains(&r.key)
                    }
                };
                fetch::fetch(work, &records, from.as_deref(), &want, log).map(drop)
            }
            Some("carry") => {
                pipeline::carry(path(1)?, resources.as_deref(), &rest(), log).map(drop)
            }
            Some("report") => {
                let work = path(1)?;
                pipeline::report(work, log)?;
                println!("{}", work.join("coverage.md").display());
                Ok(())
            }
            Some("check") => {
                let c = horizon_port_pack::check::check(path(1)?, path(2)?)?;
                println!("{}: {}. {}", c.mod_id, c.verdict.label(), c.summary);
                for p in &c.ports {
                    println!("  ports: {p}");
                }
                for s in &c.skipped {
                    println!("  skipped: {s}");
                }
                let code = c.code_line();
                if !code.is_empty() {
                    println!("  code: {code}");
                }
                for line in c.mixins.lines() {
                    println!("    {}", line.text);
                }
                Ok(())
            }
            Some("port") => {
                let work = path(2)?;
                pipeline::import(path(1)?, work, log)?;
                pipeline::inventory(work, offline, log)?;
                pipeline::port(work, resources.as_deref(), from.as_deref(), log)?;
                println!("{}", work.join("coverage.md").display());
                Ok(())
            }
            Some("translate") => {
                let home = path(1)?;
                translate::sync(home, log)?;
                for e in translate::entries(home) {
                    let state = match e.state {
                        translate::State::Installed if translate::is_enabled(home, &e.mod_id) => {
                            "on"
                        }
                        translate::State::Installed => "off",
                        translate::State::NotInstalled => "not installed",
                        translate::State::Failed => "failed",
                    };
                    let verdict = e.verdict.map_or("", |v| v.label());
                    println!("{} [{verdict}, {state}] {}", e.jar, e.summary);
                    for n in &e.notes {
                        println!("  note: {n}");
                    }
                    if !e.code.is_empty() {
                        println!("  code: {}", e.code);
                    }
                    for line in &e.mixins {
                        println!("    {}", line.text);
                    }
                }
                Ok(())
            }
            Some("mixin-targets") => mixin_targets(path(1)?),
            Some("upgrade-datapack") => {
                let pack = path(1)?;
                let out = match &world {
                    Some(world) => {
                        let name = pack.file_stem().unwrap_or_default().to_string_lossy();
                        world.join("datapacks").join(format!("{name}-26.3"))
                    }
                    None => path(2)?.to_path_buf(),
                };
                let report = upgrade::upgrade_into(pack, &out)?;
                for line in report.lines() {
                    println!("{line}");
                }
                println!("{}", out.display());
                Ok(())
            }
            _ => Err(USAGE.to_owned()),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Removes `--name <value>` from `args`.
fn option(args: &mut Vec<String>, name: &str) -> Result<Option<PathBuf>, String> {
    match args.iter().position(|a| a == name) {
        Some(i) if i + 1 < args.len() => {
            let value = args.remove(i + 1);
            args.remove(i);
            Ok(Some(PathBuf::from(value)))
        }
        Some(_) => Err(USAGE.to_owned()),
        None => Ok(None),
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != name);
    args.len() != before
}
