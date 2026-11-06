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

/// Backslash-escapes what the Wi-Fi code format gives a meaning.
fn wifi_escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The text a phone reads as "join this network".
pub fn wifi_text(params: &Params) -> Result<String, JobError> {
    let network = params::text(params, "network");
    if network.is_empty() {
        return Err(invalid("enter the name of the network"));
    }
    let security = params::text(params, "security");
    let password = params::text(params, "password");
    if security != "none" && password.is_empty() {
        return Err(invalid("enter the password of the network"));
    }
    let mut text = format!("WIFI:S:{};", wifi_escaped(network));
    if security == "none" {
        text.push_str("T:nopass;");
    } else {
        text.push_str(&format!("T:{security};P:{};", wifi_escaped(password)));
    }
    if params::flag(params, "hidden") {
        text.push_str("H:true;");
    }
    text.push(';');
    Ok(text)
}

fn vcard_escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}

/// A contact in vCard 3.0.
pub fn vcard_text(params: &Params) -> Result<String, JobError> {
    let name = params::text(params, "name");
    if name.trim().is_empty() {
        return Err(invalid("enter a name"));
    }
    let name = vcard_escaped(name);
    let mut lines = vec![
        "BEGIN:VCARD".to_owned(),
        "VERSION:3.0".to_owned(),
        format!("FN:{name}"),
        format!("N:{name};;;;"),
    ];
    for (id, key) in [
        ("phone", "TEL"),
        ("email", "EMAIL"),
        ("company", "ORG"),
        ("website", "URL"),
    ] {
        let value = params::text(params, id);
        if !value.is_empty() {
            lines.push(format!("{key}:{}", vcard_escaped(value)));
        }
    }
    lines.push("END:VCARD".to_owned());
    Ok(lines.join("\r\n"))
}

/// A QR code of what `text` makes from the parameters.
pub fn generate_from(
    text: fn(&Params) -> Result<String, JobError>,
) -> impl Fn(&JobContext, JobInput, &Params) -> Result<JobOutput, JobError> + Send + Sync {
    move |ctx, _, params| generate(ctx, JobInput::Text(text(params)?), params)
}

/// Code 128, Code 39 and EAN barcodes as a PNG.
pub fn barcode(_: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError> {
    use barcoders::{
        generators::image::{Color, Image, Rotation},
        sym::{code39::Code39, code128::Code128, ean8::EAN8, ean13::EAN13},
    };
    let JobInput::Text(text) = input else {
        return Err(invalid("this tool takes text, not files"));
    };
    let text = text.trim();
    let wrong = |e: barcoders::error::Error| invalid(format!("this text does not fit the format: {e}"));
    let bars = match params::text(params, "format") {
        "code39" => Code39::new(text).map_err(wrong)?.encode(),
        "ean13" => EAN13::new(text).map_err(wrong)?.encode(),
        "ean8" => EAN8::new(text).map_err(wrong)?.encode(),
        // Character set B: letters, digits and punctuation.
        _ => Code128::new(format!("\u{0181}{text}")).map_err(wrong)?.encode(),
    };
    let generator = Image::PNG {
        height: params::number(params, "height", 120).clamp(20, 1000),
        xdim: 3,
        rotation: Rotation::Zero,
        foreground: Color::black(),
        background: Color::white(),
    };
    let bytes = generator.generate(&bars[..]).map_err(wrong)?;
    Ok(JobOutput::Image(OutputImage {
        ext: "png".to_owned(),
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
