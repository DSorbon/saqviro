//! What a timeline shows: a few frames and the shape of the sound. Reads the
//! file and writes only to the job's temp folder.

use std::{ffi::OsString, fs, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, Params};
use serde::Serialize;

use super::Engine;

/// Enough to recognise a place in the film without waiting for it.
const FRAMES: u32 = 10;
const FRAME_HEIGHT: &str = "scale=-2:160";
/// One bar per pixel or two of a window-wide timeline.
const PEAKS: usize = 300;
/// Samples per second: the shape survives and an hour stays small.
const SAMPLE_RATE: &str = "2000";

#[derive(Debug, Serialize)]
struct Timeline {
    duration: f64,
    /// JPEGs in Base64, evenly spread over the file.
    frames: Vec<String>,
    /// Loudest moment of each slice, from 0 to 1.
    peaks: Vec<f32>,
}

/// The loudest sample of each of `count` slices, relative to the loudest of all.
pub fn peaks(samples: &[i16], count: usize) -> Vec<f32> {
    if samples.is_empty() || count == 0 {
        return Vec::new();
    }
    let slice = samples.len().div_ceil(count);
    let raw: Vec<u16> = samples
        .chunks(slice)
        .map(|c| c.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0))
        .collect();
    let top = f32::from(raw.iter().copied().max().unwrap_or(0).max(1));
    raw.iter().map(|p| f32::from(*p) / top).collect()
}

fn to_os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// Frames and sound of one file, as JSON for the interface.
pub fn timeline() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let JobInput::Files(files) = input else {
            return Err(JobError::new(ErrorKind::InvalidInput, "this works on a file"));
        };
        let path = files
            .first()
            .ok_or_else(|| JobError::new(ErrorKind::InvalidInput, "add a file"))?;
        let engine = Engine::get()?;
        let probe = engine.probe(ctx, path)?;
        let duration = probe
            .duration
            .ok_or_else(|| JobError::new(ErrorKind::UnsupportedFormat, "this file has no length"))?;
        let dir = ctx.temp_dir()?;
        let mut frames = Vec::new();
        if probe.video.is_some() {
            for i in 0..FRAMES {
                ctx.check_cancelled()?;
                let at = (f64::from(i) + 0.5) * duration / f64::from(FRAMES);
                let file = dir.join(format!("frame-{i}.jpg"));
                let mut args = to_os(&["-ss", &format!("{at:.3}")]);
                args.extend(Engine::input(path));
                args.extend(to_os(&["-frames:v", "1", "-vf", FRAME_HEIGHT, "-y"]));
                args.push(file.clone().into_os_string());
                engine.run(ctx, "error", args, |_| {})?;
                // A frame past the last one is simply left out.
                if let Ok(bytes) = fs::read(&file) {
                    frames.push(STANDARD.encode(bytes));
                }
            }
        }
        let mut peaks_found = Vec::new();
        if !probe.audio.is_empty() {
            let file = dir.join("sound.raw");
            let mut args = Engine::input(path);
            args.extend(to_os(&["-vn", "-ac", "1", "-ar", SAMPLE_RATE, "-f", "s16le", "-y"]));
            args.push(file.clone().into_os_string());
            engine.run(ctx, "error", args, |_| {})?;
            let bytes = fs::read(&file)?;
            let samples: Vec<i16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b))
                .collect();
            peaks_found = peaks(&samples, PEAKS);
        }
        let timeline = Timeline {
            duration,
            frames,
            peaks: peaks_found,
        };
        let json = serde_json::to_string(&timeline).map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?;
        Ok(JobOutput::Text(json))
    })
}
