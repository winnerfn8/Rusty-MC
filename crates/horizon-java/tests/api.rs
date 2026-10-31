//! `api.txt` lists the Java API layer: its Fabric API modules, and each class with its superclass,
//! interfaces and public members (name and descriptor), read from the layer's compiled classes.
//! horizon-port-pack reads it for the verdicts: what a mod's code uses of vanilla's, Fabric's or
//! Mixin's API that the layer doesn't have yet. It must match the classes;
//! `HORIZON_BLESS=1 cargo test -p horizon-java --features jvm --test api` rewrites it.
#![cfg(feature = "jvm")]

use horizon_java::{classfile, fabric, jvm};
use horizon_port_pack::classes;
use std::path::Path;

const PUBLIC: u16 = 0x0001;
const PROTECTED: u16 = 0x0004;
const SYNTHETIC: u16 = 0x1000;

fn listing() -> String {
    let mut out = String::from(
        "# The optional Java layer's API (crates/horizon-java/java/), generated from its classes:\n\
         # HORIZON_BLESS=1 cargo test -p horizon-java --features jvm --test api\n\
         # `module` lines: Fabric API's modules with a Rust version. `class` lines: name, superclass,\n\
         # interfaces; then its public and protected members, name and descriptor.\n",
    );
    for m in fabric::MODULES {
        out.push_str(&format!("module {m}\n"));
    }
    let mut all: Vec<(&str, &[u8])> = jvm::api_classes().collect();
    all.sort_by_key(|(n, _)| *n);
    for (name, bytes) in all {
        let (_, sup, interfaces) = classfile::names(bytes).expect(name);
        let mut line = format!("class {name} {}", sup.unwrap_or_default());
        for i in interfaces {
            line.push(' ');
            line.push_str(&i);
        }
        out.push_str(line.trim_end());
        out.push('\n');
        let class = classes::parse_annotated(bytes).expect(name);
        let mut members: Vec<(String, String)> = class
            .fields
            .iter()
            .chain(&class.methods)
            .filter(|m| m.access & (PUBLIC | PROTECTED) != 0 && m.access & SYNTHETIC == 0)
            .filter(|m| m.name != "<clinit>" && !m.name.starts_with("rm$"))
            .map(|m| (m.name.clone(), m.descriptor.clone()))
            .collect();
        members.sort();
        members.dedup();
        for (n, d) in members {
            out.push_str(&format!("  {n} {d}\n"));
        }
    }
    out
}

#[test]
fn api_txt_lists_the_layer() {
    let want = listing();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("api.txt");
    if std::env::var_os("HORIZON_BLESS").is_some() {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        have == want,
        "api.txt is out of date: HORIZON_BLESS=1 cargo test -p horizon-java --features jvm --test api"
    );
}
