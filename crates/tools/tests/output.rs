use std::fs;

use saqviro_jobs::ErrorKind;
use saqviro_tools::output::{Outputs, render_name};

#[test]
fn renders_known_variables_and_leaves_unknown_ones() {
    let name = render_name(
        "{stem}-{width}x{height}.{ext}",
        &[("stem", "cat"), ("width", "800"), ("ext", "jpg")],
    );
    assert_eq!(name, "cat-800x{height}.jpg");
}

#[test]
fn never_overwrites_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("photo.jpg"), b"user's original").unwrap();

    let mut out = Outputs::new();
    let a = out.write(dir.path(), "photo.jpg", b"new 1").unwrap();
    let b = out.write(dir.path(), "photo.jpg", b"new 2").unwrap();
    let c = out.write(dir.path(), "notes", b"no extension").unwrap();
    let d = out.write(dir.path(), "notes", b"no extension 2").unwrap();

    assert_eq!(a.file_name().unwrap(), "photo (2).jpg");
    assert_eq!(b.file_name().unwrap(), "photo (3).jpg");
    assert_eq!(c.file_name().unwrap(), "notes");
    assert_eq!(d.file_name().unwrap(), "notes (2)");
    assert_eq!(fs::read(dir.path().join("photo.jpg")).unwrap(), b"user's original");
    assert_eq!(fs::read(&b).unwrap(), b"new 2");
    assert_eq!(out.finish(), [a, b, c, d]);
}

#[test]
fn unfinished_outputs_are_removed_but_user_files_are_not() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("keep.png"), b"user").unwrap();
    let created = {
        let mut out = Outputs::new();
        out.write(dir.path(), "keep.png", b"partial").unwrap()
        // dropped without finish(): the job failed or was cancelled
    };
    assert!(!created.exists());
    assert!(dir.path().join("keep.png").exists());
}

#[test]
fn rejects_names_that_escape_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut out = Outputs::new();
    for bad in ["../evil.jpg", "a/b.jpg", "a\\b.jpg", "..", ""] {
        let err = out.write(dir.path(), bad, b"x").unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput, "{bad}");
    }
}

#[cfg(unix)]
#[test]
fn read_only_folders_report_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

    let err = Outputs::new().write(&locked, "a.jpg", b"x").unwrap_err();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(err.kind, ErrorKind::PermissionDenied);
}

#[test]
fn values_are_never_substituted_again() {
    let name = render_name("{stem}.{ext}", &[("stem", "{ext} report"), ("ext", "png")]);
    assert_eq!(name, "{ext} report.png");
}
