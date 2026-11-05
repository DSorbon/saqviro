//! Edits qpdf has no option for, on files `Qpdf::expand` has already decrypted
//! and decoded. Everything here is safe Rust, so a hostile PDF can fail these
//! functions but cannot corrupt memory.

use std::{collections::HashSet, io::Cursor, path::Path};

use image::{DynamicImage, GrayImage, ImageDecoder, ImageReader, RgbImage, metadata::Orientation};
use lopdf::{
    Dictionary, Document, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};
use saqviro_jobs::{ErrorKind, JobError};

use crate::images::{self, Fit, Format, jpeg};

const A4: (f32, f32) = (595.28, 841.89);
const LETTER: (f32, f32) = (612.0, 792.0);
/// PDF viewers refuse pages larger than this, in points.
const MAX_PAGE: f32 = 14_400.0;
/// Points per pixel for a page sized to its image: 96 pixels per inch.
const POINTS_PER_PIXEL: f32 = 0.75;
/// Largest image that gets decoded here.
const MAX_PIXELS: u64 = 400_000_000;

fn pdf_error(e: lopdf::Error) -> JobError {
    JobError::new(ErrorKind::UnsupportedFormat, format!("this PDF could not be read: {e}"))
}

pub fn load(path: &Path) -> Result<Document, JobError> {
    Document::load(path).map_err(pdf_error)
}

pub fn save(doc: &mut Document, path: &Path) -> Result<(), JobError> {
    doc.save(path).map(drop).map_err(JobError::from)
}

/// Removes document properties and XMP, wherever they are attached.
pub fn strip_metadata(doc: &mut Document) {
    doc.trailer.remove(b"Info");
    for object in doc.objects.values_mut() {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &mut stream.dict,
            _ => continue,
        };
        dict.remove(b"Metadata");
        dict.remove(b"PieceInfo");
        dict.remove(b"LastModified");
    }
    doc.prune_objects();
}

fn number(object: &Object) -> Option<f32> {
    object
        .as_float()
        .ok()
        .or_else(|| object.as_i64().ok().map(|n| n as f32))
}

fn resolved<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    doc.dereference(object).ok().map(|(_, o)| o)
}

