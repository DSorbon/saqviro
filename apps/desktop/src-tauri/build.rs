//! Embeds every `tools/**/tool.yaml` into the binary, so the app never reads
//! manifests from disk at runtime.

use std::{
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("tools directory is readable") {
        let path = entry.expect("tools entry is readable").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.file_name().is_some_and(|n| n == "tool.yaml") {
            out.push(path);
        }
    }
}

fn main() {
    let tools = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tools")
        .canonicalize()
        .expect("tools directory exists");
    println!("cargo:rerun-if-changed={}", tools.display());

    let mut files = Vec::new();
    collect(&tools, &mut files);
    files.sort();

    let mut src = String::from("pub static MANIFESTS: &[(&str, &str)] = &[\n");
    for file in &files {
        let rel = file.strip_prefix(&tools).expect("manifest is inside tools").display();
        writeln!(
            src,
            "    ({:?}, include_str!({:?})),",
            rel.to_string(),
            file.display().to_string()
        )
        .expect("write to string");
    }
    src.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").expect("OUT_DIR is set")).join("manifests.rs"),
        src,
    )
    .expect("write manifests.rs");

    tauri_build::build();
}
