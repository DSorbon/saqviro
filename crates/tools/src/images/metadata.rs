//! Lossless metadata removal for JPEG, PNG and WebP. Pixel data is copied
//! byte for byte; only containers around it change.
//!
//! Kept: what affects how the image looks (color profile, orientation,
//! physical size). Removed: everything else, including EXIF (GPS, camera,
//! author, dates), XMP, IPTC, comments, embedded thumbnails and trailing data.

use saqviro_jobs::{ErrorKind, JobError};

use super::{Format, jpeg};

fn damaged() -> JobError {
    JobError::new(ErrorKind::InvalidInput, "the file is damaged or incomplete")
}

pub fn strip(bytes: &[u8]) -> Result<Vec<u8>, JobError> {
    match Format::detect(bytes) {
        Some(Format::Jpeg) => strip_jpeg(bytes),
        Some(Format::Png) => strip_png(bytes),
        Some(Format::Webp) => strip_webp(bytes),
        _ => Err(JobError::new(
            ErrorKind::UnsupportedFormat,
            "metadata can be removed from JPEG, PNG and WebP files",
        )),
    }
}

/// EXIF orientation (1–8) from TIFF-structured EXIF data; 1 if absent or unreadable.
fn orientation(tiff: &[u8]) -> u16 {
    let read16 = |at: usize, be: bool| {
        tiff.get(at..at + 2).map(|b| {
            if be {
                u16::from_be_bytes([b[0], b[1]])
            } else {
                u16::from_le_bytes([b[0], b[1]])
            }
        })
    };
    let read32 = |at: usize, be: bool| {
        tiff.get(at..at + 4).map(|b| {
            let b = [b[0], b[1], b[2], b[3]];
            if be {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            }
        })
    };
    let be = match tiff.get(..2) {
        Some(b"MM") => true,
        Some(b"II") => false,
        _ => return 1,
    };
    let Some(ifd) = read32(4, be).map(|o| o as usize) else {
        return 1;
    };
    let count = read16(ifd, be).unwrap_or(0) as usize;
    (0..count)
        .map(|i| ifd + 2 + i * 12)
        .find(|&entry| read16(entry, be) == Some(0x0112))
        .and_then(|entry| read16(entry + 8, be))
        .filter(|o| (1..=8).contains(o))
        .unwrap_or(1)
}

/// TIFF-structured EXIF holding nothing but the orientation tag.
fn orientation_only(orientation: u16) -> Vec<u8> {
    let mut t = b"MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
    t.extend(orientation.to_be_bytes());
    t.extend([0, 0, 0, 0, 0, 0]);
    t
}

fn strip_jpeg(bytes: &[u8]) -> Result<Vec<u8>, JobError> {
    let (segments, data_start) = jpeg::header_segments(bytes).ok_or_else(damaged)?;
    let data_end = jpeg::image_end(bytes).ok_or_else(damaged)?;

    let mut orient = 1;
    let mut kept = Vec::new();
    for seg in &segments {
        let payload = seg.payload(bytes);
        let keep = match seg.marker {
            0xE1 => {
                if let Some(tiff) = payload.strip_prefix(b"Exif\0\0") {
                    orient = orientation(tiff);
                }
                false
            }
            0xE0 => payload.starts_with(b"JFIF\0"),
            0xE2 => payload.starts_with(b"ICC_PROFILE\0"),
            0xEE => payload.starts_with(b"Adobe"),
            0xE3..=0xEF | 0xFE => false,
            _ => true,
        };
        if keep {
            kept.push(seg);
        }
    }

    let mut out = Vec::with_capacity(data_end);
    out.extend([0xFF, 0xD8]);
    let jfif = kept.iter().take_while(|s| s.marker == 0xE0).count();
    for seg in &kept[..jfif] {
        out.extend(&bytes[seg.range.clone()]);
    }
    if orient != 1 {
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(orientation_only(orient));
        out.extend([0xFF, 0xE1]);
        out.extend(((app1.len() + 2) as u16).to_be_bytes());
        out.extend(app1);
    }
    for seg in &kept[jfif..] {
        out.extend(&bytes[seg.range.clone()]);
    }
    out.extend(&bytes[data_start..data_end]);
    Ok(out)
}

/// Chunks that affect rendering or animation. Everything else is dropped.
const PNG_KEEP: [&[u8; 4]; 18] = [
    b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"gAMA", b"cHRM", b"sRGB", b"iCCP", b"sBIT", b"pHYs", b"bKGD",
    b"cICP", b"mDCV", b"cLLI", b"acTL", b"fcTL", b"fdAT",
];

fn png_chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
    let mut c = (data.len() as u32).to_be_bytes().to_vec();
    c.extend(kind);
    c.extend(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    c.extend(crc.finalize().to_be_bytes());
    c
}

fn strip_png(bytes: &[u8]) -> Result<Vec<u8>, JobError> {
    let mut chunks = Vec::new();
    let mut pos = 8;
    let mut orient = 1;
    loop {
        let len = bytes
            .get(pos..pos + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(damaged)?;
        let end = pos
            .checked_add(12 + len)
            .filter(|&e| e <= bytes.len())
            .ok_or_else(damaged)?;
        let kind = &bytes[pos + 4..pos + 8];
        if kind == b"eXIf" {
            orient = orientation(&bytes[pos + 8..pos + 8 + len]);
        }
        chunks.push((kind, pos..end));
        pos = end;
        if kind == b"IEND" {
            break;
        }
    }

    let mut out = bytes[..8].to_vec();
    let mut exif_written = orient == 1;
    for (kind, range) in chunks {
        if kind == b"IDAT" && !exif_written {
            out.extend(png_chunk(b"eXIf", &orientation_only(orient)));
            exif_written = true;
        }
        if PNG_KEEP.iter().any(|k| k.as_slice() == kind) {
            out.extend(&bytes[range]);
        }
    }
    Ok(out)
}

const WEBP_KEEP: [&[u8; 4]; 7] = [b"VP8X", b"VP8 ", b"VP8L", b"ALPH", b"ANIM", b"ANMF", b"ICCP"];

fn strip_webp(bytes: &[u8]) -> Result<Vec<u8>, JobError> {
    let riff_len = bytes
        .get(4..8)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or_else(damaged)?;
    let end = riff_len
        .checked_add(8)
        .filter(|&e| e <= bytes.len())
        .ok_or_else(damaged)?;

    let mut body = b"WEBP".to_vec();
    let mut pos = 12;
    while pos < end {
        let kind = bytes.get(pos..pos + 4).ok_or_else(damaged)?;
        let len = bytes
            .get(pos + 4..pos + 8)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(damaged)?;
        let next = (pos + 8 + len + len % 2).min(end);
        if pos + 8 + len > end {
            return Err(damaged());
        }
        if WEBP_KEEP.iter().any(|k| k.as_slice() == kind) {
            let start = body.len();
            body.extend(&bytes[pos..next]);
            if kind == b"VP8X" {
                body[start + 8] &= !(0x08 | 0x04); // EXIF and XMP flags
            }
        }
        pos = next;
    }

    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}
