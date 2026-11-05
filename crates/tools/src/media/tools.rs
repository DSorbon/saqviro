//! Video and audio tool executors. Files run one by one; each result is
//! written to the job's temp directory first and moved next to its original
//! only when FFmpeg succeeds. The first failing file fails the whole job, and
//! results already moved are removed.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};
use serde_json::Value;

use super::{
    Encoders, Engine,
    probe::{AudioStream, Probe, VideoStream, describe},
};
use crate::output::{Outputs, file_stem, input_files, name_error, render_name};

/// Lowest video bitrate worth producing, in bits per second.
const MIN_VIDEO_BITRATE: u64 = 150_000;
/// Bits per pixel per frame assumed when a file does not state its bitrate.
const ESTIMATED_BITS_PER_PIXEL: f64 = 0.1;
/// AAC or Opus audio in re-encoded video, in kbit/s.
const VIDEO_AUDIO_KBPS: u32 = 128;

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn param_str<'a>(params: &'a Params, id: &str) -> Option<&'a str> {
    params.get(id).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

fn param_u32(params: &Params, id: &str) -> Option<u32> {
    params
        .get(id)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
}

/// Reads `90`, `1:30`, `00:01:30.5` as seconds.
pub fn parse_time(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let (last, whole) = parts.split_last()?;
    let mut total = 0.0;
    for part in whole {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    let seconds = if last.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        last.parse::<f64>().ok()?
    } else {
        return None;
    };
    Some(total * 60.0 + seconds)
}

/// The part of a file a tool works on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: Option<f64>,
    pub end: Option<f64>,
}

impl Range {
    /// Reads and checks the `start` and `end` parameters.
    pub fn from_params(params: &Params, duration: Option<f64>) -> Result<Self, JobError> {
        let read = |id: &str| {
            param_str(params, id)
                .map(|text| parse_time(text).ok_or_else(|| invalid(format!("`{text}` is not a time, try 1:30"))))
                .transpose()
        };
        let range = Self {
            start: read("start")?,
            end: read("end")?,
        };
        if let (Some(start), Some(end)) = (range.start, range.end)
            && start >= end
        {
            return Err(invalid("the end must be after the start"));
        }
        if let (Some(start), Some(duration)) = (range.start, duration)
            && start >= duration
        {
            return Err(invalid("the start is past the end of the file"));
        }
        Ok(range)
    }

    /// Input options; they go before `-i`.
    fn args(self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(start) = self.start {
            args.extend(["-ss".to_owned(), format!("{start:.3}")]);
        }
        if let Some(end) = self.end {
            args.extend(["-to".to_owned(), format!("{end:.3}")]);
        }
        args
    }

    /// Length of the selection within a file of `duration` seconds.
    fn length(self, duration: Option<f64>) -> Option<f64> {
        let end = match (self.end, duration) {
            (Some(end), Some(d)) => Some(end.min(d)),
            (end, d) => end.or(d),
        };
        end.map(|end| end - self.start.unwrap_or(0.0))
    }
}

/// A video file format a result can be written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    Webm,
}

impl Container {
    fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "mp4" | "m4v" => Some(Self::Mp4),
            "mov" => Some(Self::Mov),
            "mkv" => Some(Self::Mkv),
            "webm" => Some(Self::Webm),
            _ => None,
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
        }
    }
}

/// How a video will be encoded on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPlan {
    pub container: Container,
    pub encoder: &'static str,
    pub note: Option<String>,
}

/// Picks the encoder, falling back to VP9 in WebM when this machine has no
/// H.264 encoder and the requested format needs one.
pub fn plan_video(encoders: Encoders, wanted: Container) -> Result<VideoPlan, JobError> {
    let vp9 = |container, note: Option<&str>| {
        if encoders.vp9 {
            Ok(VideoPlan {
                container,
                encoder: "libvpx-vp9",
                note: note.map(str::to_owned),
            })
        } else {
            Err(JobError::new(
                ErrorKind::EngineMissing,
                "no video encoder works on this computer",
            ))
        }
    };
    match (wanted, encoders.h264) {
        (Container::Webm, _) => vp9(Container::Webm, None),
        (container, Some(encoder)) => Ok(VideoPlan {
            container,
            encoder,
            note: None,
        }),
        (Container::Mkv, None) => vp9(Container::Mkv, None),
        (_, None) => vp9(
            Container::Webm,
            Some("This computer has no H.264 encoder, so the video was saved as WebM (VP9)."),
        ),
    }
}

