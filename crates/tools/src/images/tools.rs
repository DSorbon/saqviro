//! Image tool executors. Every tool processes a batch of files one by one,
//! writes each result next to its original, and reports progress per file.
//! The first failing file fails the whole job, and results already written
//! by that job are removed.

use std::{fs, io::Cursor, path::Path, sync::Arc};

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};

use super::{Fit, Format, Picture, decode, encode, metadata, read, resize as resize_image};
use crate::output::{Outputs, input_files, name_error, render_name, target_dir};

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
            let dir = target_dir(ctx, path);
            let written = outputs
                .write(&dir, &name, &made.bytes)
                .map_err(|e| with_name(path, e))?;
            let source_size = std::fs::metadata(path).ok().map(|m| m.len());
            results.push(OutputFile {
                path: written,
                size: made.bytes.len() as u64,
                source_size,
                note: None,
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

/// The largest centered area with this width-to-height ratio.
fn centered(size: (u32, u32), ratio: (u32, u32)) -> (u32, u32, u32, u32) {
    let (w, h) = (u64::from(size.0), u64::from(size.1));
    let (rw, rh) = (u64::from(ratio.0), u64::from(ratio.1));
    let (cw, ch) = if w * rh > h * rw {
        (h * rw / rh, h)
    } else {
        (w, w * rh / rw)
    };
    let (cw, ch) = (cw.max(1) as u32, ch.max(1) as u32);
    ((size.0 - cw) / 2, (size.1 - ch) / 2, cw, ch)
}

/// The area to keep: x, y, width, height. Reaches the image's edge when a
/// size is left out, and stops there when it is too large.
pub fn crop_area(size: (u32, u32), params: &Params) -> Result<(u32, u32, u32, u32), JobError> {
    if let Some((w, h)) = param_str(params, "aspect").and_then(|a| a.split_once(':'))
        && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
    {
        return Ok(centered(size, (w, h)));
    }
    let x = param_u32(params, "x").unwrap_or(0);
    let y = param_u32(params, "y").unwrap_or(0);
    if x >= size.0 || y >= size.1 {
        return Err(JobError::new(
            ErrorKind::InvalidInput,
            format!("the area starts outside the {}×{} image", size.0, size.1),
        ));
    }
    let width = param_u32(params, "width").map_or(size.0 - x, |w| w.min(size.0 - x));
    let height = param_u32(params, "height").map_or(size.1 - y, |h| h.min(size.1 - y));
    Ok((x, y, width, height))
}

pub fn crop(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let Picture { image, icc, format } = decode(path)?;
        let (x, y, width, height) = crop_area(image.dimensions(), params)?;
        let cropped = image.crop_imm(x, y, width, height);
        Made::image(&cropped, icc.as_deref(), writable(format), NEAR_LOSSLESS)
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

/// Reads `#rgb` or `#rrggbb`.
pub fn parse_color(text: &str) -> Result<[u8; 3], JobError> {
    let wrong = || JobError::new(ErrorKind::InvalidInput, format!("`{text}` is not a color, try #ff8800"));
    let hex = text.trim().trim_start_matches('#');
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()
        .ok_or_else(wrong)?;
    match digits[..] {
        [r, g, b] => Ok([r * 17, g * 17, b * 17]),
        [r1, r2, g1, g2, b1, b2] => Ok([r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2]),
        _ => Err(wrong()),
    }
}

fn color_param(params: &Params) -> Result<[u8; 3], JobError> {
    parse_color(param_str(params, "color").unwrap_or("#ffffff"))
}

/// Puts an image on a canvas of solid color at `x`, `y`.
fn on_canvas(image: &DynamicImage, size: (u32, u32), at: (i64, i64), color: [u8; 3]) -> DynamicImage {
    let [r, g, b] = color;
    let mut canvas = RgbaImage::from_pixel(size.0, size.1, Rgba([r, g, b, 255]));
    imageops::overlay(&mut canvas, &image.to_rgba8(), at.0, at.1);
    DynamicImage::ImageRgba8(canvas)
}

pub fn flip(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let Picture { image, icc, format } = decode(path)?;
        let flipped = if param_str(params, "direction") == Some("vertical") {
            image.flipv()
        } else {
            image.fliph()
        };
        Made::image(&flipped, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}

pub fn grayscale(template: String) -> Arc<dyn Executor> {
    batch(template, |path, _| {
        let Picture { image, format, .. } = decode(path)?;
        // The color profile of the source does not describe gray pixels.
        Made::image(&image.grayscale(), None, writable(format), NEAR_LOSSLESS)
    })
}

pub fn border(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let color = color_param(params)?;
        let width = param_u32(params, "width").unwrap_or(20);
        let Picture { image, icc, format } = decode(path)?;
        let (w, h) = image.dimensions();
        let size = (w + 2 * width, h + 2 * width);
        let framed = on_canvas(&image, size, (i64::from(width), i64::from(width)), color);
        Made::image(&framed, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}

/// Round Corners. `percent` is of the shorter side, so 50 makes a circle or a
/// pill. Edges are smoothed over one pixel.
pub fn round_corners_of(image: &DynamicImage, percent: u32) -> RgbaImage {
    let mut out = image.to_rgba8();
    let (w, h) = out.dimensions();
    let radius = f64::from(w.min(h)) * f64::from(percent.min(50)) / 100.0;
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let dx = (radius - px).max(px - (f64::from(w) - radius)).max(0.0);
        let dy = (radius - py).max(py - (f64::from(h) - radius)).max(0.0);
        if dx > 0.0 && dy > 0.0 {
            let coverage = (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
            pixel.0[3] = (f64::from(pixel.0[3]) * coverage) as u8;
        }
    }
    out
}

pub fn round_corners(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let Picture { image, icc, .. } = decode(path)?;
        let rounded = round_corners_of(&image, param_u32(params, "radius").unwrap_or(10));
        Made::image(
            &DynamicImage::ImageRgba8(rounded),
            icc.as_deref(),
            Format::Png,
            NEAR_LOSSLESS,
        )
    })
}

pub fn remove_alpha(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let color = color_param(params)?;
        let Picture { image, icc, format } = decode(path)?;
        let solid = on_canvas(&image, image.dimensions(), (0, 0), color);
        let solid = DynamicImage::ImageRgb8(solid.to_rgb8());
        Made::image(&solid, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}

/// A readable EXIF summary. Location is included: seeing it is why people look.
fn exif_lines(bytes: &[u8]) -> Vec<String> {
    use exif::{In, Tag};
    let Ok(data) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
        return Vec::new();
    };
    let tags = [
        ("Camera", Tag::Model),
        ("Lens", Tag::LensModel),
        ("Taken", Tag::DateTimeOriginal),
        ("Exposure", Tag::ExposureTime),
        ("Aperture", Tag::FNumber),
        ("ISO", Tag::PhotographicSensitivity),
        ("Focal length", Tag::FocalLength),
        ("Software", Tag::Software),
        ("Latitude", Tag::GPSLatitude),
        ("Longitude", Tag::GPSLongitude),
    ];
    tags.iter()
        .filter_map(|(label, tag)| {
            let field = data.get_field(*tag, In::PRIMARY)?;
            Some(format!("{label}: {}", field.display_value().with_unit(&data)))
        })
        .collect()
}

pub fn info() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let mut sections = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let bytes = read(path).map_err(|e| name_error(path, e))?;
            let picture = super::decode_bytes(path, &bytes).map_err(|e| name_error(path, e))?;
            let (w, h) = picture.image.dimensions();
            let mut lines = vec![
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                format!("Size: {w} × {h}"),
                format!("Format: {}", picture.format.ext().to_uppercase()),
                format!("File size: {} KB", (bytes.len() as u64).div_ceil(1024)),
                format!(
                    "Transparency: {}",
                    if picture.image.color().has_alpha() { "yes" } else { "no" }
                ),
                format!("Color profile: {}", if picture.icc.is_some() { "yes" } else { "no" }),
            ];
            lines.extend(exif_lines(&bytes));
            sections.push(lines.join("\n"));
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        Ok(JobOutput::Text(sections.join("\n\n")))
    })
}

