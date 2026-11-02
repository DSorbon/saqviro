//! HEIC/HEIF decoding through libheif. libheif applies the file's own
//! rotation, mirroring and cropping while decoding.

use std::path::Path;

use image::{DynamicImage, RgbImage, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
use saqviro_jobs::{ErrorKind, JobError};

use super::{Format, MAX_SIDE, Picture, file_name};

pub fn decode(path: &Path, bytes: &[u8]) -> Result<Picture, JobError> {
    let damaged = |e: libheif_rs::HeifError| {
        JobError::new(
            ErrorKind::InvalidInput,
            format!("{} is damaged or incomplete: {e}", file_name(path)),
        )
    };
    let ctx = HeifContext::read_from_bytes(bytes).map_err(damaged)?;
    let handle = ctx.primary_image_handle().map_err(damaged)?;
    if handle.width() > MAX_SIDE || handle.height() > MAX_SIDE {
        return Err(JobError::new(
            ErrorKind::InvalidInput,
            format!(
                "{} is too large to open (at most {MAX_SIDE}×{MAX_SIDE} pixels)",
                file_name(path)
            ),
        ));
    }

    let alpha = handle.has_alpha_channel();
    let chroma = if alpha { RgbChroma::Rgba } else { RgbChroma::Rgb };
    let decoded = LibHeif::new()
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(damaged)?;
    let plane = decoded
        .planes()
        .interleaved
        .ok_or_else(|| JobError::new(ErrorKind::Unknown, "libheif returned no pixels"))?;
    let (w, h) = (decoded.width(), decoded.height());

    // Rows may be padded: copy each row without its padding.
    let channels = if alpha { 4 } else { 3 };
    let row = w as usize * channels;
    let pixels: Vec<u8> = plane
        .data
        .chunks(plane.stride)
        .take(h as usize)
        .flat_map(|r| &r[..row])
        .copied()
        .collect();
    let image = if alpha {
        RgbaImage::from_raw(w, h, pixels).map(DynamicImage::ImageRgba8)
    } else {
        RgbImage::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8)
    }
    .ok_or_else(|| JobError::new(ErrorKind::Unknown, "libheif returned a short image"))?;

    let icc = handle.color_profile_raw().map(|p| p.data);
    Ok(Picture {
        image,
        icc,
        format: Format::Heic,
    })
}
