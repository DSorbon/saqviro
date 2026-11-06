//! Printing on top of existing pages, in the base font every PDF reader has.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use saqviro_jobs::{ErrorKind, JobError};

const FONT: &[u8] = b"SaqviroStamp";
const STATE: &[u8] = b"SaqviroAlpha";

/// Helvetica's widths for ASCII 32 to 126, in thousandths of the font size.
const WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556,
    556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334,
    260, 334, 584,
];

pub fn text_width(text: &str, size: f32) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| {
            let at = (c as usize).wrapping_sub(32);
            u32::from(WIDTHS.get(at).copied().unwrap_or(556))
        })
        .sum();
    units as f32 * size / 1000.0
}

/// The base fonts cover Western European text only.
pub fn latin_bytes(text: &str) -> Result<Vec<u8>, JobError> {
    text.chars()
        .map(|c| match c as u32 {
            n @ (0x20..=0x7E | 0xA0..=0xFF) => Ok(n as u8),
            _ => Err(JobError::new(
                ErrorKind::InvalidInput,
                format!("`{c}` cannot be printed: only Western European letters are supported here"),
            )),
        })
        .collect()
}

/// A value a page can inherit from its parents.
fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut dict = doc.get_dictionary(page).ok()?;
    for _ in 0..64 {
        if let Ok(value) = dict.get(key) {
            return doc.dereference(value).ok().map(|(_, o)| o);
        }
        dict = doc
            .get_dictionary(dict.get(b"Parent").ok()?.as_reference().ok()?)
            .ok()?;
    }
    None
}

