//! Manifests and implementations must stay in sync.

use std::{fs, path::Path};

use saqviro_core::registry::Registry;

fn manifests() -> Registry {
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
    let mut files = Vec::new();
    collect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools"), &mut files);
    Registry::from_sources(files.iter().map(|(p, s)| (p.as_str(), s.as_str()))).unwrap()
}

#[test]
fn every_builtin_tool_has_an_executor() {
    let gated = |id: &str| saqviro_tools::NEEDS_FEATURE.iter().any(|(t, _)| *t == id);
    let builtin = manifests();
    let builtin = builtin.tools().iter().filter(|t| t.engine == "builtin");
    for tool in builtin.filter(|t| cfg!(feature = "heic") || !gated(t.id.as_str())) {
        assert!(saqviro_tools::executor(tool).is_some(), "{} has no executor", tool.id);
    }
}

#[test]
fn every_executor_has_a_manifest() {
    let registry = manifests();
    for id in saqviro_tools::IMPLEMENTED {
        assert!(registry.get(id).is_some(), "{id} has no manifest");
    }
}

/// The list of tools in the documentation. Regenerate with
/// `UPDATE_DOCS=1 cargo test -p saqviro-tools --test catalog`.
#[test]
fn the_tool_list_in_the_docs_is_current() {
    let registry = manifests();
    let mut out = format!(
        "# Tools\n\n{} tools. This file is generated from the manifests in `tools/`.\n",
        registry.tools().len()
    );
    let mut category = "";
    for tool in registry.tools() {
        if tool.category.as_str() != category {
            category = tool.category.as_str();
            let title = match category {
                "pdf" => "PDF",
                "qr" => "QR & barcodes",
                other => &format!("{}{}", other[..1].to_uppercase(), &other[1..]),
            };
            out.push_str(&format!("\n## {title}\n\n| Tool | What it does |\n|---|---|\n"));
        }
        out.push_str(&format!("| {} | {} |\n", tool.name, tool.description));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/tools.md");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        fs::write(&path, &out).unwrap();
    }
    assert!(
        fs::read_to_string(&path).unwrap_or_default() == out,
        "docs/tools.md is out of date"
    );
}
