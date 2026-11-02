//! Image tool executors. Every tool processes a batch of files one by one,
//! writes each result next to its original, and reports progress per file.
//! The first failing file fails the whole job, and results already written
//! by that job are removed.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use image::{DynamicImage, GenericImageView};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};

use super::{Fit, Format, Picture, decode, encode, metadata, read, resize as resize_image};
use crate::output::{Outputs, render_name};

/// Quality used when a tool must re-encode but the user did not choose one.
const NEAR_LOSSLESS: u8 = 92;

fn param_str<'a>(params: &'a Params, id: &str) -> Option<&'a str> {
    params.get(id).and_then(|v| v.as_str())
}

fn param_u32(params: &Params, id: &str) -> Option<u32> {
    params
        .get(id)
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
}

fn quality(params: &Params) -> u8 {
    param_u32(params, "quality").map_or(85, |q| q.clamp(1, 100) as u8)
}

/// Formats a tool can write back unchanged; others fall back to PNG.
fn writable(format: Format) -> Format {
    match format {
        Format::Jpeg | Format::Png | Format::Webp => format,
        _ => Format::Png,
    }
}

/// What a tool produced for one file: encoded bytes plus name variables.
struct Made {
    bytes: Vec<u8>,
    ext: &'static str,
    size: Option<(u32, u32)>,
}

impl Made {
    fn image(img: &DynamicImage, icc: Option<&[u8]>, format: Format, quality: u8) -> Result<Self, JobError> {
        Ok(Self {
            bytes: encode(img, icc, format, quality)?,
            ext: format.ext(),
            size: Some(img.dimensions()),
        })
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "image".into(), |s| s.to_string_lossy().into_owned())
}

fn with_name(path: &Path, e: JobError) -> JobError {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if e.message.contains(&name) {
        e
    } else {
        JobError::new(e.kind, format!("{name}: {}", e.message))
    }
}

/// Runs `make` for each input file and writes the results.
fn batch(
    template: String,
    make: impl Fn(&Path, &Params) -> Result<Made, JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let JobInput::Files(files) = input else {
            return Err(JobError::new(ErrorKind::InvalidInput, "this tool works on files"));
        };
        if files.is_empty() {
            return Err(JobError::new(ErrorKind::InvalidInput, "add at least one file"));
        }

        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let made = make(path, params).map_err(|e| with_name(path, e))?;
            let (w, h) = made
                .size
                .map(|(w, h)| (w.to_string(), h.to_string()))
                .unwrap_or_default();
            let name = render_name(
                &template,
                &[("stem", &stem(path)), ("ext", made.ext), ("width", &w), ("height", &h)],
            );
            let dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let written = outputs
                .write(&dir, &name, &made.bytes)
                .map_err(|e| with_name(path, e))?;
            let source_size = std::fs::metadata(path).ok().map(|m| m.len());
            results.push(OutputFile {
                path: written,
                size: made.bytes.len() as u64,
                source_size,
            });
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

/// Convert Images, Image to WebP/AVIF, WebP to JPG/PNG. `fixed` overrides the `format` parameter.
pub fn convert(fixed: Option<Format>, template: String) -> Arc<dyn Executor> {
    batch(template, move |path, params| {
        let format = fixed
            .or_else(|| param_str(params, "format").and_then(Format::from_name))
            .ok_or_else(|| JobError::new(ErrorKind::InvalidInput, "choose an output format"))?;
        let pic = decode(path)?;
        Made::image(&pic.image, pic.icc.as_deref(), format, quality(params))
    })
}

pub fn resize(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let fit = param_str(params, "fit").and_then(Fit::from_name).unwrap_or(Fit::Inside);
        let Picture { image, icc, format } = decode(path)?;
        let resized = resize_image(&image, param_u32(params, "width"), param_u32(params, "height"), fit)?;
        Made::image(&resized, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}

/// Keeps the format. PNG is optimized losslessly; JPEG and WebP are re-encoded
/// at the chosen quality. A result is never bigger than its original.
pub fn compress(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let original = read(path)?;
        let format = Format::detect(&original);
        let bytes = match format {
            Some(Format::Png) => {
                let mut options = oxipng::Options::from_preset(3);
                options.strip = oxipng::StripChunks::Safe;
                oxipng::optimize_from_memory(&original, &options)
                    .map_err(|e| JobError::new(ErrorKind::InvalidInput, format!("could not optimize the PNG: {e}")))?
            }
            Some(f @ (Format::Jpeg | Format::Webp)) => {
                let pic = decode(path)?;
                encode(&pic.image, pic.icc.as_deref(), f, quality(params))?
            }
            _ => {
                return Err(JobError::new(
                    ErrorKind::UnsupportedFormat,
                    "JPG, PNG and WebP files can be compressed",
                ));
            }
        };
        let format = format.unwrap_or(Format::Png);
        let bytes = if bytes.len() < original.len() { bytes } else { original };
        Ok(Made {
            bytes,
            ext: format.ext(),
            size: None,
        })
    })
}

pub fn rotate(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let Picture { image, icc, format } = decode(path)?;
        let rotated = match param_str(params, "angle") {
            Some("180") => image.rotate180(),
            Some("270") => image.rotate270(),
            _ => image.rotate90(),
        };
        Made::image(&rotated, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}

/// Lossless. The result is checked: stripping it again must change nothing.
pub fn remove_metadata(template: String) -> Arc<dyn Executor> {
    batch(template, |path, _| {
        let original = read(path)?;
        let format = Format::detect(&original).unwrap_or(Format::Png);
        let clean = metadata::strip(&original)?;
        if metadata::strip(&clean)? != clean {
            return Err(JobError::new(
                ErrorKind::Unknown,
                "metadata could not be removed completely",
            ));
        }
        Ok(Made {
            bytes: clean,
            ext: format.ext(),
            size: None,
        })
    })
}
