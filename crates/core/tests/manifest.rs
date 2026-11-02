use saqviro_core::manifest::{Category, DataType, ToolManifest};

const VALID: &str = r#"
schema_version: 1
id: image.resize
name: Resize Image
category: image
tool_version: 1
description: Resize one or many images.
keywords: [scale, shrink]
inputs:
  - type: image
    multiple: true
    mime: [image/jpeg, image/png]
    extensions: [jpg, jpeg, png]
outputs:
  - type: image
    filename: "{stem}-{width}x{height}.{ext}"
engine: libvips
capabilities: { batch: true, preview: true, offline: true, destructive: false }
permissions: { network: false }
platforms: [windows, macos, linux]
parameters:
  - { id: width, type: integer, min: 1 }
  - { id: height, type: integer, min: 1 }
  - { id: fit, type: enum, values: [contain, cover] }
presets:
  - { id: web, name: Web, params: { width: 1920, fit: contain } }
ui:
  advanced: [fit]
"#;

fn parse(src: &str) -> Result<ToolManifest, String> {
    ToolManifest::from_yaml(src).map_err(|e| e.to_string())
}

fn with(replace: &str, by: &str) -> String {
    assert!(VALID.contains(replace), "fixture does not contain {replace:?}");
    VALID.replacen(replace, by, 1)
}

#[test]
fn parses_valid_manifest() {
    let m = parse(VALID).unwrap();
    assert_eq!(m.id.as_str(), "image.resize");
    assert_eq!(m.category, Category::Image);
    assert_eq!(m.inputs[0].data_type, DataType::Image);
    assert_eq!(m.presets.len(), 1);
    assert_eq!(m.default_preset().id, "web");
}

#[test]
fn rejects_unknown_field() {
    let err = parse(&with("engine: libvips", "engine: libvips\ncolour: red")).unwrap_err();
    assert!(err.contains("colour"), "{err}");
}

#[test]
fn rejects_missing_required_field() {
    let err = parse(&with("keywords: [scale, shrink]\n", "")).unwrap_err();
    assert!(err.contains("keywords"), "{err}");
}

#[test]
fn rejects_unsupported_schema_version() {
    let err = parse(&with("schema_version: 1", "schema_version: 2")).unwrap_err();
    assert!(err.contains("schema_version"), "{err}");
}

#[test]
fn rejects_malformed_tool_ids() {
    for bad in [
        "image",
        "Image.resize",
        "image..resize",
        "image.resize.a.b",
        "image.re size",
        "image.1resize",
    ] {
        let err = parse(&with("id: image.resize", &format!("id: \"{bad}\""))).unwrap_err();
        assert!(err.contains("id"), "{bad}: {err}");
    }
}

#[test]
fn accepts_three_segment_id() {
    let m = parse(&with("id: image.resize", "id: image.metadata.remove")).unwrap();
    assert_eq!(m.id.as_str(), "image.metadata.remove");
}

#[test]
fn rejects_id_outside_its_category() {
    let err = parse(&with("id: image.resize", "id: pdf.resize")).unwrap_err();
    assert!(err.contains("category"), "{err}");
}

#[test]
fn requires_at_least_one_preset() {
    let err = parse(
        &with(
            "  - { id: web, name: Web, params: { width: 1920, fit: contain } }\n",
            "  []\n",
        )
        .replace("presets:\n  []", "presets: []"),
    )
    .unwrap_err();
    assert!(err.contains("presets"), "{err}");
}

#[test]
fn rejects_more_than_four_presets() {
    let p = "  - { id: web, name: Web, params: { width: 1920, fit: contain } }\n";
    let many = (0..5)
        .map(|i| p.replace("id: web", &format!("id: p{i}")))
        .collect::<String>();
    let err = parse(&with(p, &many)).unwrap_err();
    assert!(err.contains("presets"), "{err}");
}

#[test]
fn rejects_preset_with_unknown_parameter() {
    let err = parse(&with("width: 1920, fit", "depth: 1920, fit")).unwrap_err();
    assert!(err.contains("depth"), "{err}");
}

#[test]
fn rejects_preset_value_outside_parameter_rules() {
    let err = parse(&with("width: 1920", "width: 0")).unwrap_err();
    assert!(err.contains("width"), "{err}");
    let err = parse(&with("fit: contain }", "fit: stretch }")).unwrap_err();
    assert!(err.contains("fit"), "{err}");
}

#[test]
fn rejects_duplicate_parameter_ids() {
    let err = parse(&with("{ id: height, type: integer", "{ id: width, type: integer")).unwrap_err();
    assert!(err.contains("width"), "{err}");
}

#[test]
fn rejects_advanced_reference_to_unknown_parameter() {
    let err = parse(&with("advanced: [fit]", "advanced: [quality]")).unwrap_err();
    assert!(err.contains("quality"), "{err}");
}

#[test]
fn file_inputs_need_mime_and_extensions() {
    let err = parse(&with("    extensions: [jpg, jpeg, png]\n", "")).unwrap_err();
    assert!(err.contains("extensions"), "{err}");
}

#[test]
fn text_inputs_need_no_file_types() {
    let src = with(
        "  - type: image\n    multiple: true\n    mime: [image/jpeg, image/png]\n    extensions: [jpg, jpeg, png]\n",
        "  - type: text\n",
    );
    parse(&src).unwrap();
}

#[test]
fn rejects_empty_platforms_and_keywords() {
    assert!(
        parse(&with("platforms: [windows, macos, linux]", "platforms: []"))
            .unwrap_err()
            .contains("platforms")
    );
    assert!(
        parse(&with("keywords: [scale, shrink]", "keywords: []"))
            .unwrap_err()
            .contains("keywords")
    );
}

#[test]
fn reports_every_problem_at_once() {
    let src = with("keywords: [scale, shrink]", "keywords: []")
        .replace("platforms: [windows, macos, linux]", "platforms: []");
    let err = parse(&src).unwrap_err();
    assert!(err.contains("keywords") && err.contains("platforms"), "{err}");
}

#[test]
fn accepts_extensions_case_insensitively() {
    let m = parse(VALID).unwrap();
    assert!(m.accepts_extension("JPG"));
    assert!(!m.accepts_extension("pdf"));
}
