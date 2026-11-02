//! Test images are generated, not committed, so every fixture is license-clean.
#![allow(dead_code)]

use std::{fs, io::Cursor, path::Path, path::PathBuf};

use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

/// A 40×20 image whose left half is red and right half is blue, so rotation is observable.
pub fn two_tone(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            Rgb([220, 30, 30])
        } else {
            Rgb([30, 30, 220])
        }
    }))
}

pub fn transparent(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0])))
}

pub fn encoded(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let img = if format == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(img.to_rgb8())
    } else {
        img.clone()
    };
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

/// TIFF/EXIF data with an orientation tag and, optionally, a GPS IFD pointer and an Artist tag.
pub fn exif_tiff(orientation: u16, private: bool) -> Vec<u8> {
    let mut entries: Vec<(u16, u16, u32, u32)> = vec![(0x0112, 3, 1, (orientation as u32) << 16)];
    if private {
        entries.push((0x013B, 2, 4, u32::from_be_bytes(*b"Bob\0"))); // Artist
        entries.push((0x8825, 4, 1, 0)); // GPS IFD pointer (target is irrelevant for detection)
    }
    let mut t = b"MM\0\x2a\0\0\0\x08".to_vec();
    t.extend((entries.len() as u16).to_be_bytes());
    for (tag, ty, count, value) in entries {
        t.extend(tag.to_be_bytes());
        t.extend(ty.to_be_bytes());
        t.extend(count.to_be_bytes());
        t.extend(value.to_be_bytes());
    }
    t.extend(0u32.to_be_bytes());
    t
}

fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut s = vec![0xFF, marker];
    s.extend(((payload.len() + 2) as u16).to_be_bytes());
    s.extend(payload);
    s
}

/// Inserts segments right after SOI.
pub fn jpeg_with_segments(jpeg: &[u8], segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    for (marker, payload) in segments {
        out.extend(segment(*marker, payload));
    }
    out.extend(&jpeg[2..]);
    out
}

pub fn exif_app1(orientation: u16, private: bool) -> (u8, Vec<u8>) {
    let mut p = b"Exif\0\0".to_vec();
    p.extend(exif_tiff(orientation, private));
    (0xE1, p)
}

pub fn xmp_app1() -> (u8, Vec<u8>) {
    (
        0xE1,
        b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:Description photoshop:City=\"Dushanbe\"/></x:xmpmeta>".to_vec(),
    )
}

pub fn icc_app2(profile: &[u8]) -> (u8, Vec<u8>) {
    let mut p = b"ICC_PROFILE\0\x01\x01".to_vec();
    p.extend(profile);
    (0xE2, p)
}

pub fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = (data.len() as u32).to_be_bytes().to_vec();
    c.extend(kind);
    c.extend(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    c.extend(crc.finalize().to_be_bytes());
    c
}

/// Inserts chunks right after IHDR.
pub fn png_with_chunks(png: &[u8], chunks: &[Vec<u8>]) -> Vec<u8> {
    let ihdr_end = 8 + 12 + 13;
    let mut out = png[..ihdr_end].to_vec();
    for c in chunks {
        out.extend(c);
    }
    out.extend(&png[ihdr_end..]);
    out
}

/// A PNG that claims to be `w`×`h` but has no pixel data: a decompression bomb stand-in.
pub fn png_header_only(w: u32, h: u32) -> Vec<u8> {
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(png_chunk(b"IHDR", &ihdr));
    out.extend(png_chunk(b"IEND", &[]));
    out
}

/// An extended WebP (VP8X) with EXIF and XMP chunks around a simple WebP.
pub fn webp_with_metadata(simple: &[u8], w: u32, h: u32) -> Vec<u8> {
    let image_chunk = &simple[12..]; // VP8 or VP8L chunk with its header
    let chunk = |fourcc: &[u8; 4], data: &[u8]| {
        let mut c = fourcc.to_vec();
        c.extend((data.len() as u32).to_le_bytes());
        c.extend(data);
        if data.len() % 2 == 1 {
            c.push(0);
        }
        c
    };
    let mut vp8x = vec![0x08 | 0x04, 0, 0, 0];
    vp8x.extend(&(w - 1).to_le_bytes()[..3]);
    vp8x.extend(&(h - 1).to_le_bytes()[..3]);
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(b"VP8X", &vp8x));
    body.extend(image_chunk);
    body.extend(chunk(b"EXIF", &exif_tiff(1, true)));
    body.extend(chunk(b"XMP ", b"<x:xmpmeta>City=Dushanbe</x:xmpmeta>"));
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}