/// A page's MediaBox, which it can inherit from its parents.
fn media_box(doc: &Document, page: ObjectId) -> Option<(f32, f32)> {
    let mut dict = doc.get_dictionary(page).ok()?;
    for _ in 0..64 {
        if let Some(rect) = dict.get(b"MediaBox").ok().and_then(|o| resolved(doc, o))
            && let Ok(rect) = rect.as_array()
            && let [x0, y0, x1, y1] = rect.as_slice()
        {
            let side = |a: &Object, b: &Object| Some((number(resolved(doc, b)?)? - number(resolved(doc, a)?)?).abs());
            return Some((side(x0, x1)?, side(y0, y1)?));
        }
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
    None
}

/// The longest page side, in points.
fn longest_side(doc: &Document) -> f32 {
    let pages = doc.get_pages();
    let sides = pages.values().filter_map(|p| media_box(doc, *p));
    sides.map(|(w, h)| w.max(h)).fold(0.0, f32::max).max(1.0)
}

/// Images used as masks: they stay exact.
fn mask_ids(doc: &Document) -> HashSet<ObjectId> {
    let mut ids = HashSet::new();
    for object in doc.objects.values() {
        if let Object::Stream(stream) = object {
            for key in [&b"SMask"[..], b"Mask"] {
                if let Ok(id) = stream.dict.get(key).and_then(Object::as_reference) {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}

/// Components of a color space this code can handle: gray or RGB.
fn components(doc: &Document, space: &Object) -> Option<u32> {
    let space = resolved(doc, space)?;
    if let Ok(name) = space.as_name() {
        return match name {
            b"DeviceGray" | b"CalGray" => Some(1),
            b"DeviceRGB" | b"CalRGB" => Some(3),
            _ => None,
        };
    }
    let array = space.as_array().ok()?;
    match array.first().and_then(|o| o.as_name().ok())? {
        b"ICCBased" => {
            let profile = resolved(doc, array.get(1)?)?.as_stream().ok()?;
            let n = profile.dict.get(b"N").ok()?.as_i64().ok()?;
            matches!(n, 1 | 3).then_some(n as u32)
        }
        b"CalGray" => Some(1),
        b"CalRGB" => Some(3),
        _ => None,
    }
}

/// Pixels of one image XObject, if it is a kind that can be re-encoded.
fn pixels(doc: &Document, stream: &Stream) -> Option<DynamicImage> {
    let dict = &stream.dict;
    let int = |key: &[u8]| dict.get(key).ok().and_then(|o| o.as_i64().ok());
    if dict.has(b"Decode") || dict.get(b"ImageMask").is_ok_and(|o| o.as_bool().unwrap_or(false)) {
        return None;
    }
    let (w, h) = (
        u32::try_from(int(b"Width")?).ok()?,
        u32::try_from(int(b"Height")?).ok()?,
    );
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return None;
    }
    let n = components(doc, dict.get(b"ColorSpace").ok()?)?;

    let filter = dict.get(b"Filter").ok().and_then(|o| resolved(doc, o));
    let filter = match filter {
        None => None,
        Some(Object::Array(list)) if list.len() == 1 => list[0].as_name().ok(),
        Some(Object::Array(list)) if list.is_empty() => None,
        Some(other) => Some(other.as_name().ok()?),
    };
    match filter {
        Some(b"DCTDecode") => {
            if jpeg::components(&stream.content)? != n {
                return None;
            }
            images::decode_bytes(Path::new("image"), &stream.content)
                .ok()
                .map(|p| p.image)
        }
        None if int(b"BitsPerComponent") == Some(8) => {
            let raw = stream.content.clone();
            if n == 1 {
                GrayImage::from_raw(w, h, raw).map(DynamicImage::ImageLuma8)
            } else {
                RgbImage::from_raw(w, h, raw).map(DynamicImage::ImageRgb8)
            }
        }
        _ => None,
    }
}

/// What `recompress_images` did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Recompressed {
    pub images: usize,
    pub saved: u64,
}

/// Encodes images again as JPEG at `quality`, scaling down those with more
/// pixels than the largest page shows at `dpi`. Keeps whichever is smaller.
/// `keep_going` is asked before each image and can stop the work.
pub fn recompress_images(
    doc: &mut Document,
    dpi: u32,
    quality: u8,
    mut keep_going: impl FnMut(f32) -> Result<(), JobError>,
) -> Result<Recompressed, JobError> {
    let limit = (longest_side(doc) / 72.0 * dpi as f32).ceil() as u32;
    let masks = mask_ids(doc);
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, object)| {
            !masks.contains(id)
                && object.as_stream().is_ok_and(|s| {
                    s.dict
                        .get(b"Subtype")
                        .is_ok_and(|t| t.as_name().is_ok_and(|n| n == b"Image"))
                })
        })
        .map(|(id, _)| *id)
        .collect();

    let mut done = Recompressed::default();
    for (i, id) in ids.iter().enumerate() {
        keep_going(i as f32 / ids.len() as f32)?;
        let Some(stream) = doc.objects.get(id).and_then(|o| o.as_stream().ok()) else {
            continue;
        };
        let Some(mut image) = pixels(doc, stream) else {
            continue;
        };
        let before = stream.content.len() as u64;
        if image.width().max(image.height()) > limit {
            image = images::resize(&image, Some(limit), Some(limit), Fit::Inside)?;
        }
        let encoded = images::encode(&image, None, Format::Jpeg, quality)?;
        if encoded.len() as u64 >= before {
            continue;
        }
        let (w, h) = (image.width(), image.height());
        // The encoder writes RGB, so gray images change color space with it.
        let rgb_space = stream
            .dict
            .get(b"ColorSpace")
            .is_ok_and(|s| components(doc, s) == Some(3));
        let Some(Object::Stream(stream)) = doc.objects.get_mut(id) else {
            continue;
        };
        done.images += 1;
        done.saved += before - encoded.len() as u64;
        stream.dict.set("Width", i64::from(w));
        stream.dict.set("Height", i64::from(h));
        stream.dict.set("BitsPerComponent", 8);
        stream.dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        stream.dict.remove(b"DecodeParms");
        if !rgb_space {
            stream.dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        }
        stream.set_content(encoded);
        stream.allows_compression = false;
    }
    Ok(done)
}

/// Page size of a new PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// Each page is as large as its image.
    Fit,
    A4,
    Letter,
}

