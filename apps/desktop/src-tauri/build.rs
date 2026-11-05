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

    // Tauri puts its manifest in the app only, and test binaries then fail to
    // start on Windows. Linking our own covers both.
    let mut attributes = tauri_build::Attributes::new();
    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        attributes = attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("tauri build step failed");
}
