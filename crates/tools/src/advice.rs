//! Smart Drop: suggestions from what a file is like, not only what kind it
//! is. It reads a little of each file and never changes anything.

use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::Serialize;

const MB: u64 = 1024 * 1024;
const LARGE_VIDEO: u64 = 100 * MB;
const LARGE_IMAGE: u64 = 5 * MB;
const LARGE_PDF: u64 = 10 * MB;
const LARGE_AUDIO: u64 = 30 * MB;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Advice {
    /// The tool to suggest.
    pub tool: &'static str,
    /// Why, in a few words a person can read at a glance.
    pub reason: String,
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// True if a photo says where it was taken.
pub fn has_location(path: &Path) -> bool {
    let Ok(file) = File::open(path) else { return false };
    let Ok(data) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else {
        return false;
    };
    data.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY).is_some()
}

fn size_text(bytes: u64) -> String {
    crate::files::scan::format_size(bytes)
}

fn one_file(path: &Path) -> Vec<Advice> {
    let size = fs::metadata(path).map_or(0, |m| m.len());
    let large = |limit: u64, tool: &'static str, what: &str| {
        (size >= limit).then(|| Advice {
            tool,
            reason: format!("This {what} is {}", size_text(size)),
        })
    };
    let mut advice = Vec::new();
    match extension(path).as_str() {
        "jpg" | "jpeg" | "heic" | "heif" | "tif" | "tiff" | "png" | "webp" => {
            if has_location(path) {
                advice.push(Advice {
                    tool: "image.metadata.remove",
                    reason: "This photo shows where it was taken".into(),
                });
            }
            if matches!(extension(path).as_str(), "heic" | "heif") {
                advice.push(Advice {
                    tool: "image.heic-to-jpg",
                    reason: "Many apps cannot open HEIC".into(),
                });
            }
            advice.extend(large(LARGE_IMAGE, "image.compress", "image"));
        }
        "pdf" => advice.extend(large(LARGE_PDF, "pdf.compress", "PDF")),
        "mp4" | "mov" | "m4v" | "mkv" | "webm" | "avi" => advice.extend(large(LARGE_VIDEO, "video.compress", "video")),
        "wav" | "aiff" | "aif" | "flac" => advice.extend(large(LARGE_AUDIO, "audio.convert", "recording")),
        _ => {}
    }
    advice
}

/// What stands out about these files, most useful first, one entry per tool.
pub fn advise(paths: &[PathBuf]) -> Vec<Advice> {
    let mut found: Vec<Advice> = Vec::new();
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for path in paths {
        for advice in one_file(path) {
            match counts.iter_mut().find(|(tool, _)| *tool == advice.tool) {
                Some((_, count)) => *count += 1,
                None => {
                    counts.push((advice.tool, 1));
                    found.push(advice);
                }
            }
        }
    }
    for advice in &mut found {
        let count = counts
            .iter()
            .find(|(tool, _)| *tool == advice.tool)
            .map_or(1, |(_, n)| *n);
        if count > 1 {
            advice.reason = match advice.tool {
                "image.metadata.remove" => format!("{count} photos show where they were taken"),
                "image.heic-to-jpg" => advice.reason.clone(),
                _ => format!("{count} large files"),
            };
        }
    }
    found
}
