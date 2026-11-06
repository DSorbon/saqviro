//! Text drawn on pictures. It goes through SVG so that it can use the fonts
//! on the system, and with them every language.

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops};
use resvg::{tiny_skia, usvg};
use saqviro_jobs::{ErrorKind, JobError, Params};
use serde_json::Value;

use super::tools::parse_color;

/// Space from the edge, as a share of the shorter side.
const MARGIN: f32 = 0.03;

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The fonts of this computer. `sans-serif` means Arial unless told otherwise,
/// and Linux usually has no Arial.
pub fn load_fonts(options: &mut usvg::Options<'_>) {
    let fonts = options.fontdb_mut();
    fonts.load_system_fonts();
    // Without fontconfig the usual search finds nothing on Linux.
    if fonts.is_empty() && cfg!(target_os = "linux") {
        fonts.load_fonts_dir("/usr/share/fonts");
    }
    let has = |family: &str| fonts.faces().any(|f| f.families.iter().any(|(name, _)| name == family));
    let wanted = ["Arial", "Helvetica", "Noto Sans", "DejaVu Sans", "Liberation Sans"];
    let found = wanted.into_iter().find(|f| has(f)).map(str::to_owned);
    let any = || {
        fonts
            .faces()
            .next()
            .and_then(|f| f.families.first())
            .map(|(name, _)| name.clone())
    };
    if let Some(family) = found.or_else(any) {
        fonts.set_sans_serif_family(family);
    }
}

/// What a watermark should look like, read from a tool's parameters.
#[derive(Debug, Clone)]
pub struct Watermark {
    text: String,
    /// Height of the letters, as a percentage of the shorter side.
    size: f32,
    position: String,
    /// Centre of the text in percent of the picture, when it was put by hand.
    at: Option<(f32, f32)>,
    opacity: f32,
    color: [u8; 3],
}

impl Watermark {
    pub fn from_params(params: &Params) -> Result<Self, JobError> {
        let text = |id: &str| params.get(id).and_then(Value::as_str).unwrap_or_default();
        let number = |id: &str, default: f32| params.get(id).and_then(Value::as_f64).map_or(default, |n| n as f32);
        if text("text").trim().is_empty() {
            return Err(invalid("enter the text to print"));
        }
        let color = if text("color").is_empty() {
            "#ffffff"
        } else {
            text("color")
        };
        let percent = |id: &str| params.get(id).and_then(Value::as_f64).map(|n| n as f32);
        Ok(Self {
            at: percent("x").zip(percent("y")),
            text: text("text").to_owned(),
            size: number("size", 5.0),
            position: if text("position").is_empty() {
                "bottom-right".to_owned()
            } else {
                text("position").to_owned()
            },
            opacity: number("opacity", 60.0) / 100.0,
            color: parse_color(color)?,
        })
    }

    /// The text alone, on a transparent background, tightly cut.
    fn draw(&self, height: f32) -> Result<RgbaImage, JobError> {
        let [r, g, b] = self.color;
        let width = height * (self.text.chars().count() as f32 + 2.0);
        let box_height = height * 1.6;
        // A dark edge keeps light text readable on a light picture.
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{box_height}"><text x="{x}" y="{y}" font-family="sans-serif" font-size="{height}" fill="rgb({r},{g},{b})" stroke="rgba(0,0,0,0.35)" stroke-width="{stroke}" paint-order="stroke">{text}</text></svg>"#,
            x = height * 0.2,
            y = height * 1.15,
            stroke = height * 0.04,
            text = escaped(&self.text),
        );
        let mut options = usvg::Options::default();
        load_fonts(&mut options);
        let tree =
            usvg::Tree::from_str(&svg, &options).map_err(|e| invalid(format!("could not draw the text: {e}")))?;
        let (w, h) = (width.ceil() as u32, box_height.ceil() as u32);
        let mut canvas = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| invalid("the text is too large"))?;
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut canvas.as_mut());
        let pixels = canvas
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        let drawn = RgbaImage::from_raw(w, h, pixels).ok_or_else(|| invalid("could not draw the text"))?;
        let cut = trimmed(&drawn);
        if cut.width() == 0 {
            return Err(JobError::new(
                ErrorKind::EngineMissing,
                "no font on this computer can draw this text",
            ));
        }
        Ok(cut)
    }

    fn place(&self, canvas: (u32, u32), stamp: (u32, u32)) -> (i64, i64) {
        let margin = (canvas.0.min(canvas.1) as f32 * MARGIN) as i64;
        let (cw, ch, sw, sh) = (
            i64::from(canvas.0),
            i64::from(canvas.1),
            i64::from(stamp.0),
            i64::from(stamp.1),
        );
        if let Some((x, y)) = self.at {
            let centre = |whole: i64, part: i64, percent: f32| {
                let at = (whole as f32 * percent / 100.0) as i64 - part / 2;
                at.clamp(0, (whole - part).max(0))
            };
            return (centre(cw, sw, x), centre(ch, sh, y));
        }
        match self.position.as_str() {
            "top-left" => (margin, margin),
            "top-right" => (cw - sw - margin, margin),
            "bottom-left" => (margin, ch - sh - margin),
            "center" => ((cw - sw) / 2, (ch - sh) / 2),
            _ => (cw - sw - margin, ch - sh - margin),
        }
    }

    /// A transparent picture of `size` with the text in its place.
    pub fn layer(&self, size: (u32, u32)) -> Result<RgbaImage, JobError> {
        let height = (size.0.min(size.1) as f32 * self.size / 100.0).max(6.0);
        let mut stamp = self.draw(height)?;
        // Text wider than the picture is made to fit.
        let room = (size.0 as f32 * (1.0 - 2.0 * MARGIN)).max(1.0) as u32;
        if stamp.width() > room {
            let h = (u64::from(stamp.height()) * u64::from(room) / u64::from(stamp.width())).max(1) as u32;
            stamp = imageops::resize(&stamp, room, h, imageops::FilterType::Triangle);
        }
        for pixel in stamp.pixels_mut() {
            pixel.0[3] = (f32::from(pixel.0[3]) * self.opacity) as u8;
        }
        let mut layer = RgbaImage::new(size.0, size.1);
        let (x, y) = self.place(size, stamp.dimensions());
        imageops::overlay(&mut layer, &stamp, x, y);
        Ok(layer)
    }

    pub fn apply(&self, image: &DynamicImage) -> Result<DynamicImage, JobError> {
        let mut out = image.to_rgba8();
        imageops::overlay(&mut out, &self.layer(image.dimensions())?, 0, 0);
        Ok(DynamicImage::ImageRgba8(out))
    }
}

/// The part of a picture that is not transparent.
fn trimmed(image: &RgbaImage) -> RgbaImage {
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, Rgba([_, _, _, alpha])) in image.enumerate_pixels() {
        if *alpha > 0 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if left == u32::MAX {
        return RgbaImage::new(0, 0);
    }
    imageops::crop_imm(image, left, top, right - left + 1, bottom - top + 1).to_image()
}
