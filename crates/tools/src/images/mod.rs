//! Image engine: decoding with EXIF orientation and ICC profiles, encoding to
//! JPEG (mozjpeg), PNG, WebP and AVIF, and resizing (fast_image_resize).
//! Formats are detected from file content, never from the extension.

use std::{fs, io, io::Cursor, path::Path};

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{
    DynamicImage, ImageDecoder, ImageEncoder, ImageError, ImageFormat, ImageReader, Limits, Rgb, RgbImage,
    codecs::{avif::AvifEncoder, png::PngEncoder},
    error::LimitErrorKind,
};
use saqviro_jobs::{ErrorKind, JobError};

pub mod jpeg;
pub mod metadata;
pub mod tools;

/// Larger images are rejected before any pixel memory is allocated.
const MAX_SIDE: u32 = 32_768;
const MAX_ALLOC: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    Webp,
    Avif,
    Gif,
    Bmp,
    Tiff,
    Heic,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Avif => "avif",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Heic => "heic",
        }
    }

    /// Output formats chosen by name in tool parameters.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }

    pub fn detect(bytes: &[u8]) -> Option<Self> {
        let brand = bytes.get(8..12);
        if bytes.get(4..8) == Some(b"ftyp") && matches!(brand, Some(b"heic" | b"heix" | b"heim" | b"heis" | b"mif1")) {
            return Some(Self::Heic);
        }
        match image::guess_format(bytes).ok()? {
            ImageFormat::Jpeg => Some(Self::Jpeg),
            ImageFormat::Png => Some(Self::Png),
            ImageFormat::WebP => Some(Self::Webp),
            ImageFormat::Avif => Some(Self::Avif),
            ImageFormat::Gif => Some(Self::Gif),
            ImageFormat::Bmp => Some(Self::Bmp),
            ImageFormat::Tiff => Some(Self::Tiff),
            _ => None,
        }
    }

    fn image_format(self) -> Option<ImageFormat> {
        match self {
            Self::Jpeg => Some(ImageFormat::Jpeg),
            Self::Png => Some(ImageFormat::Png),
            Self::Webp => Some(ImageFormat::WebP),
            Self::Gif => Some(ImageFormat::Gif),
            Self::Bmp => Some(ImageFormat::Bmp),
            Self::Tiff => Some(ImageFormat::Tiff),
            Self::Avif | Self::Heic => None,
        }
    }
}

/// A decoded image, already turned upright according to its EXIF orientation.
pub struct Picture {
    pub image: DynamicImage,
    pub icc: Option<Vec<u8>>,
    pub format: Format,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("format", &self.format)
            .field("size", &(self.image.width(), self.image.height()))
            .field("icc_bytes", &self.icc.as_ref().map(Vec::len))
            .finish()
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn read(path: &Path) -> Result<Vec<u8>, JobError> {
    fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => JobError::new(ErrorKind::InvalidInput, format!("{} was not found", file_name(path))),
        _ => JobError::from(e),
    })
}

fn image_error(path: &Path, e: ImageError) -> JobError {
    let name = file_name(path);
    match e {
        ImageError::Limits(l)
            if matches!(
                l.kind(),
                LimitErrorKind::DimensionError | LimitErrorKind::InsufficientMemory
            ) =>
        {
            JobError::new(
                ErrorKind::InvalidInput,
                format!("{name} is too large to open (at most {MAX_SIDE}×{MAX_SIDE} pixels)"),
            )
        }
        ImageError::Unsupported(u) => JobError::new(ErrorKind::UnsupportedFormat, format!("{name}: {u}")),
        other => JobError::new(
            ErrorKind::InvalidInput,
            format!("{name} is damaged or incomplete: {other}"),
        ),
    }
}

pub fn decode(path: &Path) -> Result<Picture, JobError> {
    let bytes = read(path)?;
    let format = Format::detect(&bytes).ok_or_else(|| {
        JobError::new(
            ErrorKind::UnsupportedFormat,
            format!("{} is not an image Saqviro can read", file_name(path)),
        )
    })?;
    if format == Format::Heic {
        return heic(path, &bytes);
    }
    if format == Format::Jpeg && !jpeg::is_complete(&bytes) {
        return Err(JobError::new(
            ErrorKind::InvalidInput,
            format!("{} is damaged or incomplete", file_name(path)),
        ));
    }
    let image_format = format.image_format().ok_or_else(|| {
        JobError::new(
            ErrorKind::UnsupportedFormat,
            format!("reading {} files is not supported yet", format.ext().to_uppercase()),
        )
    })?;

    let mut reader = ImageReader::with_format(Cursor::new(&bytes), image_format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);

    let mut decoder = reader.into_decoder().map_err(|e| image_error(path, e))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let icc = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| image_error(path, e))?;
    image.apply_orientation(orientation);
    Ok(Picture { image, icc, format })
}

#[cfg(feature = "heic")]
pub mod heic;

#[cfg(feature = "heic")]
fn heic(path: &Path, bytes: &[u8]) -> Result<Picture, JobError> {
    heic::decode(path, bytes)
}

