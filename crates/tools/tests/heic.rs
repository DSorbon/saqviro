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

#[test]
fn metadata_removal_turns_heic_into_a_clean_jpg() {
    let yaml = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/image/metadata-remove/tool.yaml"),
    )
    .unwrap();
    let tool = saqviro_core::manifest::ToolManifest::from_yaml(&yaml).unwrap();
    assert!(tool.accepts_extension("heic"));

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("photo.heic");
    std::fs::copy(fixture(), &source).unwrap();
    let config = saqviro_jobs::Config {
        temp_root: dir.path().join("jobs"),
        workers: 1,
    };
    let jobs = saqviro_jobs::JobManager::new(config, |_: &saqviro_jobs::JobSnapshot| {}).unwrap();
    let exec = saqviro_tools::executor(&tool).unwrap();
    let input = saqviro_jobs::JobInput::Files(vec![source]);
    let id = jobs.submit(tool.id.as_str(), exec, input, serde_json::Map::new());
    let done = jobs.wait(id).unwrap();
    let Some(saqviro_jobs::JobOutput::Files(files)) = done.output else {
        panic!("{:?}", done.error);
    };
    assert_eq!(files[0].path.file_name().unwrap(), "photo-clean.jpg");
    assert!(files[0].note.is_some());
    let bytes = std::fs::read(&files[0].path).unwrap();
    assert!(!saqviro_tools::advice::has_location(&files[0].path));
    assert_eq!(Format::detect(&bytes), Some(Format::Jpeg));
}
