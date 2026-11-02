use saqviro_core::registry::Registry;

fn manifest(id: &str, category: &str, exts: &str) -> String {
    format!(
        r#"
schema_version: 1
id: {id}
name: {id}
category: {category}
tool_version: 1
description: test
keywords: [test]
inputs:
  - {{ type: any, mime: [application/octet-stream], extensions: [{exts}] }}
outputs:
  - {{ type: any }}
engine: builtin
capabilities: {{ batch: false, preview: false, offline: true, destructive: false }}
permissions: {{ network: false }}
platforms: [windows, macos, linux]
presets:
  - {{ id: default, name: Default }}
"#
    )
}

#[test]
fn loads_tools_sorted_by_id() {
    let a = manifest("pdf.merge", "pdf", "pdf");
    let b = manifest("image.resize", "image", "png");
    let reg = Registry::from_sources([
        ("pdf/merge/tool.yaml", a.as_str()),
        ("image/resize/tool.yaml", b.as_str()),
    ])
    .unwrap();
    let ids: Vec<_> = reg.tools().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["image.resize", "pdf.merge"]);
    assert!(reg.get("pdf.merge").is_some());
    assert!(reg.get("pdf.split").is_none());
}

#[test]
fn rejects_duplicate_ids_naming_both_files() {
    let a = manifest("pdf.merge", "pdf", "pdf");
    let err = Registry::from_sources([("a/tool.yaml", a.as_str()), ("b/tool.yaml", a.as_str())])
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("pdf.merge") && err.contains("a/tool.yaml") && err.contains("b/tool.yaml"),
        "{err}"
    );
}

#[test]
fn collects_errors_from_all_files() {
    let err = Registry::from_sources([("x/tool.yaml", "id: ["), ("y/tool.yaml", "schema_version: 1")])
        .unwrap_err()
        .to_string();
    assert!(err.contains("x/tool.yaml") && err.contains("y/tool.yaml"), "{err}");
}

#[test]
fn finds_tools_for_extension() {
    let a = manifest("pdf.merge", "pdf", "pdf");
    let b = manifest("image.resize", "image", "png");
    let reg = Registry::from_sources([("a", a.as_str()), ("b", b.as_str())]).unwrap();
    let ids: Vec<_> = reg.for_extension("PDF").map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["pdf.merge"]);
}

#[test]
fn suggests_only_tools_accepting_every_dropped_file() {
    let pdf = manifest("pdf.merge", "pdf", "pdf");
    let img = manifest("image.resize", "image", "png, jpg");
    let any = manifest("files.zip", "files", "pdf, png, jpg");
    let reg = Registry::from_sources([("a", pdf.as_str()), ("b", img.as_str()), ("c", any.as_str())]).unwrap();
    let ids = |files: &[&str]| {
        reg.for_files(files)
            .iter()
            .map(|t| t.id.to_string())
            .collect::<Vec<_>>()
    };

    assert_eq!(ids(&["/tmp/a.PNG", "/tmp/b.jpg"]), ["files.zip", "image.resize"]);
    assert_eq!(ids(&["/tmp/a.png", "/tmp/doc.pdf"]), ["files.zip"]);
    assert_eq!(ids(&["/tmp/Makefile"]), Vec::<String>::new());
    assert_eq!(ids(&[]), Vec::<String>::new());
}
