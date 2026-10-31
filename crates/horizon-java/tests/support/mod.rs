//! Builds the test mod (`tests/tickcounter/`) into a jar with the JDK's `javac`, and turns its
//! supported injections into bindings the way the game does (horizon-port-pack's reading).
#![allow(dead_code)]
use horizon_java::{
    fabric::Entrypoint,
    hooks::{Hook, Side},
    jvm::Binding,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            java_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}

fn javac(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = Command::new("javac")
        .env_remove("JAVA_TOOL_OPTIONS")
        .args(args)
        .output()
        .map_err(|e| format!("javac: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(())
}

/// The tick counter's jar, built in `dir`. Err when there is no JDK.
pub fn tickcounter(dir: &Path) -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mod_dir = root.join("tests/tickcounter");
    let classes = dir.join("classes");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(&classes).unwrap();
    // It compiles against the stubs and the layer's own classes, found by source and not
    // written out (-implicit:none): only the mod's classes go in the jar.
    let sourcepath = std::env::join_paths([mod_dir.join("stubs"), root.join("java")]).unwrap();
    let mut sources = Vec::new();
    java_files(&mod_dir.join("src"), &mut sources);
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "--release".as_ref(),
        "21".as_ref(),
        "-implicit:none".as_ref(),
        "-sourcepath".as_ref(),
        sourcepath.as_os_str(),
        "-d".as_ref(),
        classes.as_os_str(),
    ];
    args.extend(sources.iter().map(|p| p.as_os_str()));
    javac(&args)?;
    let jar = dir.join("tickcounter-1.0.0.jar");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for name in ["fabric.mod.json", "tickcounter.mixins.json"] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(&std::fs::read(mod_dir.join(name)).unwrap())
            .unwrap();
    }
    let mut class_files = Vec::new();
    collect(&classes, &classes, &mut class_files);
    for (name, path) in class_files {
        zip.start_file(name.as_str(), opts).unwrap();
        zip.write_all(&std::fs::read(path).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    Ok(jar)
}

/// The class files under `dir`, by their path in a jar.
fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, root, out);
        } else if p.extension().is_some_and(|x| x == "class") {
            let name = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((name, p));
        }
    }
}

/// The jar's entry points, read as the game reads them.
pub fn entrypoints(jar: &Path) -> Vec<Entrypoint> {
    let info = horizon_port_pack::jar::read(jar).unwrap();
    info.entrypoints
        .iter()
        .map(|(kind, spec)| Entrypoint {
            jar: jar.to_path_buf(),
            mod_id: info.mod_id.clone(),
            kind: kind.clone(),
            spec: spec.clone(),
        })
        .collect()
}

/// The jar's supported injections as bindings, read as the game reads them.
pub fn bindings(jar: &Path) -> Vec<Binding> {
    use horizon_port_pack::{check, jar as meta, mixins};
    let info = meta::read(jar).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).unwrap()).unwrap();
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let report = mixins::read(&mut zip, &info, check::loader(&names));
    report
        .injections
        .iter()
        .filter(|i| i.group == mixins::Group::Supported)
        .filter_map(|i| {
            Some(Binding {
                hook: Hook::from_name(&i.hook)?,
                jar: jar.to_path_buf(),
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
            })
        })
        .collect()
}