fn encoder_args(encoder: &str, bit_rate: u64) -> Vec<String> {
    let mut args: Vec<String> = ["-c:v", encoder, "-b:v", &bit_rate.to_string(), "-pix_fmt", "yuv420p"]
        .map(str::to_owned)
        .to_vec();
    let extra: &[&str] = match encoder {
        "libvpx-vp9" => &["-deadline", "good", "-cpu-used", "4", "-row-mt", "1"],
        "h264_videotoolbox" => &["-allow_sw", "1"],
        _ => &[],
    };
    args.extend(extra.iter().map(|a| (*a).to_owned()));
    args
}

/// Audio for a video in `container`. With `keep`, audio the container can
/// hold is copied rather than encoded again.
fn video_audio_args(container: Container, audio: Option<&AudioStream>, keep: bool) -> Vec<String> {
    let Some(audio) = audio else {
        return vec!["-an".into()];
    };
    let (fits, encoder): (&[&str], _) = match container {
        Container::Webm => (&["opus", "vorbis"], "libopus"),
        _ => (&["aac", "mp3"], "aac"),
    };
    if keep && fits.contains(&audio.codec.as_str()) {
        return vec!["-c:a".into(), "copy".into()];
    }
    vec![
        "-c:a".into(),
        encoder.into(),
        "-b:a".into(),
        format!("{VIDEO_AUDIO_KBPS}k"),
    ]
}

/// The bitrate that keeps a video looking the same in H.264 or VP9.
fn same_quality_bit_rate(probe: &Probe, video: &VideoStream) -> u64 {
    let measured = probe.video_bit_rate().unwrap_or_else(|| {
        let pixels = f64::from(video.width) * f64::from(video.height);
        (pixels * video.fps.unwrap_or(30.0) * ESTIMATED_BITS_PER_PIXEL) as u64
    });
    // These codecs need less data than H.264 for the same picture.
    let efficient = matches!(video.codec.as_str(), "hevc" | "av1" | "vp9");
    if efficient { measured * 3 / 2 } else { measured }
}

/// Scales so the shorter side is at most `limit`, keeping sizes even. The
/// shorter side is what "720p" means, also for portrait video.
fn scale_filter(limit: u32) -> String {
    let side = |dim: &str| format!("trunc(min({dim},{limit})/2)*2");
    format!(
        "scale='if(gt(iw,ih),-2,{})':'if(gt(iw,ih),{},-2)'",
        side("iw"),
        side("ih")
    )
}

/// The bitrate after scaling to `limit`.
fn scaled_bit_rate(bit_rate: u64, video: &VideoStream, limit: u32) -> u64 {
    let short = video.width.min(video.height).max(1);
    if limit >= short {
        return bit_rate;
    }
    let ratio = f64::from(limit) / f64::from(short);
    (bit_rate as f64 * ratio * ratio) as u64
}

/// The container a result keeps, for a source with extension `ext`.
fn same_container(ext: &str) -> Container {
    Container::from_ext(ext).unwrap_or(Container::Mp4)
}

/// One audio format a result can be written in.
fn audio_args(format: &str, kbps: u32, source: &str) -> Result<(&'static str, Vec<String>), JobError> {
    let bitrate = format!("{kbps}k");
    let (ext, same, codec, lossy): (_, &[&str], _, _) = match format {
        "mp3" => ("mp3", &["mp3"], "libmp3lame", true),
        "m4a" => ("m4a", &["aac"], "aac", true),
        "wav" => ("wav", &["pcm_s16le"], "pcm_s16le", false),
        "flac" => ("flac", &["flac"], "flac", false),
        "ogg" => ("ogg", &["opus"], "libopus", true),
        _ => return Err(invalid(format!("unknown audio format `{format}`"))),
    };
    let mut args = vec!["-c:a".to_owned()];
    if same.contains(&source) {
        args.push("copy".into());
    } else {
        args.push(codec.into());
        if lossy {
            args.extend(["-b:a".into(), bitrate]);
        }
    }
    Ok((ext, args))
}