/// Scales up as well as down, keeping the shape.
fn fit_within(image: &DynamicImage, side: u32) -> DynamicImage {
    image.resize(side, side, imageops::FilterType::Lanczos3)
}

fn square(image: &DynamicImage, side: u32) -> Result<DynamicImage, JobError> {
    let fitted = fit_within(image, side);
    let mut canvas = RgbaImage::new(side, side);
    let (w, h) = fitted.dimensions();
    let at = (i64::from((side - w.min(side)) / 2), i64::from((side - h.min(side)) / 2));
    imageops::overlay(&mut canvas, &fitted.to_rgba8(), at.0, at.1);
    Ok(DynamicImage::ImageRgba8(canvas))
}

fn png_bytes(image: &DynamicImage) -> Result<Vec<u8>, JobError> {
    encode(image, None, Format::Png, 100)
}

fn ico_bytes(image: &DynamicImage, sizes: &[u32]) -> Result<Vec<u8>, JobError> {
    use image::{
        ExtendedColorType,
        codecs::ico::{IcoEncoder, IcoFrame},
    };
    let wrong = |e: image::ImageError| JobError::new(ErrorKind::Unknown, format!("could not write the icon: {e}"));
    let frames = sizes
        .iter()
        .map(|side| {
            let png = png_bytes(&square(image, *side)?)?;
            IcoFrame::with_encoded(png, *side, *side, ExtendedColorType::Rgba8).map_err(wrong)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    IcoEncoder::new(&mut out).encode_images(&frames).map_err(wrong)?;
    Ok(out)
}

pub fn to_ico(template: String) -> Arc<dyn Executor> {
    batch(template, |path, _| {
        let picture = decode(path)?;
        Ok(Made {
            bytes: ico_bytes(&picture.image, &[16, 24, 32, 48, 64, 128, 256])?,
            ext: "ico",
            size: None,
        })
    })
}

/// Writes one folder per source with the files `make` returns.
fn folders(
    template: String,
    make: impl Fn(&Path) -> Result<Vec<(String, Vec<u8>)>, JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let made = make(path).map_err(|e| name_error(path, e))?;
            let name = render_name(&template, &[("stem", &stem(path))]);
            let folder = outputs.folder(&target_dir(ctx, path), &name)?;
            let mut size = 0;
            for (name, bytes) in made {
                size += bytes.len() as u64;
                fs::write(folder.join(name), bytes)?;
            }
            results.push(OutputFile {
                path: folder,
                size,
                source_size: None,
                note: None,
            });
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

const WEB_MANIFEST: &str = r#"{
  "icons": [
    { "src": "/icon-192.png", "type": "image/png", "sizes": "192x192" },
    { "src": "/icon-512.png", "type": "image/png", "sizes": "512x512" }
  ]
}
"#;

pub fn favicon(template: String) -> Arc<dyn Executor> {
    folders(template, |path| {
        let picture = decode(path)?;
        let mut files = vec![("favicon.ico".to_owned(), ico_bytes(&picture.image, &[16, 32, 48])?)];
        for (name, side) in [
            ("apple-touch-icon.png", 180),
            ("icon-192.png", 192),
            ("icon-512.png", 512),
        ] {
            files.push((name.to_owned(), png_bytes(&square(&picture.image, side)?)?));
        }
        files.push(("manifest.webmanifest".to_owned(), WEB_MANIFEST.as_bytes().to_vec()));
        Ok(files)
    })
}

pub fn gif_frames(template: String) -> Arc<dyn Executor> {
    use image::{AnimationDecoder, codecs::gif::GifDecoder};
    folders(template, |path| {
        let bytes = read(path)?;
        let wrong = |e: image::ImageError| {
            JobError::new(ErrorKind::UnsupportedFormat, format!("this GIF could not be read: {e}"))
        };
        let frames = GifDecoder::new(Cursor::new(bytes)).map_err(wrong)?.into_frames();
        let mut files = Vec::new();
        for (i, frame) in frames.enumerate() {
            let image = DynamicImage::ImageRgba8(frame.map_err(wrong)?.into_buffer());
            files.push((format!("frame-{:04}.png", i + 1), png_bytes(&image)?));
        }
        Ok(files)
    })
}

/// Writes the one result of a tool that makes one file out of many.
fn deliver(
    ctx: &JobContext,
    template: &str,
    first: &Path,
    bytes: &[u8],
    ext: &str,
    note: Option<String>,
) -> Result<JobOutput, JobError> {
    ctx.check_cancelled()?;
    let name = render_name(template, &[("stem", &stem(first)), ("ext", ext)]);
    let mut outputs = Outputs::new();
    let path = outputs.write(&target_dir(ctx, first), &name, bytes)?;
    outputs.finish();
    Ok(JobOutput::Files(vec![OutputFile {
        path,
        size: bytes.len() as u64,
        source_size: None,
        note,
    }]))
}

/// Contact Sheet. Pictures keep their shape inside square tiles.
pub fn contact_sheet(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let color = color_param(params)?;
        let tile = param_u32(params, "tile").unwrap_or(300);
        let gap = param_u32(params, "gap").unwrap_or(8);
        let columns = param_u32(params, "columns").unwrap_or(4).min(files.len() as u32).max(1);
        let rows = (files.len() as u32).div_ceil(columns);
        let side = |n: u32| n * tile + (n + 1) * gap;
        let (width, height) = (side(columns), side(rows));
        if u64::from(width) * u64::from(height) > 400_000_000 {
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                "this sheet would be too large; use smaller tiles",
            ));
        }
        let [r, g, b] = color;
        let mut sheet = RgbaImage::from_pixel(width, height, Rgba([r, g, b, 255]));
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let picture = decode(path).map_err(|e| name_error(path, e))?;
            let fitted = fit_within(&picture.image, tile);
            let (w, h) = fitted.dimensions();
            let (column, row) = (i as u32 % columns, i as u32 / columns);
            let x = gap + column * (tile + gap) + (tile - w.min(tile)) / 2;
            let y = gap + row * (tile + gap) + (tile - h.min(tile)) / 2;
            imageops::overlay(&mut sheet, &fitted.to_rgba8(), i64::from(x), i64::from(y));
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        let format = if param_str(params, "format") == Some("png") {
            Format::Png
        } else {
            Format::Jpeg
        };
        let bytes = encode(&DynamicImage::ImageRgba8(sheet), None, format, NEAR_LOSSLESS)?;
        deliver(ctx, &template, &files[0], &bytes, format.ext(), None)
    })
}