fn number(doc: &Document, object: &Object) -> Option<f32> {
    match doc.dereference(object).ok()?.1 {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// What a person sees of a page: where it is and how it is turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageView {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    /// Clockwise: 0, 90, 180 or 270.
    pub rotate: i64,
}

impl PageView {
    pub fn of(doc: &Document, page: ObjectId) -> Option<Self> {
        let rect = inherited(doc, page, b"CropBox").or_else(|| inherited(doc, page, b"MediaBox"))?;
        let [a, b, c, d] = rect.as_array().ok()?.as_slice() else {
            return None;
        };
        let (a, b, c, d) = (number(doc, a)?, number(doc, b)?, number(doc, c)?, number(doc, d)?);
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(|r| r.as_i64().ok())
            .unwrap_or(0);
        Some(Self {
            left: a.min(c),
            bottom: b.min(d),
            right: a.max(c),
            top: b.max(d),
            rotate: rotate.rem_euclid(360),
        })
    }

    /// Width and height as shown.
    pub fn size(&self) -> (f32, f32) {
        let (w, h) = (self.right - self.left, self.top - self.bottom);
        if matches!(self.rotate, 90 | 270) {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// A text matrix: a point as shown, turned by `angle` degrees as shown.
    pub fn matrix(&self, x: f32, y: f32, angle: f32) -> [f32; 6] {
        let (w, h) = (self.right - self.left, self.top - self.bottom);
        let (px, py, turn) = match self.rotate {
            90 => (w - y, x, 90.0),
            180 => (w - x, h - y, 180.0),
            270 => (y, h - x, 270.0),
            _ => (x, y, 0.0),
        };
        let (sin, cos) = (angle + turn as f32).to_radians().sin_cos();
        [cos, sin, -sin, cos, self.left + px, self.bottom + py]
    }
}

/// Text to print on a page.
pub struct Stamp {
    pub text: Vec<u8>,
    pub size: f32,
    pub matrix: [f32; 6],
    /// How much shows through, from 0.0 to 1.0.
    pub opacity: f32,
    pub gray: f32,
}

fn escaped(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for byte in text {
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(*byte);
    }
    out
}

fn content(stamp: &Stamp) -> Vec<u8> {
    let [a, b, c, d, e, f] = stamp.matrix;
    let mut out = format!(
        "q /{} gs {g} g BT /{} {size} Tf {a:.4} {b:.4} {c:.4} {d:.4} {e:.2} {f:.2} Tm (",
        String::from_utf8_lossy(STATE),
        String::from_utf8_lossy(FONT),
        g = stamp.gray,
        size = stamp.size,
    )
    .into_bytes();
    out.extend(escaped(&stamp.text));
    out.extend(b") Tj ET Q");
    out
}

/// The page's resources with the stamp's added. Written on the page itself,
/// since what it inherits may be shared with other pages.
fn add_resources(doc: &mut Document, page: ObjectId, font: ObjectId, opacity: f32) -> Result<(), lopdf::Error> {
    let mut resources: Dictionary = match inherited(doc, page, b"Resources") {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let sub = |resources: &Dictionary, doc: &Document, key: &[u8]| -> Dictionary {
        resources
            .get(key)
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok().cloned())
            .unwrap_or_default()
    };
    let mut fonts = sub(&resources, doc, b"Font");
    fonts.set(FONT.to_vec(), font);
    resources.set("Font", fonts);
    let mut states = sub(&resources, doc, b"ExtGState");
    states.set(STATE.to_vec(), dictionary! { "ca" => opacity });
    resources.set("ExtGState", states);
    doc.get_dictionary_mut(page)?.set("Resources", resources);
    Ok(())
}

/// Prints on one page. The old content is wrapped so nothing it leaves
/// behind, like a scale or a clip, can reach the stamp.
fn stamp_page(doc: &mut Document, page: ObjectId, font: ObjectId, stamp: &Stamp) -> Result<(), lopdf::Error> {
    add_resources(doc, page, font, stamp.opacity)?;
    let old = match doc.get_dictionary(page)?.get(b"Contents") {
        Ok(contents) => match doc.dereference(contents)?.1 {
            // The array itself can be an object of its own.
            Object::Array(items) => items.clone(),
            _ => vec![contents.clone()],
        },
        Err(_) => Vec::new(),
    };
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let close = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    let top = doc.add_object(Stream::new(Dictionary::new(), content(stamp)));
    let mut contents: Vec<Object> = vec![open.into()];
    contents.extend(old);
    contents.extend([close.into(), top.into()]);
    doc.get_dictionary_mut(page)?.set("Contents", contents);
    Ok(())
}

/// Prints what `make` returns on every page. It gets the page number from 1,
/// the page count and how the page is shown.
pub fn stamp_pages(
    doc: &mut Document,
    make: impl Fn(u32, u32, &PageView) -> Result<Stamp, JobError>,
) -> Result<(), JobError> {
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let pages = doc.get_pages();
    let count = pages.len() as u32;
    let wrong =
        |e: lopdf::Error| JobError::new(ErrorKind::UnsupportedFormat, format!("this PDF could not be read: {e}"));
    for (number, page) in pages {
        let view = PageView::of(doc, page)
            .ok_or_else(|| JobError::new(ErrorKind::UnsupportedFormat, format!("page {number} has no size")))?;
        let stamp = make(number, count, &view)?;
        stamp_page(doc, page, font, &stamp).map_err(wrong)?;
    }
    Ok(())
}

/// Trims every page by these margins, in points.
pub fn crop_pages(doc: &mut Document, top: f32, right: f32, bottom: f32, left: f32) -> Result<(), JobError> {
    for (number, page) in doc.get_pages() {
        let v = PageView::of(doc, page)
            .ok_or_else(|| JobError::new(ErrorKind::UnsupportedFormat, format!("page {number} has no size")))?;
        // Margins are given as a person sees the page.
        let (t, r, b, l) = match v.rotate {
            90 => (right, bottom, left, top),
            180 => (bottom, left, top, right),
            270 => (left, top, right, bottom),
            _ => (top, right, bottom, left),
        };
        let (x0, y0, x1, y1) = (v.left + l, v.bottom + b, v.right - r, v.top - t);
        if x0 >= x1 || y0 >= y1 {
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                format!("these margins leave nothing of page {number}"),
            ));
        }
        let rect: Vec<Object> = vec![x0.into(), y0.into(), x1.into(), y1.into()];
        let dict = doc
            .get_dictionary_mut(page)
            .map_err(|e| JobError::new(ErrorKind::UnsupportedFormat, e.to_string()))?;
        dict.set("CropBox", rect);
    }
    Ok(())
}

/// A PDF text string, in UTF-16 so any language fits.
pub fn text_string(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

pub fn read_text_string(object: &Object) -> Option<String> {
    let Object::String(bytes, _) = object else { return None };
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    Some(bytes.iter().map(|b| *b as char).collect())
}

/// The document's Info dictionary, created if there is none.
pub fn set_info(doc: &mut Document, entries: &[(&str, &str)]) -> Result<(), JobError> {
    let existing = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());
    let id = match existing {
        Some(id) => id,
        None => {
            let id = doc.add_object(Dictionary::new());
            doc.trailer.set("Info", id);
            id
        }
    };
    let info = doc
        .get_dictionary_mut(id)
        .map_err(|e| JobError::new(ErrorKind::UnsupportedFormat, e.to_string()))?;
    for (key, value) in entries {
        info.set(key.as_bytes().to_vec(), text_string(value));
    }
    Ok(())
}

/// The readable entries of the Info dictionary.
pub fn info_entries(doc: &Document) -> Vec<(String, String)> {
    let Some(info) = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok())
    else {
        return Vec::new();
    };
    [
        "Title",
        "Author",
        "Subject",
        "Keywords",
        "Creator",
        "Producer",
        "CreationDate",
        "ModDate",
    ]
    .into_iter()
    .filter_map(|key| {
        let value = info.get(key.as_bytes()).ok().and_then(|o| doc.dereference(o).ok())?.1;
        let text = read_text_string(value)?;
        (!text.trim().is_empty()).then(|| (key.to_owned(), text))
    })
    .collect()
}