/// The audio format closest to a source extension.
fn audio_format_of(ext: &str) -> &'static str {
    match ext {
        "m4a" | "aac" => "m4a",
        "wav" | "aiff" | "aif" => "wav",
        "flac" => "flac",
        "ogg" | "opus" => "ogg",
        _ => "mp3",
    }
}

fn kbps(params: &Params) -> u32 {
    param_u32(params, "bitrate").unwrap_or(192)
}

/// One file of a batch.
struct Step<'a> {
    ctx: &'a JobContext,
    engine: &'static Engine,
    path: &'a Path,
    ext: String,
    probe: Probe,
    params: &'a Params,
    temp: PathBuf,
    /// The share of the job's progress this file covers.
    span: (f32, f32),
}

/// A finished result, still in the job's temp directory.
struct Made {
    file: PathBuf,
    ext: &'static str,
    note: Option<String>,
}

fn to_os(args: &[String]) -> impl Iterator<Item = OsString> + '_ {
    args.iter().map(OsString::from)
}

impl Step<'_> {
    fn video(&self) -> Result<&VideoStream, JobError> {
        self.probe
            .video
            .as_ref()
            .ok_or_else(|| invalid("this file has no video"))
    }

    fn audio(&self) -> Result<&AudioStream, JobError> {
        self.probe
            .audio
            .first()
            .ok_or_else(|| invalid("this file has no audio"))
    }

    /// Runs FFmpeg over part of this file's progress span.
    fn ffmpeg(
        &self,
        log_level: &str,
        before_input: &[String],
        after_input: Vec<OsString>,
        length: Option<f64>,
        span: (f32, f32),
    ) -> Result<String, JobError> {
        let mut args: Vec<OsString> = to_os(before_input).collect();
        args.extend(Engine::input(self.path));
        args.extend(after_input);
        // FFmpeg's reported time can step back between streams.
        let mut furthest = 0.0;
        self.engine.run(self.ctx, log_level, args, |seconds| {
            if let Some(length) = length.filter(|l| *l > 0.0) {
                furthest = (seconds / length).clamp(0.0, 1.0).max(furthest);
                self.ctx.progress(span.0 + (span.1 - span.0) * furthest as f32);
            }
        })
    }

    /// Writes one result with `ext`.
    fn write(
        &self,
        before_input: &[String],
        args: &[String],
        ext: &'static str,
        length: Option<f64>,
        note: Option<String>,
    ) -> Result<Made, JobError> {
        self.write_in(self.span, before_input, args, ext, length, note)
    }

    /// Like `write`, reporting progress over `span` only.
    fn write_in(
        &self,
        span: (f32, f32),
        before_input: &[String],
        args: &[String],
        ext: &'static str,
        length: Option<f64>,
        note: Option<String>,
    ) -> Result<Made, JobError> {
        let file = self.temp.join(format!("result.{ext}"));
        let mut after: Vec<OsString> = to_os(args).collect();
        if matches!(ext, "mp4" | "mov" | "m4a") {
            after.extend(["-movflags".into(), "+faststart".into()]);
        }
        after.extend(["-y".into(), file.clone().into_os_string()]);
        self.ffmpeg("error", before_input, after, length, span)?;
        Ok(Made { file, ext, note })
    }

    /// Re-encodes the video into `wanted` at `bit_rate`, through `filter`.
    fn encode_video(
        &self,
        wanted: Container,
        bit_rate: u64,
        filter: Option<&str>,
        keep_audio: bool,
        before_input: &[String],
        length: Option<f64>,
    ) -> Result<Made, JobError> {
        let video = self.video()?;
        let plan = plan_video(self.engine.encoders(), wanted)?;
        let mut args = vec!["-map".to_owned(), format!("0:{}", video.index)];
        let audio = self.probe.audio.first();
        if let Some(audio) = audio {
            args.extend(["-map".into(), format!("0:{}", audio.index)]);
        }
        args.extend(["-map_metadata".into(), "0".into()]);
        if let Some(filter) = filter {
            args.extend(["-vf".into(), filter.into()]);
        }
        args.extend(encoder_args(plan.encoder, bit_rate.max(MIN_VIDEO_BITRATE)));
        args.extend(video_audio_args(plan.container, audio, keep_audio));
        self.write(before_input, &args, plan.container.ext(), length, plan.note)
    }
}