/// Frames to GIF. Every frame takes the size of the first.
pub fn frames_to_gif(template: String) -> Arc<dyn Executor> {
    use image::{
        Delay, Frame,
        codecs::gif::{GifEncoder, Repeat},
    };
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        if files.len() < 2 {
            return Err(JobError::new(ErrorKind::InvalidInput, "add at least 2 images"));
        }
        let delay = Delay::from_numer_denom_ms(param_u32(params, "delay_ms").unwrap_or(200), 1);
        let wrong = |e: image::ImageError| JobError::new(ErrorKind::Unknown, format!("could not write the GIF: {e}"));
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new_with_speed(&mut bytes, 10);
            encoder.set_repeat(Repeat::Infinite).map_err(wrong)?;
            let mut size = None;
            for (i, path) in files.iter().enumerate() {
                ctx.check_cancelled()?;
                let picture = decode(path).map_err(|e| name_error(path, e))?;
                let (w, h) = *size.get_or_insert_with(|| {
                    let (w, h) = picture.image.dimensions();
                    let width = param_u32(params, "width").unwrap_or(w);
                    (width, (u64::from(h) * u64::from(width) / u64::from(w)).max(1) as u32)
                });
                let fitted =
                    resize_image(&picture.image, Some(w), Some(h), Fit::Inside).map_err(|e| name_error(path, e))?;
                let (fw, fh) = fitted.dimensions();
                let mut canvas = RgbaImage::new(w, h);
                imageops::overlay(
                    &mut canvas,
                    &fitted.to_rgba8(),
                    i64::from((w - fw.min(w)) / 2),
                    i64::from((h - fh.min(h)) / 2),
                );
                encoder
                    .encode_frame(Frame::from_parts(canvas, 0, 0, delay))
                    .map_err(wrong)?;
                ctx.progress((i + 1) as f32 / files.len() as f32);
            }
        }
        deliver(ctx, &template, &files[0], &bytes, "gif", None)
    })
}

