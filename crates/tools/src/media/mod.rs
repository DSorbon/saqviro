//! Video and audio through FFmpeg, run as a separate process. See
//! `docs/decisions/0003-media-engine.md` for why it works this way.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

use saqviro_jobs::{ErrorKind, JobContext, JobError};

use crate::process::{self, hide_console};

pub mod probe;
pub mod tools;

use probe::Probe;

/// Demuxers allowed to open user files. Formats that reference other files
/// (concat, HLS, DASH) are deliberately missing.
const FORMATS: &str = "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,mpeg,mpegts,asf,flv,ogg,mp3,aac,wav,flac,aiff,gif";

/// H.264 encoders from the operating system, tried in order.
const H264_ENCODERS: &[&str] = if cfg!(target_os = "macos") {
    &["h264_videotoolbox"]
} else if cfg!(windows) {
    &["h264_mf"]
} else {
    &[]
};

/// Seconds of media written so far, from one line of `-progress` output.
pub fn parse_progress(line: &str) -> Option<f64> {
    let micros: f64 = line.strip_prefix("out_time_us=")?.trim().parse().ok()?;
    (micros >= 0.0).then_some(micros / 1_000_000.0)
}

/// Turns FFmpeg's stderr into an error a person can read.
pub fn failure(stderr: &str) -> JobError {
    let line = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty());
    let line = line.unwrap_or("FFmpeg stopped without saying why");
    if stderr.contains("No space left on device") {
        JobError::new(ErrorKind::DiskFull, "not enough disk space")
    } else if stderr.contains("Invalid data found") || stderr.contains("moov atom not found") {
        JobError::new(
            ErrorKind::UnsupportedFormat,
            "this file is damaged or is not a media file",
        )
    } else if stderr.contains("not on whitelist") {
        JobError::new(ErrorKind::UnsupportedFormat, "this file type is not supported")
    } else {
        JobError::new(ErrorKind::ProcessFailed, line)
    }
}

/// Runs a process to the end and returns its stderr.
fn supervise(ctx: &JobContext, command: &mut Command, on_line: impl FnMut(&str) + Send) -> Result<String, JobError> {
    let (status, stderr) = process::supervise(ctx, command, on_line)?;
    if status.success() {
        Ok(stderr)
    } else {
        Err(failure(&stderr))
    }
}

/// What this machine can encode video with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoders {
    pub h264: Option<&'static str>,
    pub vp9: bool,
}

/// Located FFmpeg binaries.
#[derive(Debug)]
pub struct Engine {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    encoders: OnceLock<Encoders>,
}

impl Engine {
    /// The engine for this process, found once.
    pub fn get() -> Result<&'static Self, JobError> {
        static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();
        ENGINE
            .get_or_init(|| {
                Some(Self {
                    ffmpeg: process::engine("ffmpeg")?,
                    ffprobe: process::engine("ffprobe")?,
                    encoders: OnceLock::new(),
                })
            })
            .as_ref()
            .ok_or_else(|| JobError::new(ErrorKind::EngineMissing, "FFmpeg is missing from this installation"))
    }

    fn works(&self, encoder: &str) -> bool {
        let mut command = Command::new(&self.ffmpeg);
        command
            .args([
                "-nostdin",
                "-v",
                "quiet",
                "-f",
                "lavfi",
                "-i",
                "color=black:s=256x256:r=1:d=1",
            ])
            .args([
                "-frames:v",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                encoder,
                "-f",
                "null",
                "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        command.status().is_ok_and(|s| s.success())
    }

    /// Encoders that actually run here: being compiled in is not enough.
    pub fn encoders(&self) -> Encoders {
        *self.encoders.get_or_init(|| Encoders {
            h264: H264_ENCODERS.iter().copied().find(|e| self.works(e)),
            vp9: self.works("libvpx-vp9"),
        })
    }

    /// Arguments that open `path` as an input, confined to plain media files.
    fn input(path: &Path) -> Vec<OsString> {
        let mut target = OsString::from("file:");
        target.push(path);
        let args: [&OsStr; 5] = [
            "-protocol_whitelist".as_ref(),
            "file".as_ref(),
            "-format_whitelist".as_ref(),
            FORMATS.as_ref(),
            "-i".as_ref(),
        ];
        let mut out: Vec<OsString> = args.iter().map(|a| a.to_os_string()).collect();
        out.push(target);
        out
    }

    pub fn probe(&self, ctx: &JobContext, path: &Path) -> Result<Probe, JobError> {
        let mut json = String::new();
        let mut command = Command::new(&self.ffprobe);
        command
            .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
            .args(Self::input(path));
        supervise(ctx, &mut command, |line| json.push_str(line))?;
        Probe::parse(&json)
    }

    /// Runs FFmpeg and hands every line of its standard output to `on_line`.
    pub fn run_lines(
        &self,
        ctx: &JobContext,
        log_level: &str,
        args: Vec<OsString>,
        on_line: impl FnMut(&str) + Send,
    ) -> Result<String, JobError> {
        let mut command = Command::new(&self.ffmpeg);
        command
            .args(["-nostdin", "-hide_banner", "-nostats", "-loglevel", log_level])
            .args(args);
        supervise(ctx, &mut command, on_line)
    }

    /// Runs FFmpeg. `on_time` receives the seconds of media written so far.
    /// Returns its stderr, which holds the log when `log_level` is `info`.
    pub fn run(
        &self,
        ctx: &JobContext,
        log_level: &str,
        args: Vec<OsString>,
        mut on_time: impl FnMut(f64) + Send,
    ) -> Result<String, JobError> {
        let mut command = Command::new(&self.ffmpeg);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-nostats",
                "-loglevel",
                log_level,
                "-progress",
                "pipe:1",
                "-stats_period",
                "0.1",
            ])
            .args(args);
        supervise(ctx, &mut command, |line| {
            if let Some(seconds) = parse_progress(line) {
                on_time(seconds);
            }
        })
    }
}
