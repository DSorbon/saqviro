//! `two-tone.heic` was made with macOS `sips` from a 40×20 image whose left
//! half is red and right half is blue.
#![cfg(feature = "heic")]

mod common;

use std::path::Path;

use image::GenericImageView;
use saqviro_tools::images::{Format, decode};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-tone.heic")
}

#[test]
fn decodes_heic() {
    let pic = decode(&fixture()).unwrap();
    assert_eq!(pic.format, Format::Heic);
    assert_eq!(pic.image.dimensions(), (40, 20));
    let left = pic.image.get_pixel(5, 10);
    let right = pic.image.get_pixel(35, 10);
    assert!(left[0] > 150 && left[2] < 100, "left should be red: {left:?}");
    assert!(right[2] > 150 && right[0] < 100, "right should be blue: {right:?}");
}

#[test]
fn damaged_heic_is_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture()).unwrap();
    let path = common::write(dir.path(), "cut.heic", &bytes[..bytes.len() / 2]);
    let err = decode(&path).unwrap_err();
    assert_eq!(err.kind, saqviro_jobs::ErrorKind::InvalidInput, "{}", err.message);
}

#[test]
fn heic_to_jpg_is_available() {
    let yaml =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/image/heic-to-jpg/tool.yaml"))
            .unwrap();
    let tool = saqviro_core::manifest::ToolManifest::from_yaml(&yaml).unwrap();
    assert!(saqviro_tools::executor(&tool).is_some());
    assert!(tool.accepts_extension("HEIC"));
}
