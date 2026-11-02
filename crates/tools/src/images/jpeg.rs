//! Walking JPEG segments without decoding pixels.

/// One marker segment: `marker` is the byte after 0xFF; `range` covers the whole
/// segment including the marker and length bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub marker: u8,
    pub range: std::ops::Range<usize>,
}

impl Segment {
    pub fn payload<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.range.start + 4..self.range.end]
    }
}

pub const SOS: u8 = 0xDA;
const EOI: u8 = 0xD9;

/// Segments from SOI up to and including the first SOS, plus the offset where
/// entropy-coded data starts. `None` if the header is malformed.
pub fn header_segments(bytes: &[u8]) -> Option<(Vec<Segment>, usize)> {
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return None;
    }
    let mut segments = Vec::new();
    let mut pos = 2;
    loop {
        // Markers may be preceded by any number of 0xFF fill bytes.
        while bytes.get(pos) == Some(&0xFF) && bytes.get(pos + 1) == Some(&0xFF) {
            pos += 1;
        }
        if bytes.get(pos) != Some(&0xFF) {
            return None;
        }
        let marker = *bytes.get(pos + 1)?;
        let len = u16::from_be_bytes([*bytes.get(pos + 2)?, *bytes.get(pos + 3)?]) as usize;
        if len < 2 || pos + 2 + len > bytes.len() {
            return None;
        }
        let range = pos..pos + 2 + len;
        pos = range.end;
        segments.push(Segment { marker, range });
        if marker == SOS {
            return Some((segments, pos));
        }
    }
}

/// True if the image data ends with an EOI marker. A file cut short in the
/// middle of the image data decodes "successfully" into a half-grey picture,
/// so this is checked before decoding.
pub fn is_complete(bytes: &[u8]) -> bool {
    image_end(bytes).is_some()
}

/// Offset just past the EOI marker of the main image. Anything after it
/// (vendor trailers, Motion Photo video) is not part of the picture.
pub fn image_end(bytes: &[u8]) -> Option<usize> {
    let (_, mut pos) = header_segments(bytes)?;
    // In entropy-coded data 0xFF is followed by 0x00 (stuffing), RSTn, or a real marker.
    while pos + 1 < bytes.len() {
        if bytes[pos] == 0xFF {
            match bytes[pos + 1] {
                EOI => return Some(pos + 2),
                0x00 | 0xD0..=0xD7 | 0xFF => pos += 1,
                // Another scan (progressive JPEG) or table: skip its segment.
                _ => {
                    let len = bytes
                        .get(pos + 2..pos + 4)
                        .map(|l| u16::from_be_bytes([l[0], l[1]]) as usize)?;
                    pos += 2 + len;
                    continue;
                }
            }
        }
        pos += 1;
    }
    None
}
