mod common;

use common::*;
use image::{GenericImageView, ImageFormat};
use saqviro_jobs::ErrorKind;
use saqviro_tools::images::{Fit, Format, decode, encode, resize};

#[test]
fn detects_the_format_from_content_not_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let img = two_tone(40, 20);
    for (format, expected) in [
        (ImageFormat::Jpeg, Format::Jpeg),
        (ImageFormat::Png, Format::Png),
        (ImageFormat::WebP, Format::Webp),
        (ImageFormat::Gif, Format::Gif),
        (ImageFormat::Bmp, Format::Bmp),
        (ImageFormat::Tiff, Format::Tiff),
    ] {
        let path = write(dir.path(), "wrong-extension.txt", &encoded(&img, format));
        let pic = decode(&path).unwrap();
        assert_eq!(pic.format, expected);
        assert_eq!(pic.image.dimensions(), (40, 20));
    }
}

#[test]
fn applies_exif_orientation() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = jpeg_with_segments(&encoded(&two_tone(40, 20), ImageFormat::Jpeg), &[exif_app1(6, false)]);
    let pic = decode(&write(dir.path(), "rotated.jpg", &jpeg)).unwrap();
    assert_eq!(pic.image.dimensions(), (20, 40));
    // Orientation 6 rotates 90° clockwise: the red left half ends up on top.
    let top = pic.image.get_pixel(10, 5);
    assert!(top[0] > 150 && top[2] < 100, "{top:?}");
}

#[test]
fn keeps_the_icc_profile_when_re_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let profile = b"fake-icc-profile-bytes".repeat(10);
    let jpeg = jpeg_with_segments(&encoded(&two_tone(40, 20), ImageFormat::Jpeg), &[icc_app2(&profile)]);
    let pic = decode(&write(dir.path(), "p3.jpg", &jpeg)).unwrap();
    assert_eq!(pic.icc.as_deref(), Some(profile.as_slice()));

    for format in [Format::Jpeg, Format::Png] {
        let bytes = encode(&pic.image, pic.icc.as_deref(), format, 90).unwrap();
        let again = decode(&write(dir.path(), "again", &bytes)).unwrap();
        assert_eq!(again.icc.as_deref(), Some(profile.as_slice()), "{format:?}");
    }
}

#[test]
fn encodes_every_output_format() {
    let dir = tempfile::tempdir().unwrap();
    let img = two_tone(40, 20);
    for format in [Format::Jpeg, Format::Png, Format::Webp] {
        let bytes = encode(&img, None, format, 80).unwrap();
        let pic = decode(&write(dir.path(), "out", &bytes)).unwrap();
        assert_eq!((pic.format, pic.image.dimensions()), (format, (40, 20)));
    }
    let avif = encode(&img, None, Format::Avif, 80).unwrap();
    assert_eq!(&avif[4..12], b"ftypavif");
}

#[test]
fn jpeg_puts_transparent_pixels_on_white() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = encode(&transparent(8, 8), None, Format::Jpeg, 90).unwrap();
    let px = decode(&write(dir.path(), "white.jpg", &bytes))
        .unwrap()
        .image
        .get_pixel(4, 4);
    assert!(px[0] > 245 && px[1] > 245 && px[2] > 245, "{px:?}");
}

#[test]
fn resizes_without_enlarging() {
    let img = two_tone(400, 300);
    let dims = |w, h, fit| resize(&img, w, h, fit).unwrap().dimensions();
    assert_eq!(dims(Some(200), None, Fit::Inside), (200, 150));
    assert_eq!(dims(None, Some(150), Fit::Inside), (200, 150));
    assert_eq!(dims(Some(200), Some(200), Fit::Inside), (200, 150));
    assert_eq!(dims(Some(100), Some(100), Fit::Cover), (100, 100));
    assert_eq!(dims(Some(100), Some(50), Fit::Fill), (100, 50));
    assert_eq!(dims(Some(1000), None, Fit::Inside), (400, 300), "never enlarges");
    assert_eq!(
        dims(Some(800), Some(800), Fit::Cover),
        (300, 300),
        "cover shrinks the box instead of enlarging"
    );
}

#[test]
fn resize_needs_a_width_or_height() {
    let err = resize(&two_tone(40, 20), None, None, Fit::Inside).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[test]
fn rejects_files_that_are_not_images() {
    let dir = tempfile::tempdir().unwrap();
    let err = decode(&write(dir.path(), "notes.jpg", b"just some text")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::UnsupportedFormat);
}

#[test]
fn rejects_truncated_images() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = encoded(&two_tone(400, 300), ImageFormat::Jpeg);
    let err = decode(&write(dir.path(), "cut.jpg", &jpeg[..jpeg.len() / 3])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[test]
fn rejects_decompression_bombs_before_allocating() {
    let dir = tempfile::tempdir().unwrap();
    let err = decode(&write(dir.path(), "bomb.png", &png_header_only(100_000, 100_000))).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
    assert!(err.message.contains("too large"), "{}", err.message);
}

#[test]
fn reports_missing_files() {
    let err = decode(std::path::Path::new("/definitely/not/here.png")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}