fn file_ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Runs `make` for each input file and moves the results next to the originals.
fn batch(
    template: String,
    make: impl Fn(&Step) -> Result<Made, JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let engine = Engine::get()?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let share = 1.0 / files.len() as f32;
            let start = i as f32 * share;
            let temp = ctx.temp_dir()?.join(i.to_string());
            fs::create_dir_all(&temp)?;
            let step = Step {
                ctx,
                engine,
                path,
                ext: file_ext(path),
                probe: engine.probe(ctx, path).map_err(|e| name_error(path, e))?,
                params,
                temp,
                span: (start, start + share),
            };
            let made = make(&step).map_err(|e| name_error(path, e))?;

            let name = render_name(&template, &[("stem", &file_stem(path)), ("ext", made.ext)]);
            let dir = path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            let size = fs::metadata(&made.file)?.len();
            let written = outputs.adopt(&dir, &name, &made.file)?;
            results.push(OutputFile {
                path: written,
                size,
                source_size: fs::metadata(path).ok().map(|m| m.len()),
                note: made.note,
            });
            ctx.progress(start + share);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

/// Compress Video. `quality` is the share of the source bitrate to keep.
pub fn compress(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let share = u64::from(param_u32(step.params, "quality").unwrap_or(50).clamp(10, 90));
        let limit = param_str(step.params, "resolution").and_then(|r| r.parse::<u32>().ok());
        let mut bit_rate = step
            .probe
            .video_bit_rate()
            .unwrap_or_else(|| same_quality_bit_rate(&step.probe, video));
        bit_rate = bit_rate * share / 100;
        if let Some(limit) = limit {
            bit_rate = scaled_bit_rate(bit_rate, video, limit);
        }
        let filter = limit.map(scale_filter);
        let wanted = same_container(&step.ext);
        let made = step.encode_video(wanted, bit_rate, filter.as_deref(), false, &[], step.probe.duration)?;

        // A file that is already tight can grow when encoded again.
        let source = fs::metadata(step.path)?.len();
        if fs::metadata(&made.file)?.len() >= source && made.ext == step.ext {
            fs::copy(step.path, &made.file)?;
            let note = Some("This video is already well compressed, so it was left as it is.".to_owned());
            return Ok(Made { note, ..made });
        }
        Ok(made)
    })
}

/// Convert Video. Streams that fit the new format are copied, which is fast
/// and loses nothing.
pub fn convert(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let format = param_str(step.params, "format").unwrap_or("mp4");
        let wanted = Container::from_ext(format).ok_or_else(|| invalid(format!("unknown format `{format}`")))?;

        let fits = match wanted {
            Container::Webm => matches!(video.codec.as_str(), "vp8" | "vp9" | "av1"),
            _ => matches!(video.codec.as_str(), "h264" | "hevc"),
        };
        let audio = step.probe.audio.first();
        if fits {
            let mut args = vec!["-map".to_owned(), format!("0:{}", video.index)];
            if let Some(audio) = audio {
                args.extend(["-map".into(), format!("0:{}", audio.index)]);
            }
            args.extend(["-map_metadata".into(), "0".into(), "-c:v".into(), "copy".into()]);
            if video.codec == "hevc" && wanted != Container::Mkv {
                // Apple players need this tag to play HEVC.
                args.extend(["-tag:v".into(), "hvc1".into()]);
            }
            args.extend(video_audio_args(wanted, audio, true));
            return step.write(&[], &args, wanted.ext(), step.probe.duration, None);
        }
        let bit_rate = same_quality_bit_rate(&step.probe, video);
        step.encode_video(wanted, bit_rate, None, true, &[], step.probe.duration)
    })
}

/// Trim Video. By default streams are copied, so cuts land on keyframes;
/// `exact` re-encodes for frame-accurate cuts.
pub fn trim_video(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let range = Range::from_params(step.params, step.probe.duration)?;
        if range.start.is_none() && range.end.is_none() {
            return Err(invalid("enter a start or an end time"));
        }
        let length = range.length(step.probe.duration);
        let exact = step.params.get("exact").and_then(Value::as_bool).unwrap_or(false);
        if exact {
            let bit_rate = same_quality_bit_rate(&step.probe, video);
            let wanted = same_container(&step.ext);
            return step.encode_video(wanted, bit_rate, None, true, &range.args(), length);
        }
        let ext = copy_ext(&step.ext)?;
        let args = ["-map", "0", "-c", "copy", "-avoid_negative_ts", "make_zero"].map(str::to_owned);
        step.write(&range.args(), &args, ext, length, None)
    })
}

