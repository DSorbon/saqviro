mod common;

use std::{fs, path::PathBuf};

use common::*;
use image::ImageFormat;
use saqviro_tools::advice::advise;

fn sized(dir: &std::path::Path, name: &str, mb: u64) -> PathBuf {
    let path = dir.join(name);
    fs::File::create(&path).unwrap().set_len(mb * 1024 * 1024).unwrap();
    path
}

/// A JPEG with a GPS latitude in its EXIF data.
fn located(dir: &std::path::Path, name: &str) -> PathBuf {
    let mut t = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    // IFD0: one entry, the pointer to the GPS IFD at offset 26.
    t.extend([0x88, 0x25, 0, 4, 0, 0, 0, 1, 0, 0, 0, 26]);
    t.extend([0, 0, 0, 0]);
    // GPS IFD: GPSLatitude, three rationals at offset 44.
    t.extend([0, 1, 0, 2, 0, 5, 0, 0, 0, 3, 0, 0, 0, 44]);
    t.extend([0, 0, 0, 0]);
    for (n, d) in [(40u32, 1u32), (0, 1), (0, 1)] {
        t.extend(n.to_be_bytes());
        t.extend(d.to_be_bytes());
    }
    let jpeg = encoded(&two_tone(8, 8), ImageFormat::Jpeg);
    write(dir, name, &jpeg_with_segments(&jpeg, &[(0xE1, t)]))
}

#[test]
fn calls_out_a_photo_that_shows_a_place() {
    let dir = tempfile::tempdir().unwrap();
    let found = advise(&[located(dir.path(), "a.jpg")]);
    assert_eq!(found[0].tool, "image.metadata.remove");
    assert_eq!(found[0].reason, "This photo shows where it was taken");
}

#[test]
fn counts_files_with_the_same_advice() {
    let dir = tempfile::tempdir().unwrap();
    let found = advise(&[located(dir.path(), "a.jpg"), located(dir.path(), "b.jpg")]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].reason, "2 photos show where they were taken");
}

#[test]
fn says_nothing_about_an_ordinary_photo() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "a.jpg", &encoded(&two_tone(8, 8), ImageFormat::Jpeg));
    assert_eq!(advise(&[path]), []);
}

#[test]
fn suggests_compressing_large_files() {
    let dir = tempfile::tempdir().unwrap();
    let found = advise(&[sized(dir.path(), "a.mov", 150)]);
    assert_eq!(found[0].tool, "video.compress");
    assert_eq!(found[0].reason, "This video is 150.0 MB");
    assert_eq!(advise(&[sized(dir.path(), "small.mov", 5)]), []);
    assert_eq!(advise(&[sized(dir.path(), "b.pdf", 20)])[0].tool, "pdf.compress");
    assert_eq!(advise(&[sized(dir.path(), "c.wav", 40)])[0].tool, "audio.convert");
}

#[test]
fn notices_heic() {
    let dir = tempfile::tempdir().unwrap();
    let found = advise(&[sized(dir.path(), "a.HEIC", 1)]);
    assert_eq!(found[0].tool, "image.heic-to-jpg");
}

#[test]
fn a_missing_file_is_just_not_advised_on() {
    assert_eq!(advise(&[PathBuf::from("/no/such/file.jpg")]), []);
}
