//! With the `jvm` feature, compiles the Java layer's own classes (`java/`) with the JDK's `javac`
//! into `OUT_DIR`, for `jvm.rs` to define in the JVM. Building needs a JDK; the player's game only
//! needs a Java runtime.
use std::{env, fs, path::Path, path::PathBuf, process::Command};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "java") {
            out.push(p);
        }
    }
}

fn classes(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            classes(&p, root, out);
        } else if let Some(name) = p.strip_prefix(root).ok().and_then(|r| r.to_str()) {
            out.push((name.trim_end_matches(".class").replace('\\', "/"), p));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=java");
    println!("cargo:rerun-if-env-changed=JAVA_HOME");
    if env::var_os("CARGO_FEATURE_JVM").is_none() {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("classes");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap();
    let mut files = Vec::new();
    sources(Path::new("java"), &mut files);
    let javac = env::var_os("JAVA_HOME")
        .map(|h| PathBuf::from(h).join("bin").join("javac"))
        .filter(|j| j.is_file() || j.with_extension("exe").is_file())
        .unwrap_or_else(|| PathBuf::from("javac"));
    // Java 21 bytecode: the layer's own classes load on any Java from 21 on.
    let status = Command::new(&javac)
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&out)
        .args(&files)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "the jvm feature needs a JDK's javac to build ({}: {e})",
                javac.display()
            )
        });
    assert!(status.success(), "javac failed on crates/horizon-java/java");
    let mut found = Vec::new();
    classes(&out, &out, &mut found);
    // jvm.rs defines them superclasses first (it reads each class file).
    found.sort();
    let list: String = found
        .iter()
        .map(|(name, path)| {
            format!(
                "    ({name:?}, include_bytes!({:?})),\n",
                path.display().to_string()
            )
        })
        .collect();
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("classes.rs"),
        format!("/// The Java layer's own classes, by internal name.\npub const CLASSES: &[(&str, &[u8])] = &[\n{list}];\n"),
    )
    .unwrap();
}