/// A result extension for tools that keep the source format. Only known
/// extensions reach FFmpeg, which picks the muxer from the name.
fn copy_ext(ext: &str) -> Result<&'static str, JobError> {
    // FFmpeg reads `.m4v` as a raw stream; these files are really MP4.
    if ext == "m4v" {
        return Ok("mp4");
    }
    const KNOWN: &[&str] = &[
        "mp4", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "wmv", "flv", "3gp", "ts", "mp3", "m4a", "aac", "wav",
        "flac", "ogg", "opus", "aiff", "wma",
    ];
    KNOWN.iter().find(|k| **k == ext).copied().ok_or_else(|| {
        JobError::new(
            ErrorKind::UnsupportedFormat,
            format!("`.{ext}` files are not supported"),
        )
    })
}

/// Video to GIF. Each GIF gets its own palette, which is what keeps colours
/// from banding.
pub fn to_gif(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        step.video()?;
        let range = Range::from_params(step.params, step.probe.duration)?;
        let fps = param_u32(step.params, "fps").unwrap_or(12).clamp(1, 30);
        let width = param_u32(step.params, "width").unwrap_or(480).clamp(16, 1920);
        let filter = format!(
            "fps={fps},scale='min(iw,{width})':-2:flags=lanczos,split[a][b];\
             [a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a"
        );
        let args = ["-an", "-vf", &filter, "-loop", "0"].map(str::to_owned);
        step.write(&range.args(), &args, "gif", range.length(step.probe.duration), None)
    })
}

/// Extract Audio and Convert Audio. Audio already in the wanted format is
/// copied, so it does not lose quality.
pub fn audio_to(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let audio = step.audio()?;
        let format = param_str(step.params, "format").unwrap_or("mp3");
        let (ext, codec) = audio_args(format, kbps(step.params), &audio.codec)?;
        let mut args = vec![
            "-map".to_owned(),
            format!("0:{}", audio.index),
            "-vn".into(),
            "-map_metadata".into(),
            "0".into(),
        ];
        args.extend(codec);
        step.write(&[], &args, ext, step.probe.duration, None)
    })
}

/// Mute Video. Nothing is encoded, so the picture stays identical.
pub fn mute(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let ext = copy_ext(&step.ext)?;
        let map = format!("0:{}", video.index);
        let args = ["-map", &map, "-map_metadata", "0", "-an", "-c:v", "copy"].map(str::to_owned);
        step.write(&[], &args, ext, step.probe.duration, None)
    })
}

/// Resize Video. Never enlarges.
pub fn resize(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let limit = param_str(step.params, "resolution")
            .and_then(|r| r.parse::<u32>().ok())
            .unwrap_or(720);
        let bit_rate = scaled_bit_rate(same_quality_bit_rate(&step.probe, video), video, limit);
        let filter = scale_filter(limit);
        let wanted = same_container(&step.ext);
        step.encode_video(wanted, bit_rate, Some(&filter), true, &[], step.probe.duration)
    })
}

/// Rotate Video.
pub fn rotate(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let video = step.video()?;
        let filter = match param_str(step.params, "angle").unwrap_or("90") {
            "90" => "transpose=1",
            "180" => "hflip,vflip",
            "270" => "transpose=2",
            other => return Err(invalid(format!("unknown angle `{other}`"))),
        };
        let bit_rate = same_quality_bit_rate(&step.probe, video);
        let wanted = same_container(&step.ext);
        step.encode_video(wanted, bit_rate, Some(filter), true, &[], step.probe.duration)
    })
}

