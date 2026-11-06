//! Subtitle Converter, between SRT and WebVTT.

use saqviro_jobs::Params;
use serde_json::Value;

use crate::params;

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start: i64,
    pub end: i64,
    pub text: String,
}

/// `00:01:02,500`, `00:01:02.500` or `01:02.500`, in milliseconds.
fn parse_time(text: &str) -> Option<i64> {
    let (clock, millis) = text.trim().split_once([',', '.'])?;
    let mut total = 0i64;
    let parts: Vec<&str> = clock.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    for part in parts {
        total = total * 60 + part.parse::<i64>().ok()?;
    }
    Some(total * 1000 + millis.parse::<i64>().ok()?)
}

fn format_time(ms: i64, separator: char) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn parse(text: &str) -> Result<Vec<Cue>, String> {
    let clean = text.trim_start_matches('\u{FEFF}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    for block in clean.split("\n\n") {
        let mut lines = block.lines();
        let Some(timing) = lines.by_ref().find(|l| l.contains("-->")) else {
            continue;
        };
        let (from, rest) = timing.split_once("-->").ok_or("a time line is damaged")?;
        // Anything after the end time is WebVTT positioning, which SRT has no way to say.
        let to = rest.split_whitespace().next().unwrap_or_default();
        let wrong = || format!("`{}` is not a subtitle time", timing.trim());
        cues.push(Cue {
            start: parse_time(from).ok_or_else(wrong)?,
            end: parse_time(to).ok_or_else(wrong)?,
            text: lines.collect::<Vec<_>>().join("\n"),
        });
    }
    if cues.is_empty() {
        return Err("no subtitles found; paste SRT or WebVTT text".into());
    }
    Ok(cues)
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let shift = params.get("shift_seconds").and_then(Value::as_f64).unwrap_or(0.0);
    let shift = (shift * 1000.0).round() as i64;
    let cues = parse(text)?;
    let vtt = params::text(params, "format") != "srt";
    let mut out = String::new();
    if vtt {
        out.push_str("WEBVTT\n\n");
    }
    let separator = if vtt { '.' } else { ',' };
    let mut number = 0;
    for cue in cues {
        // A cue moved entirely before the start is gone.
        if cue.end + shift <= 0 {
            continue;
        }
        number += 1;
        if !vtt {
            out.push_str(&format!("{number}\n"));
        }
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_time(cue.start + shift, separator),
            format_time(cue.end + shift, separator),
            cue.text
        ));
    }
    Ok(out)
}
