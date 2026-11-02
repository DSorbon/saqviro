//! `packages/tool-schema/tool.schema.json` is generated from the Rust types.
//! Regenerate with `UPDATE_SCHEMA=1 cargo test -p saqviro-core --test schema`.

use std::{fs, path::Path};

use saqviro_core::manifest::ToolManifest;

#[test]
fn committed_schema_matches_rust_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/tool-schema/tool.schema.json");
    let generated = serde_json::to_string_pretty(&schemars::schema_for!(ToolManifest)).unwrap() + "\n";
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        fs::write(&path, &generated).unwrap();
    }
    let committed = fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == generated,
        "tool.schema.json is out of date; run `UPDATE_SCHEMA=1 cargo test -p saqviro-core --test schema`"
    );
}
