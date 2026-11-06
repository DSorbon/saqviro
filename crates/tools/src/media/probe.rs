//! Reading what a media file contains from ffprobe's JSON.

use std::path::Path;

use saqviro_jobs::{ErrorKind, JobError};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub index: u64,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub bit_rate: Option<u64>,
    /// Degrees a player turns the picture by.
    pub rotation: i64,
}

impl VideoStream {
    /// Width and height as a player shows them.
    pub fn shown_size(&self) -> (u32, u32) {
        if self.rotation.rem_euclid(180) == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub index: u64,
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bit_rate: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Probe {
    pub format: String,
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub bit_rate: Option<u64>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
    /// Codec of each subtitle track.
    pub subtitles: Vec<String>,
}

/// ffprobe writes numbers as strings.
fn number<T: std::str::FromStr>(v: &Value, key: &str) -> Option<T> {
    match &v[key] {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.to_string().parse().ok(),
        _ => None,
    }
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("unknown").to_owned()
}

/// `30000/1001` → 29.97.
fn rate(v: &Value) -> Option<f64> {
    let (n, d) = v["avg_frame_rate"].as_str()?.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d > 0.0 && n > 0.0).then_some(n / d)
}

/// From the display matrix, or the `rotate` tag of older files.
fn rotation(stream: &Value) -> i64 {
    let side_data = stream["side_data_list"].as_array().map_or(&[][..], Vec::as_slice);
    side_data
        .iter()
        .find_map(|d| d["rotation"].as_f64())
        .map(|r| r.round() as i64)
        .or_else(|| number(&stream["tags"], "rotate"))
        .unwrap_or(0)
}

impl Probe {
    pub fn parse(json: &str) -> Result<Self, JobError> {
        let root: Value = serde_json::from_str(json)
            .map_err(|e| JobError::new(ErrorKind::ProcessFailed, format!("unreadable ffprobe output: {e}")))?;
        let format = &root["format"];
        let mut probe = Self {
            format: text(format, "format_name"),
            duration: number(format, "duration"),
            size: number(format, "size"),
            bit_rate: number(format, "bit_rate"),
            ..Self::default()
        };
        for stream in root["streams"].as_array().map_or(&[][..], Vec::as_slice) {
            let index = stream["index"].as_u64().unwrap_or_default();
            match stream["codec_type"].as_str() {
                // Cover art in an audio file is a video stream, but not a video.
                Some("video") if stream["disposition"]["attached_pic"].as_u64() != Some(1) => {
                    probe.video.get_or_insert(VideoStream {
                        index,
                        codec: text(stream, "codec_name"),
                        width: number(stream, "width").unwrap_or_default(),
                        height: number(stream, "height").unwrap_or_default(),
                        fps: rate(stream),
                        bit_rate: number(stream, "bit_rate"),
                        rotation: rotation(stream),
                    });
                }
                Some("audio") => probe.audio.push(AudioStream {
                    index,
                    codec: text(stream, "codec_name"),
                    sample_rate: number(stream, "sample_rate"),
                    channels: number(stream, "channels"),
                    bit_rate: number(stream, "bit_rate"),
                }),
                Some("subtitle") => probe.subtitles.push(text(stream, "codec_name")),
                _ => {}
            }
        }
        Ok(probe)
    }

    /// Bits per second of the video stream, measured or estimated.
    pub fn video_bit_rate(&self) -> Option<u64> {
        let video = self.video.as_ref()?;
        let audio: u64 = self.audio.iter().filter_map(|a| a.bit_rate).sum();
        video
            .bit_rate
            .or_else(|| self.bit_rate.map(|total| total.saturating_sub(audio)))
            .filter(|b| *b > 0)
    }
}

fn format_duration(seconds: f64) -> String {
    let whole = seconds as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        whole / 3600,
        whole / 60 % 60,
        whole % 60,
        (seconds.fract() * 1000.0) as u64
    )
}

fn format_rate(bits: u64) -> String {
    if bits >= 1_000_000 {
        format!("{:.1} Mbit/s", bits as f64 / 1_000_000.0)
    } else {
        format!("{} kbit/s", bits / 1000)
    }
}

fn format_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// A readable summary of one file.
pub fn describe(path: &Path, probe: &Probe) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let mut lines = vec![name.into_owned()];
    let mut add = |key: &str, value: String| lines.push(format!("{key}: {value}"));

    add("Format", probe.format.clone());
    if let Some(duration) = probe.duration {
        add("Duration", format_duration(duration));
    }
    if let Some(size) = probe.size {
        add("Size", format_size(size));
    }
    if let Some(rate) = probe.bit_rate {
        add("Bitrate", format_rate(rate));
    }
    if let Some(v) = &probe.video {
        let mut value = format!("{}, {}×{}", v.codec, v.width, v.height);
        if let Some(fps) = v.fps {
            value.push_str(&format!(", {fps:.2} fps"));
        }
        if let Some(rate) = probe.video_bit_rate() {
            value.push_str(&format!(", {}", format_rate(rate)));
        }
        add("Video", value);
    }
    for a in &probe.audio {
        let mut value = a.codec.clone();
        if let Some(hz) = a.sample_rate {
            value.push_str(&format!(", {hz} Hz"));
        }
        if let Some(channels) = a.channels {
            value.push_str(&format!(", {channels} ch"));
        }
        if let Some(rate) = a.bit_rate {
            value.push_str(&format!(", {}", format_rate(rate)));
        }
        add("Audio", value);
    }
    lines.join("\n")
}