/// Trim Audio. Most lossy audio is copied, so it is not degraded again.
/// Lossless audio is encoded with its own codec: that loses nothing, cuts
/// exactly and gives FLAC a correct length in its header. Ogg keeps the
/// timestamps of the original when copied, so players report the wrong
/// length, and it is encoded too.
pub fn trim_audio(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let audio = step.audio()?;
        let range = Range::from_params(step.params, step.probe.duration)?;
        if range.start.is_none() && range.end.is_none() {
            return Err(invalid("enter a start or an end time"));
        }
        let ext = copy_ext(&step.ext)?;
        let lossless = audio.codec == "flac" || audio.codec.starts_with("pcm_");
        let kbps = audio.bit_rate.map_or(160, |b| (b / 1000) as u32).clamp(64, 320);
        let codec: Vec<String> = match (lossless, ext) {
            (true, _) => vec!["-c:a".into(), audio.codec.clone()],
            (false, "ogg" | "opus") => vec!["-c:a".into(), "libopus".into(), "-b:a".into(), format!("{kbps}k")],
            (false, _) => vec!["-c:a".into(), "copy".into()],
        };
        let map = format!("0:{}", audio.index);
        let mut args = ["-map", &map, "-map_metadata", "0", "-vn"].map(str::to_owned).to_vec();
        args.extend(codec);
        args.extend(["-avoid_negative_ts".into(), "make_zero".into()]);
        step.write(&range.args(), &args, ext, range.length(step.probe.duration), None)
    })
}

/// What the first loudnorm pass measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    pub integrated: f64,
    pub true_peak: f64,
    pub range: f64,
    pub threshold: f64,
    pub offset: f64,
}

/// Reads the JSON report loudnorm prints at the end of its log.
pub fn parse_loudness(log: &str) -> Result<Loudness, JobError> {
    let unreadable = || JobError::new(ErrorKind::ProcessFailed, "could not measure the loudness");
    let start = log.rfind('{').ok_or_else(unreadable)?;
    let end = log.rfind('}').filter(|e| *e > start).ok_or_else(unreadable)?;
    let report: Value = serde_json::from_str(&log[start..=end]).map_err(|_| unreadable())?;
    let value = |key: &str| {
        report[key]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .ok_or_else(|| invalid("this file is silent, so there is nothing to normalize"))
    };
    Ok(Loudness {
        integrated: value("input_i")?,
        true_peak: value("input_tp")?,
        range: value("input_lra")?,
        threshold: value("input_thresh")?,
        offset: value("target_offset")?,
    })
}

/// Normalize Audio: measures the file, then applies one constant gain, so
/// the dynamics of the recording stay as they were.
pub fn normalize(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let audio = step.audio()?;
        let target: f64 = param_str(step.params, "target")
            .and_then(|t| t.parse().ok())
            .unwrap_or(-16.0);
        let shape = format!("I={target}:TP=-1.5:LRA=11");
        let map = format!("0:{}", audio.index);

        let middle = (step.span.0 + step.span.1) / 2.0;
        let measure = format!("loudnorm={shape}:print_format=json");
        let args = ["-map", &map, "-vn", "-af", &measure, "-f", "null", "-"].map(OsString::from);
        let log = step.ffmpeg("info", &[], args.to_vec(), step.probe.duration, (step.span.0, middle))?;
        let l = parse_loudness(&log)?;

        let apply = format!(
            "loudnorm={shape}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}:linear=true",
            l.integrated, l.true_peak, l.range, l.threshold, l.offset
        );
        let format = audio_format_of(&step.ext);
        let kbps = audio.bit_rate.map_or(192, |b| (b / 1000) as u32).clamp(96, 320);
        let (ext, codec) = audio_args(format, kbps, "")?;
        let mut args = vec!["-map".to_owned(), map, "-map_metadata".into(), "0".into(), "-vn".into()];
        args.extend(["-af".into(), apply]);
        // loudnorm works at 192 kHz; go back to the rate of the source.
        args.extend(["-ar".into(), audio.sample_rate.unwrap_or(48_000).to_string()]);
        args.extend(codec);
        step.write_in((middle, step.span.1), &[], &args, ext, step.probe.duration, None)
    })
}

/// Video Info and Audio Info.
pub fn info() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let engine = Engine::get()?;
        let mut sections = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let probe = engine.probe(ctx, path).map_err(|e| name_error(path, e))?;
            sections.push(describe(path, &probe));
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        Ok(JobOutput::Text(sections.join("\n\n")))
    })
}
