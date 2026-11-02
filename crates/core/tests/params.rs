use saqviro_core::manifest::ToolManifest;
use serde_json::{Map, Value, json};

fn tool() -> ToolManifest {
    ToolManifest::from_yaml(
        r#"
schema_version: 1
id: image.resize
name: Resize
category: image
tool_version: 1
description: Resize images.
keywords: [scale]
inputs: [{ type: image, mime: [image/png], extensions: [png] }]
outputs: [{ type: image }]
engine: libvips
capabilities: { batch: true, preview: false, offline: true, destructive: false }
permissions: { network: false }
platforms: [macos]
parameters:
  - { id: width, type: integer, min: 1, max: 10000 }
  - { id: height, type: integer, min: 1, optional: true }
  - { id: fit, type: enum, values: [inside, cover], default: inside }
presets:
  - { id: web, name: Web, params: { width: 1920 } }
"#,
    )
    .unwrap()
}

fn params(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn fills_in_defaults() {
    let resolved = tool().resolve_params(&params(json!({ "width": 800 }))).unwrap();
    assert_eq!(Value::Object(resolved), json!({ "width": 800, "fit": "inside" }));
}

#[test]
fn rejects_unknown_parameters() {
    let err = tool()
        .resolve_params(&params(json!({ "width": 800, "depth": 3 })))
        .unwrap_err();
    assert!(err.iter().any(|i| i.field == "depth"), "{err:?}");
}

#[test]
fn rejects_missing_required_parameters() {
    let err = tool().resolve_params(&Map::new()).unwrap_err();
    assert!(err.iter().any(|i| i.field == "width"), "{err:?}");
}

#[test]
fn rejects_values_that_break_parameter_rules() {
    let err = tool()
        .resolve_params(&params(json!({ "width": 0, "fit": "stretch", "height": "tall" })))
        .unwrap_err();
    let fields: Vec<_> = err.iter().map(|i| i.field.as_str()).collect();
    assert_eq!(fields, ["fit", "height", "width"]);
}

#[test]
fn null_clears_an_optional_parameter() {
    let resolved = tool()
        .resolve_params(&params(json!({ "width": 5, "height": null })))
        .unwrap();
    assert!(!resolved.contains_key("height"));
}
