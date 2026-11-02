//! Every manifest committed under `tools/` must load.

use std::{fs, path::Path};

use saqviro_core::registry::Registry;

fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.file_name().is_some_and(|n| n == "tool.yaml") {
            out.push((path.display().to_string(), fs::read_to_string(&path).unwrap()));
        }
    }
}

#[test]
fn all_repository_manifests_are_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools");
    let mut files = Vec::new();
    collect(&root, &mut files);
    assert!(!files.is_empty(), "no manifests found in {}", root.display());
    let reg =
        Registry::from_sources(files.iter().map(|(p, s)| (p.as_str(), s.as_str()))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(reg.tools().len(), files.len());
}
