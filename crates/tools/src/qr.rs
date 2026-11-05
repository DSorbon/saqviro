//! QR Generator and QR Scanner.

use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, GrayImage, Luma};
use qrcode::{Color, EcLevel, QrCode, render::svg};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputImage, Params};

use crate::{
    images::{self, Format},
    output::{input_files, name_error},
    params,
};

/// Light modules around the code that scanners need.
const QUIET_ZONE: u32 = 4;

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn level(name: &str) -> EcLevel {
    match name {
        "low" => EcLevel::L,
        "quartile" => EcLevel::Q,
        "high" => EcLevel::H,
        _ => EcLevel::M,
    }
}

fn png(code: &QrCode, size: u32) -> Result<Vec<u8>, JobError> {
    let modules = code.width() as u32;
    let total = modules + 2 * QUIET_ZONE;
    let scale = (size / total).max(1);
    let colors = code.to_colors();
    let image = GrayImage::from_fn(total * scale, total * scale, |x, y| {
        let module = (x / scale)
            .checked_sub(QUIET_ZONE)
            .zip((y / scale).checked_sub(QUIET_ZONE));
        let dark = module
            .filter(|(mx, my)| *mx < modules && *my < modules)
            .is_some_and(|(mx, my)| colors[(my * modules + mx) as usize] == Color::Dark);
        Luma([if dark { 0 } else { 255 }])
    });
    images::encode(&DynamicImage::ImageLuma8(image), None, Format::Png, 100)
}

pub fn generate(_: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError> {
    let JobInput::Text(text) = input else {
        return Err(invalid("this tool takes text, not files"));
    };
    let code = QrCode::with_error_correction_level(text.as_bytes(), level(params::text(params, "correction")))
        .map_err(|_| invalid("this text is too long for a QR code"))?;
    let size = params::number(params, "size", 512).clamp(64, 4096);
    let (ext, bytes) = if params::text(params, "format") == "svg" {
        let svg = code.render::<svg::Color>().min_dimensions(size, size).build();
        ("svg", svg.into_bytes())
    } else {
        ("png", png(&code, size)?)
    };
    Ok(JobOutput::Image(OutputImage {
        ext: ext.to_owned(),
        data: STANDARD.encode(bytes),
    }))
}

/// Every QR code in a picture. The text is only ever shown, never opened.
pub fn read_codes(image: &DynamicImage) -> Vec<String> {
    let luma = image.to_luma8();
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(luma.width() as usize, luma.height() as usize, |x, y| {
            luma.get_pixel(x as u32, y as u32).0[0]
        });
    let grids = prepared.detect_grids();
    grids
        .iter()
        .filter_map(|g| g.decode().ok())
        .map(|(_, text)| text)
        .collect()
}

pub fn scan() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let mut sections = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let picture = images::decode(path).map_err(|e| name_error(path, e))?;
            let codes = read_codes(&picture.image);
            if codes.is_empty() {
                return Err(name_error(path, invalid("no QR code was found in this image")));
            }
            let text = codes.join("\n");
            let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            sections.push(if files.len() == 1 {
                text
            } else {
                format!("{name}\n{text}")
            });
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        Ok(JobOutput::Text(sections.join("\n\n")))
    })
}
