mod common;

use common::*;
use image::{GenericImageView, ImageFormat};
use saqviro_jobs::ErrorKind;
use saqviro_tools::images::{decode, jpeg, metadata::strip};

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn decoded_dims(bytes: &[u8]) -> (u32, u32) {
    let dir = tempfile::tempdir().unwrap();
    decode(&write(dir.path(), "x", bytes)).unwrap().image.dimensions()
}

#[test]
fn jpeg_loses_private_data_but_keeps_pixels_color_and_orientation() {
    let base = encoded(&two_tone(40, 20), ImageFormat::Jpeg);
    let profile = b"icc-profile".repeat(5);
    let mut original = jpeg_with_segments(
        &base,
        &[
            exif_app1(6, true),
            xmp_app1(),
            icc_app2(&profile),
            (0xFE, b"shot by Bob".to_vec()),
            (0xED, b"Photoshop 3.0\0IPTC".to_vec()),
        ],
    );
    original.extend(b"MotionPhoto_Data trailing video");

    let clean = strip(&original).unwrap();

    for private in [&b"Bob"[..], b"Dushanbe", b"IPTC", b"MotionPhoto", b"\x88\x25"] {
        assert!(
            !contains(&clean, private),
            "still contains {:?}",
            String::from_utf8_lossy(private)
        );
    }
    assert!(contains(&clean, &profile), "ICC profile was removed");
    assert_eq!(decoded_dims(&clean), (20, 40), "orientation was lost");

    let scan = |b: &[u8]| {
        let (_, start) = jpeg::header_segments(b).unwrap();
        b[start..].to_vec()
    };
    assert!(scan(&original).starts_with(&scan(&clean)), "pixel data changed");
    assert_eq!(strip(&clean).unwrap(), clean, "stripping is not idempotent");
}

#[test]
fn upright_jpeg_gets_no_exif_at_all() {
    let original = jpeg_with_segments(&encoded(&two_tone(40, 20), ImageFormat::Jpeg), &[exif_app1(1, true)]);
    let clean = strip(&original).unwrap();
    assert!(!contains(&clean, b"Exif\0\0"));
    assert_eq!(decoded_dims(&clean), (40, 20));
}

#[test]
fn png_keeps_only_rendering_chunks() {
    let base = encoded(&two_tone(40, 20), ImageFormat::Png);
    let original = png_with_chunks(
        &base,
        &[
            png_chunk(b"tEXt", b"Location\0Dushanbe"),
            png_chunk(b"iTXt", b"Author\0\0\0\0\0Bob"),
            png_chunk(b"tIME", &[7, 234, 11, 3, 12, 0, 0]),
            png_chunk(b"eXIf", &exif_tiff(6, true)),
            png_chunk(b"prVt", b"private vendor data"),
            png_chunk(b"pHYs", &[0, 0, 11, 19, 0, 0, 11, 19, 1]),
        ],
    );
    let clean = strip(&original).unwrap();
    for private in [&b"Dushanbe"[..], b"Bob", b"tIME", b"prVt"] {
        assert!(
            !contains(&clean, private),
            "still contains {:?}",
            String::from_utf8_lossy(private)
        );
    }
    assert!(contains(&clean, b"pHYs"), "rendering chunk was removed");
    assert_eq!(decoded_dims(&clean), (20, 40), "orientation was lost");
    assert_eq!(strip(&clean).unwrap(), clean);
}

#[test]
fn webp_loses_exif_and_xmp() {
    let simple = encoded(&two_tone(40, 20), ImageFormat::WebP);
    let original = webp_with_metadata(&simple, 40, 20);
    assert_eq!(decoded_dims(&original), (40, 20));

    let clean = strip(&original).unwrap();
    assert!(!contains(&clean, b"Dushanbe") && !contains(&clean, b"Bob"));
    assert_eq!(clean[20] & 0x0C, 0, "EXIF/XMP flags are still set");
    assert_eq!(
        u32::from_le_bytes(clean[4..8].try_into().unwrap()) as usize,
        clean.len() - 8
    );
    assert_eq!(decoded_dims(&clean), (40, 20));
    assert_eq!(strip(&clean).unwrap(), clean);
}

#[test]
fn other_formats_and_broken_files_are_rejected() {
    let gif = encoded(&two_tone(4, 4), ImageFormat::Gif);
    assert_eq!(strip(&gif).unwrap_err().kind, ErrorKind::UnsupportedFormat);

    let jpeg = encoded(&two_tone(40, 20), ImageFormat::Jpeg);
    assert_eq!(
        strip(&jpeg[..jpeg.len() / 2]).unwrap_err().kind,
        ErrorKind::InvalidInput
    );
}