#[cfg(not(feature = "heic"))]
fn heic(path: &Path, _bytes: &[u8]) -> Result<Picture, JobError> {
    Err(JobError::new(
        ErrorKind::UnsupportedFormat,
        format!("{}: HEIC support is not included in this build", file_name(path)),
    ))
}

fn encode_error(e: impl std::fmt::Display) -> JobError {
    JobError::new(ErrorKind::Unknown, format!("could not encode the image: {e}"))
}

/// Blends transparent pixels onto white, for formats without alpha.
fn on_white(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        Rgb([blend(p[0]), blend(p[1]), blend(p[2])])
    })
}

/// 8-bit RGB or RGBA, the layouts every encoder accepts.
fn eight_bit(img: &DynamicImage) -> DynamicImage {
    if img.color().has_alpha() {
        DynamicImage::ImageRgba8(img.to_rgba8())
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8())
    }
}

/// `quality` is 1–100 and is ignored for PNG.
pub fn encode(img: &DynamicImage, icc: Option<&[u8]>, format: Format, quality: u8) -> Result<Vec<u8>, JobError> {
    let quality = quality.clamp(1, 100);
    match format {
        Format::Jpeg => encode_jpeg(&on_white(img), icc, quality),
        Format::Png => {
            let mut out = Vec::new();
            let mut encoder = PngEncoder::new(&mut out);
            if let Some(icc) = icc {
                let _ = encoder.set_icc_profile(icc.to_vec());
            }
            img.write_with_encoder(encoder).map_err(encode_error)?;
            Ok(out)
        }
        Format::Webp => {
            let img = eight_bit(img);
            let encoder = webp::Encoder::from_image(&img).map_err(encode_error)?;
            Ok(encoder.encode(quality as f32).to_vec())
        }
        Format::Avif => {
            let mut out = Vec::new();
            eight_bit(img)
                .write_with_encoder(AvifEncoder::new_with_speed_quality(&mut out, 6, quality))
                .map_err(encode_error)?;
            Ok(out)
        }
        other => Err(JobError::new(
            ErrorKind::UnsupportedFormat,
            format!("writing {} is not supported", other.ext()),
        )),
    }
}

fn encode_jpeg(rgb: &RgbImage, icc: Option<&[u8]>, quality: u8) -> Result<Vec<u8>, JobError> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(rgb.width() as usize, rgb.height() as usize);
    comp.set_quality(quality as f32);
    comp.set_progressive_mode();
    comp.set_optimize_coding(true);
    let mut started = comp.start_compress(Vec::new()).map_err(encode_error)?;
    if let Some(icc) = icc {
        // An ICC profile is split into APP2 segments of at most 65 519 bytes, numbered from 1.
        let chunks: Vec<&[u8]> = icc.chunks(65_519).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let mut payload = b"ICC_PROFILE\0".to_vec();
            payload.extend([(i + 1) as u8, chunks.len() as u8]);
            payload.extend(*chunk);
            started.write_marker(mozjpeg::Marker::APP(2), &payload);
        }
    }
    started.write_scanlines(rgb.as_raw()).map_err(encode_error)?;
    started.finish().map_err(encode_error)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Keep the aspect ratio and fit inside the box.
    Inside,
    /// Fill the box and crop the overflow, centered.
    Cover,
    /// Stretch to the box exactly.
    Fill,
}

impl Fit {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "inside" => Some(Self::Inside),
            "cover" => Some(Self::Cover),
            "fill" => Some(Self::Fill),
            _ => None,
        }
    }
}

/// Resizes, never enlarging: a box larger than the image shrinks to fit it.
pub fn resize(img: &DynamicImage, width: Option<u32>, height: Option<u32>, fit: Fit) -> Result<DynamicImage, JobError> {
    let (sw, sh) = (img.width() as f64, img.height() as f64);
    let fit = if width.is_none() || height.is_none() {
        Fit::Inside
    } else {
        fit
    };
    let (tw, th) = match (fit, width, height) {
        (_, None, None) => return Err(JobError::new(ErrorKind::InvalidInput, "set a width or a height")),
        (Fit::Inside, w, h) => {
            let scale = [w.map(|w| w as f64 / sw), h.map(|h| h as f64 / sh), Some(1.0)]
                .into_iter()
                .flatten()
                .fold(f64::INFINITY, f64::min);
            (sw * scale, sh * scale)
        }
        (Fit::Cover, Some(w), Some(h)) => {
            let scale = (w as f64 / sw).max(h as f64 / sh).max(1.0);
            (w as f64 / scale, h as f64 / scale)
        }
        (Fit::Fill, Some(w), Some(h)) => ((w as f64).min(sw), (h as f64).min(sh)),
        _ => unreachable!("a missing side always selects Inside"),
    };
    let (tw, th) = ((tw.round() as u32).max(1), (th.round() as u32).max(1));

    let src = eight_bit(img);
    if (tw, th) == (src.width(), src.height()) {
        return Ok(src);
    }
    let mut dst = DynamicImage::new(tw, th, src.color());
    let mut options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    if fit == Fit::Cover {
        options = options.fit_into_destination(Some((0.5, 0.5)));
    }
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| JobError::new(ErrorKind::Unknown, format!("resize failed: {e}")))?;
    Ok(dst)
}