/// Pixels that differ, with a picture of where: changes in red over a pale copy.
pub fn difference(a: &DynamicImage, b: &DynamicImage) -> (f64, RgbaImage) {
    let (a, b) = (a.to_rgba8(), b.to_rgba8());
    let mut changed = 0u64;
    let mut map = RgbaImage::new(a.width(), a.height());
    for (x, y, pixel) in a.enumerate_pixels() {
        let other = b.get_pixel(x, y);
        let distance = pixel
            .0
            .iter()
            .zip(other.0)
            .map(|(p, o)| p.abs_diff(o))
            .max()
            .unwrap_or(0);
        if distance > 8 {
            changed += 1;
            map.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        } else {
            let [r, g, b, _] = pixel.0;
            let pale = |c: u8| 255 - (255 - c) / 4;
            map.put_pixel(x, y, Rgba([pale(r), pale(g), pale(b), 255]));
        }
    }
    (
        changed as f64 / (u64::from(a.width()) * u64::from(a.height())).max(1) as f64,
        map,
    )
}

pub fn compare(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let [first, second] = files.as_slice() else {
            return Err(JobError::new(ErrorKind::InvalidInput, "add exactly 2 images"));
        };
        let a = decode(first).map_err(|e| name_error(first, e))?;
        let b = decode(second).map_err(|e| name_error(second, e))?;
        if a.image.dimensions() != b.image.dimensions() {
            let (aw, ah) = a.image.dimensions();
            let (bw, bh) = b.image.dimensions();
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                format!("the images have different sizes: {aw}×{ah} and {bw}×{bh}"),
            ));
        }
        let (share, map) = difference(&a.image, &b.image);
        let note = if share == 0.0 {
            "The images are the same.".to_owned()
        } else {
            format!("{:.2}% of the pixels differ.", share * 100.0)
        };
        let bytes = png_bytes(&DynamicImage::ImageRgba8(map))?;
        deliver(ctx, &template, first, &bytes, "png", Some(note))
    })
}