impl PageSize {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "fit" => Some(Self::Fit),
            "a4" => Some(Self::A4),
            "letter" => Some(Self::Letter),
            _ => None,
        }
    }

    /// The page for an image, and where the image goes on it: x, y, w, h.
    pub fn place(self, w: u32, h: u32) -> ((f32, f32), (f32, f32, f32, f32)) {
        let (w, h) = (w as f32, h as f32);
        let paper = match self {
            Self::Fit => {
                let scale = POINTS_PER_PIXEL.min(MAX_PAGE / w.max(h));
                let size = (w * scale, h * scale);
                return (size, (0.0, 0.0, size.0, size.1));
            }
            Self::A4 => A4,
            Self::Letter => LETTER,
        };
        let page = if w > h { (paper.1, paper.0) } else { paper };
        let scale = (page.0 / w).min(page.1 / h);
        let (iw, ih) = (w * scale, h * scale);
        (page, ((page.0 - iw) / 2.0, (page.1 - ih) / 2.0, iw, ih))
    }
}

/// True if the JPEG can go into a PDF as it is. PDFs have no EXIF orientation
/// and no CMYK handling here, so those files are encoded again.
fn embeddable_jpeg(bytes: &[u8]) -> Option<&'static str> {
    let space = match jpeg::components(bytes)? {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        _ => return None,
    };
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg)
        .into_decoder()
        .ok()?;
    let orientation = decoder.orientation().ok()?;
    (orientation == Orientation::NoTransforms).then_some(space)
}

fn image_stream(bytes: Vec<u8>, picture: &images::Picture) -> Result<Stream, JobError> {
    let (w, h) = (picture.image.width(), picture.image.height());
    let dict = |space: &str, filter: Option<&str>| {
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(w),
            "Height" => i64::from(h),
            "BitsPerComponent" => 8,
        };
        dict.set("ColorSpace", Object::Name(space.as_bytes().to_vec()));
        if let Some(filter) = filter {
            dict.set("Filter", Object::Name(filter.as_bytes().to_vec()));
        }
        dict
    };
    if picture.format == Format::Jpeg
        && let Some(space) = embeddable_jpeg(&bytes)
    {
        let mut stream = Stream::new(dict(space, Some("DCTDecode")), bytes);
        stream.allows_compression = false;
        return Ok(stream);
    }
    // Transparent areas become white, as they would on paper.
    let rgb = images::on_white(&picture.image);
    Ok(Stream::new(dict("DeviceRGB", None), rgb.into_raw()))
}

fn real(n: f32) -> Object {
    Object::Real(n)
}

/// Builds a PDF with one page per image, in order.
pub fn from_images(
    paths: &[std::path::PathBuf],
    size: PageSize,
    mut keep_going: impl FnMut(f32) -> Result<(), JobError>,
) -> Result<Document, JobError> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(paths.len());

    for (i, path) in paths.iter().enumerate() {
        keep_going(i as f32 / paths.len() as f32)?;
        let bytes = images::read(path)?;
        let picture = images::decode_bytes(path, &bytes)?;
        let (page, (x, y, w, h)) = size.place(picture.image.width(), picture.image.height());

        let image_id = doc.add_object(image_stream(bytes, &picture)?);
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new("cm", vec![real(w), 0.into(), 0.into(), real(h), real(x), real(y)]),
                Operation::new("Do", vec![Object::Name(b"Im".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let encoded = content.encode().map_err(pdf_error)?;
        let content_id = doc.add_object(Stream::new(Dictionary::new(), encoded));
        kids.push(Object::Reference(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), real(page.0), real(page.1)],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im" => image_id } },
        })));
    }

    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    Ok(doc)
}
