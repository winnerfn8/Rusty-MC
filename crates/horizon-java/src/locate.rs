//! Finding the player's own Java, which is never bundled: a JDK or JRE folder, its `java`
//! launcher, or its JVM library, chosen in Settings or found on this computer.
use std::path::{Path, PathBuf};

/// The JVM library's place in a Java home, by platform (Java 9 and later; `jre/` for 8).
const LIBJVM: &[&str] = if cfg!(target_os = "windows") {
    &[
        "bin/server/jvm.dll",
        "bin/client/jvm.dll",
        "jre/bin/server/jvm.dll",
    ]
} else if cfg!(target_vendor = "apple") {
    &["lib/server/libjvm.dylib", "jre/lib/server/libjvm.dylib"]
} else {
    &[
        "lib/server/libjvm.so",
        "lib/client/libjvm.so",
        "jre/lib/amd64/server/libjvm.so",
    ]
};

/// The JVM library of the Java home `home`.
pub fn libjvm_in(home: &Path) -> Option<PathBuf> {
    LIBJVM.iter().map(|p| home.join(p)).find(|p| p.is_file())
}

/// The Java home a chosen path means: the home itself, its `bin/java`, or its JVM library.
/// macOS bundles (`…/jdk.jdk`) are entered through `Contents/Home`.
pub fn home_of(path: &Path) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    let mut candidates = vec![path.clone(), path.join("Contents/Home")];
    candidates.extend(path.ancestors().skip(1).take(4).map(Path::to_path_buf));
    candidates.into_iter().find(|h| libjvm_in(h).is_some())
}

/// The Java version a home says it is (`release`'s `JAVA_VERSION="21.0.12"`: 21).
pub fn version_of(home: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    let line = text.lines().find(|l| l.starts_with("JAVA_VERSION="))?;
    let v = line.trim_start_matches("JAVA_VERSION=").trim_matches('"');
    let mut parts = v.split(['.', '_', '+', '-']);
    match parts.next()?.parse::<u32>().ok()? {
        1 => parts.next()?.parse().ok(),
        major => Some(major),
    }
}

/// The Java homes on this computer, newest version first: `JAVA_HOME`, the `java` on `PATH`,
/// then the usual install folders.
pub fn installed() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut add = |p: &Path| {
        if let Some(h) = home_of(p)
            && !found.contains(&h)
        {
            found.push(h);
        }
    };
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        add(Path::new(&home));
    }
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let java = dir.join(exe);
        if java.is_file() {
            add(&java);
        }
    }
    let roots: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:/Program Files/Java",
            "C:/Program Files/Eclipse Adoptium",
            "C:/Program Files/Microsoft",
            "C:/Program Files/Zulu",
        ]
    } else if cfg!(target_vendor = "apple") {
        &["/Library/Java/JavaVirtualMachines", "/opt/homebrew/opt"]
    } else {
        &["/usr/lib/jvm", "/usr/java", "/opt/java", "/opt/jdk"]
    };
    for root in roots {
        let Ok(dir) = std::fs::read_dir(root) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = dir.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for e in entries {
            add(&e);
        }
    }
    found.sort_by_key(|h| std::cmp::Reverse(version_of(h).unwrap_or(0)));
    found
}

/// The JVM library to run: the player's choice (`choice`, as in Settings), else the newest
/// Java found.
pub fn libjvm(choice: &str) -> Result<PathBuf, String> {
    let home = if choice.trim().is_empty() {
        installed()
            .into_iter()
            .next()
            .ok_or("no Java found on this computer: install Java 25 (what Minecraft 26.x runs on) or choose one in Settings")?
    } else {
        home_of(Path::new(choice.trim()))
            .ok_or_else(|| format!("{choice}: not a Java folder, `java` or JVM library"))?
    };
    libjvm_in(&home).ok_or_else(|| format!("{}: no JVM library in it", home.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_is_found_from_itself_its_java_and_its_library() {
        let root = std::env::temp_dir().join(format!("horizon-java-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("jdk-25");
        let lib = home.join(LIBJVM[0]);
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(&lib, b"").unwrap();
        std::fs::write(home.join("bin/java"), b"").unwrap();
        std::fs::write(
            home.join("release"),
            "IMPLEMENTOR=\"x\"\nJAVA_VERSION=\"25.0.1\"\n",
        )
        .unwrap();
        let home = home.canonicalize().unwrap();
        assert_eq!(home_of(&home), Some(home.clone()));
        assert_eq!(home_of(&home.join("bin/java")), Some(home.clone()));
        assert_eq!(home_of(&lib), Some(home.clone()));
        assert_eq!(version_of(&home), Some(25));
        assert_eq!(libjvm(home.to_str().unwrap()), Ok(home.join(LIBJVM[0])));
        assert!(libjvm(root.join("nothing").to_str().unwrap()).is_err());
        std::fs::write(home.join("release"), "JAVA_VERSION=\"1.8.0_402\"\n").unwrap();
        assert_eq!(version_of(&home), Some(8));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