/// SVG to PNG. Links to other files in the SVG are not followed: a picture
/// must not read files by being opened.
pub fn svg_to_png(template: String) -> Arc<dyn Executor> {
    use resvg::{tiny_skia, usvg};
    batch(template, |path, params| {
        let bytes = read(path)?;
        let mut options = usvg::Options::default();
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        super::overlay::load_fonts(&mut options);
        let wrong =
            |e: usvg::Error| JobError::new(ErrorKind::UnsupportedFormat, format!("this SVG could not be read: {e}"));
        let tree = usvg::Tree::from_data(&bytes, &options).map_err(wrong)?;
        let size = tree.size();
        let scale = param_u32(params, "width").map_or(1.0, |w| w as f32 / size.width());
        let (w, h) = (
            (size.width() * scale).ceil() as u32,
            (size.height() * scale).ceil() as u32,
        );
        if u64::from(w) * u64::from(h) > 400_000_000 {
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                "this picture would be too large",
            ));
        }
        let mut canvas = tiny_skia::Pixmap::new(w.max(1), h.max(1))
            .ok_or_else(|| JobError::new(ErrorKind::InvalidInput, "this SVG has no size"))?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut canvas.as_mut(),
        );
        let bytes = canvas
            .encode_png()
            .map_err(|e| JobError::new(ErrorKind::Unknown, format!("could not write the PNG: {e}")))?;
        Ok(Made {
            bytes,
            ext: "png",
            size: Some((w, h)),
        })
    })
}

pub fn watermark(template: String) -> Arc<dyn Executor> {
    batch(template, |path, params| {
        let mark = super::overlay::Watermark::from_params(params)?;
        let Picture { image, icc, format } = decode(path)?;
        Made::image(&mark.apply(&image)?, icc.as_deref(), writable(format), NEAR_LOSSLESS)
    })
}
