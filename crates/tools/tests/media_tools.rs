//! Runs every video and audio tool through a real FFmpeg, on media generated
//! here so fixtures stay license-clean.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, ErrorKind, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, OutputFile};
use serde_json::{Value, json};

/// Linux has no system H.264 encoder here, so results fall back to VP9.
const VIDEO_EXT: &str = if cfg!(target_os = "linux") { "webm" } else { "mp4" };

fn registry() -> Registry {
    fn collect(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.file_name().is_some_and(|n| n == "tool.yaml") {
                out.push((path.display().to_string(), fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    collect(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools"), &mut files);
    Registry::from_sources(files.iter().map(|(p, s)| (p.as_str(), s.as_str()))).unwrap()
}

/// Runs a tool the way the app does: manifest → validated params → executor → job.
fn run(tool_id: &str, files: &[PathBuf], params: Value) -> (JobSnapshot, Vec<f32>) {
    let registry = registry();
    let tool = registry.get(tool_id).unwrap();
    let params = tool.resolve_params(params.as_object().unwrap()).unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let root = tempfile::tempdir().unwrap();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let seen = progress.clone();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, move |s: &JobSnapshot| seen.lock().unwrap().push(s.progress)).unwrap();
    let done = jobs
        .wait(jobs.submit(tool_id, exec, JobInput::Files(files.to_vec()), params))
        .unwrap();
    let progress = progress.lock().unwrap().clone();
    (done, progress)
}

fn one(done: &JobSnapshot) -> OutputFile {
    match &done.output {
        Some(JobOutput::Files(files)) if files.len() == 1 => files[0].clone(),
        other => panic!("expected one file, got {other:?} / {:?}", done.error),
    }
}

fn name(f: &OutputFile) -> String {
    f.path.file_name().unwrap().to_string_lossy().into_owned()
}

fn ffmpeg(args: &[&str], out: &Path) {
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y"])
        .args(args)
        .arg(out)
        .status()
        .expect("ffmpeg must be installed to run these tests");
    assert!(status.success(), "could not make {}", out.display());
}

/// A video with moving detail and a tone. MPEG-4 Part 2 needs no extra
/// libraries, so this works with every FFmpeg build.
fn video(dir: &Path, file: &str, size: &str, seconds: u32) -> PathBuf {
    let path = dir.join(file);
    let picture = format!("testsrc2=s={size}:r=25:d={seconds}");
    let tone = format!("sine=f=440:d={seconds}");
    ffmpeg(
        &[
            "-f", "lavfi", "-i", &picture, "-f", "lavfi", "-i", &tone, "-c:v", "mpeg4", "-b:v", "4M", "-c:a", "aac",
        ],
        &path,
    );
    path
}

fn silent_video(dir: &Path, file: &str) -> PathBuf {
    let path = dir.join(file);
    ffmpeg(
        &["-f", "lavfi", "-i", "testsrc2=s=320x240:r=25:d=1", "-c:v", "mpeg4"],
        &path,
    );
    path
}

fn audio(dir: &Path, file: &str, seconds: u32) -> PathBuf {
    let path = dir.join(file);
    let tone = format!("sine=f=440:d={seconds}");
    ffmpeg(&["-f", "lavfi", "-i", &tone, "-af", "volume=0.05"], &path);
    path
}

/// Stream facts straight from ffprobe, independent of the code under test.
fn streams(path: &Path) -> Value {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{} is unreadable", path.display());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn stream(path: &Path, kind: &str) -> Option<Value> {
    let info = streams(path);
    info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
        .cloned()
}

fn duration(path: &Path) -> f64 {
    streams(path)["format"]["duration"].as_str().unwrap().parse().unwrap()
}

fn size_of(path: &Path) -> (u64, u64) {
    let v = stream(path, "video").unwrap();
    (v["width"].as_u64().unwrap(), v["height"].as_u64().unwrap())
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn compress_makes_a_video_smaller_and_keeps_its_sound() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "640x480", 2);
    let before = fs::read(&src).unwrap();

    let (done, progress) = run("video.compress", std::slice::from_ref(&src), json!({ "quality": 30 }));
    let out = one(&done);

    assert!(
        out.size < out.source_size.unwrap(),
        "{} → {}",
        out.source_size.unwrap(),
        out.size
    );
    assert!(stream(&out.path, "audio").is_some());
    assert_eq!(fs::read(&src).unwrap(), before, "the original changed");
    assert!(
        progress.windows(2).all(|w| w[0] <= w[1]),
        "progress went back: {progress:?}"
    );
}

#[test]
fn reports_progress_while_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mkv", "1280x720", 3);
    let (done, progress) = run("video.convert", &[src], json!({ "format": "webm" }));
    assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
    let between = progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count();
    assert!(between >= 2, "{progress:?}");
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn compress_writes_h264_in_mp4_on_macos() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "640x480", 1);
    let (done, _) = run("video.compress", &[src], json!({}));
    let out = one(&done);
    assert_eq!(name(&out), "clip-compressed.mp4");
    assert_eq!(stream(&out.path, "video").unwrap()["codec_name"], "h264");
}

#[test]
fn compress_lowers_the_resolution_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "wide.mkv", "1280x720", 1);
    let (done, _) = run("video.compress", &[src], json!({ "resolution": "480" }));
    assert_eq!(size_of(&one(&done).path), (854, 480));
}

#[test]
fn compress_never_returns_a_bigger_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("tiny.mkv");
    ffmpeg(
        &["-f", "lavfi", "-i", "color=black:s=64x64:r=5:d=1", "-c:v", "mpeg4"],
        &src,
    );

    let (done, _) = run("video.compress", std::slice::from_ref(&src), json!({}));
    let out = one(&done);

    assert!(out.size <= out.source_size.unwrap());
    // VP9 can beat the source even here; H.264 from the system cannot.
    if let Some(note) = out.note {
        assert!(note.contains("left as it is"));
        assert_eq!(fs::read(&out.path).unwrap(), fs::read(&src).unwrap());
    }
}

#[test]
fn convert_encodes_when_the_codec_does_not_fit() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let (done, _) = run("video.convert", &[src], json!({ "format": "webm" }));
    let out = one(&done);
    assert_eq!(name(&out), "clip.webm");
    assert_eq!(stream(&out.path, "video").unwrap()["codec_name"], "vp9");
    assert_eq!(stream(&out.path, "audio").unwrap()["codec_name"], "opus");
}

#[test]
fn convert_copies_streams_that_fit() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.webm");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x240:r=25:d=1",
            "-c:v",
            "libvpx-vp9",
            "-b:v",
            "300k",
        ],
        &src,
    );
    let source = stream(&src, "video").unwrap();

    let mkv = dir.path().join("first.mkv");
    ffmpeg(&["-i", src.to_str().unwrap(), "-c", "copy"], &mkv);
    let (done, _) = run("video.convert", &[mkv], json!({ "format": "webm" }));
    let out = one(&done);

    assert_eq!(name(&out), "first.webm");
    let video = stream(&out.path, "video").unwrap();
    assert_eq!(video["codec_name"], source["codec_name"]);
    assert_eq!(video["width"], source["width"]);
}

#[test]
fn trim_keeps_the_selected_part() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 4);
    let (done, _) = run(
        "video.trim",
        &[src],
        json!({ "start": "0:01", "end": "0:03", "exact": true }),
    );
    let out = one(&done);
    assert_eq!(name(&out)[..12].to_owned(), "clip-trimmed");
    let length = duration(&out.path);
    assert!((length - 2.0).abs() < 0.2, "{length}");
}

#[test]
fn fast_trim_does_not_encode() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 3);
    let (done, _) = run("video.trim", &[src], json!({ "end": "2" }));
    let out = one(&done);
    assert_eq!(stream(&out.path, "video").unwrap()["codec_name"], "mpeg4");
    assert!(duration(&out.path) < 2.6);
}

#[test]
fn trim_needs_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let (done, _) = run("video.trim", &[src], json!({}));
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
    assert_eq!(entries(dir.path()), ["clip.mp4"]);
}

#[test]
fn makes_a_gif_no_wider_than_asked() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "640x480", 1);
    let (done, _) = run("video.to-gif", &[src], json!({ "width": 200, "fps": 5 }));
    let out = one(&done);
    assert_eq!(name(&out), "clip.gif");
    assert_eq!(stream(&out.path, "video").unwrap()["codec_name"], "gif");
    assert_eq!(size_of(&out.path).0, 200);
}

#[test]
fn a_gif_does_not_enlarge_a_small_video() {
    let dir = tempfile::tempdir().unwrap();
    let src = silent_video(dir.path(), "small.mp4");
    let (done, _) = run("video.to-gif", &[src], json!({ "width": 640 }));
    assert_eq!(size_of(&one(&done).path).0, 320);
}

#[test]
fn extracts_audio_as_mp3() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let (done, _) = run("video.extract-audio", &[src], json!({ "format": "mp3" }));
    let out = one(&done);
    assert_eq!(name(&out), "clip.mp3");
    assert_eq!(stream(&out.path, "audio").unwrap()["codec_name"], "mp3");
    assert!(stream(&out.path, "video").is_none());
}

#[test]
fn extracting_audio_in_its_own_format_copies_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let source = stream(&src, "audio").unwrap();
    let (done, _) = run("video.extract-audio", &[src], json!({ "format": "m4a", "bitrate": 64 }));
    let audio = stream(&one(&done).path, "audio").unwrap();
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["bit_rate"], source["bit_rate"], "a copy keeps its bitrate");
}

#[test]
fn a_video_without_sound_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let src = silent_video(dir.path(), "quiet.mp4");
    let (done, _) = run("video.extract-audio", &[src], json!({}));
    let e = done.error.unwrap();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    assert_eq!(e.message, "quiet.mp4: this file has no audio");
}

#[test]
fn mute_removes_sound_and_keeps_the_picture() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let source = stream(&src, "video").unwrap();
    let (done, _) = run("video.mute", &[src], json!({}));
    let out = one(&done);
    assert_eq!(name(&out), "clip-muted.mp4");
    assert!(stream(&out.path, "audio").is_none());
    let video = stream(&out.path, "video").unwrap();
    assert_eq!(
        (&video["codec_name"], &video["bit_rate"]),
        (&source["codec_name"], &source["bit_rate"])
    );
}

#[test]
fn m4v_results_are_real_mp4_files() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("clip.m4v");
    fs::rename(video(dir.path(), "clip.mp4", "320x240", 1), &src).unwrap();
    let (done, _) = run("video.mute", &[src], json!({}));
    let out = one(&done);
    assert_eq!(name(&out), "clip-muted.mp4");
    assert!(
        streams(&out.path)["format"]["format_name"]
            .as_str()
            .unwrap()
            .contains("mp4")
    );
}

#[test]
fn resize_limits_the_short_side() {
    let dir = tempfile::tempdir().unwrap();
    let wide = video(dir.path(), "wide.mp4", "1280x720", 1);
    let tall = video(dir.path(), "tall.mp4", "720x1280", 1);

    let (done, _) = run("video.resize", &[wide], json!({ "resolution": "480" }));
    assert_eq!(size_of(&one(&done).path), (854, 480));
    let (done, _) = run("video.resize", &[tall], json!({ "resolution": "480" }));
    assert_eq!(size_of(&one(&done).path), (480, 854));
}

#[test]
fn resize_never_enlarges() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "small.mp4", "320x240", 1);
    let (done, _) = run("video.resize", &[src], json!({ "resolution": "1080" }));
    assert_eq!(size_of(&one(&done).path), (320, 240));
}

#[test]
fn rotates_a_quarter_turn() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    let (done, _) = run("video.rotate", &[src], json!({ "angle": "270" }));
    let out = one(&done);
    assert!(name(&out).starts_with("clip-rotated."), "{}", name(&out));
    assert_eq!(size_of(&out.path), (240, 320));
}

#[test]
fn info_describes_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let a = video(dir.path(), "a.mp4", "320x240", 1);
    let b = video(dir.path(), "b.mkv", "640x480", 1);
    let (done, _) = run("video.info", &[a, b], json!({}));
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error);
    };
    for part in ["a.mp4", "320×240", "b.mkv", "640×480", "aac"] {
        assert!(text.contains(part), "{part} missing from:\n{text}");
    }
    assert_eq!(entries(dir.path()), ["a.mp4", "b.mkv"], "info must create no files");
}

#[test]
fn converts_audio_between_formats() {
    let dir = tempfile::tempdir().unwrap();
    let src = audio(dir.path(), "tone.wav", 1);
    for (format, codec) in [("mp3", "mp3"), ("m4a", "aac"), ("flac", "flac"), ("ogg", "opus")] {
        let (done, _) = run("audio.convert", std::slice::from_ref(&src), json!({ "format": format }));
        let out = one(&done);
        assert_eq!(name(&out), format!("tone.{format}"));
        assert_eq!(stream(&out.path, "audio").unwrap()["codec_name"], codec);
    }
}

/// The length a player gets by decoding the file, whatever its header says.
fn decoded_length(path: &Path) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    let time = log.rsplit("time=").next().unwrap().split_whitespace().next().unwrap();
    saqviro_tools::media::tools::parse_time(time).unwrap()
}

#[test]
fn trims_audio_in_every_format() {
    let dir = tempfile::tempdir().unwrap();
    for ext in ["flac", "wav", "aiff", "mp3", "m4a", "ogg", "opus", "aac"] {
        let src = audio(dir.path(), &format!("tone.{ext}"), 4);
        let (done, _) = run("audio.trim", &[src], json!({ "start": "1", "end": "2.5" }));
        let out = one(&done);
        assert_eq!(name(&out), format!("tone-trimmed.{ext}"));
        let (header, decoded) = (duration(&out.path), decoded_length(&out.path));
        assert!((header - 1.5).abs() < 0.1, "{ext}: header says {header}");
        assert!((decoded - 1.5).abs() < 0.1, "{ext}: plays for {decoded}");
    }
}

#[test]
fn trimming_keeps_lossless_audio_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("deep.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=d=2", "-c:a", "pcm_s24le"], &src);
    let (done, _) = run("audio.trim", &[src], json!({ "start": "1" }));
    assert_eq!(stream(&one(&done).path, "audio").unwrap()["codec_name"], "pcm_s24le");
}

#[test]
fn audio_info_works_on_audio() {
    let dir = tempfile::tempdir().unwrap();
    let src = audio(dir.path(), "tone.wav", 1);
    let (done, _) = run("audio.info", &[src], json!({}));
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error);
    };
    assert!(text.contains("pcm_s16le") && text.contains("Audio: "), "{text}");
    assert!(!text.contains("Video: "), "{text}");
}

fn loudness(path: &Path) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-hide_banner", "-i"])
        .arg(path)
        .args(["-af", "ebur128=framelog=quiet", "-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    let line = log.lines().rev().find(|l| l.trim_start().starts_with("I:")).unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn normalize_reaches_the_target_loudness() {
    let dir = tempfile::tempdir().unwrap();
    let src = audio(dir.path(), "quiet.wav", 6);
    let before = loudness(&src);

    let (done, progress) = run(
        "audio.normalize",
        std::slice::from_ref(&src),
        json!({ "target": "-16" }),
    );
    let out = one(&done);

    assert_eq!(name(&out), "quiet-normalized.wav");
    let after = loudness(&out.path);
    assert!((after + 16.0).abs() < 1.5, "{before} → {after}");
    let audio = stream(&out.path, "audio").unwrap();
    assert_eq!(audio["sample_rate"], stream(&src, "audio").unwrap()["sample_rate"]);
    assert!(
        progress.windows(2).all(|w| w[0] <= w[1]),
        "progress went back: {progress:?}"
    );
}

#[test]
fn normalize_refuses_silence() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("silence.wav");
    ffmpeg(&["-f", "lavfi", "-i", "anullsrc=d=2"], &src);
    let (done, _) = run("audio.normalize", &[src], json!({}));
    assert!(done.error.unwrap().message.contains("silent"));
    assert_eq!(entries(dir.path()), ["silence.wav"]);
}

#[test]
fn never_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "clip.mp4", "320x240", 1);
    fs::write(dir.path().join("clip-muted.mp4"), "mine").unwrap();
    let (done, _) = run("video.mute", &[src], json!({}));
    assert_eq!(name(&one(&done)), "clip-muted (2).mp4");
    assert_eq!(fs::read(dir.path().join("clip-muted.mp4")).unwrap(), b"mine");
}

#[test]
fn file_names_are_never_interpreted() {
    let dir = tempfile::tempdir().unwrap();
    let mut names = vec!["-i evil.mp4", "100%d {stem}.mp4", "пример видео.mp4"];
    if cfg!(unix) {
        names.push("concat:a.mp4");
    }
    for file in names {
        let src = video(dir.path(), "plain.mp4", "160x120", 1);
        let tricky = dir.path().join(file);
        fs::rename(src, &tricky).unwrap();
        let (done, _) = run("video.mute", &[tricky], json!({}));
        assert_eq!(done.status, JobStatus::Completed, "{file}: {:?}", done.error);
    }
}

#[test]
fn a_file_that_points_at_other_files_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("secret.mp4");
    fs::copy(video(dir.path(), "a.mp4", "160x120", 1), &secret).unwrap();
    let evil = dir.path().join("evil.mkv");
    fs::write(&evil, format!("ffconcat version 1.0\nfile '{}'\n", secret.display())).unwrap();

    let (done, _) = run("video.mute", &[evil], json!({}));

    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::UnsupportedFormat);
    assert_eq!(entries(dir.path()), ["a.mp4", "evil.mkv", "secret.mp4"]);
}

#[test]
fn a_damaged_file_gives_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("broken.mp4");
    fs::write(&src, b"this is not a video").unwrap();
    let (done, _) = run("video.compress", &[src], json!({}));
    let e = done.error.unwrap();
    assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
    assert!(e.message.starts_with("broken.mp4: "), "{}", e.message);
}

#[test]
fn one_bad_file_removes_the_results_of_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let good = video(dir.path(), "good.mp4", "320x240", 1);
    let bad = dir.path().join("bad.mp4");
    fs::write(&bad, b"nope").unwrap();

    let (done, _) = run("video.mute", &[good, bad], json!({}));

    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(entries(dir.path()), ["bad.mp4", "good.mp4"]);
}

#[test]
fn cancelling_stops_ffmpeg_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let src = video(dir.path(), "long.mkv", "1280x720", 20);

    let registry = registry();
    let tool = registry.get("video.convert").unwrap();
    let params = tool
        .resolve_params(json!({ "format": "webm" }).as_object().unwrap())
        .unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let started = Arc::new(Mutex::new(false));
    let seen = started.clone();
    let jobs = JobManager::new(config, move |s: &JobSnapshot| {
        if s.progress > 0.0 {
            *seen.lock().unwrap() = true;
        }
    })
    .unwrap();
    let id = jobs.submit("video.convert", exec, JobInput::Files(vec![src]), params);

    let deadline = Instant::now() + Duration::from_secs(60);
    while !*started.lock().unwrap() {
        assert!(Instant::now() < deadline, "the job never reported progress");
        std::thread::sleep(Duration::from_millis(10));
    }
    let asked = Instant::now();
    jobs.cancel(id);
    let done = jobs.wait(id).unwrap();

    assert_eq!(done.status, JobStatus::Cancelled);
    assert!(asked.elapsed() < Duration::from_secs(2), "took {:?}", asked.elapsed());
    assert_eq!(entries(dir.path()), ["long.mkv"]);
}

#[test]
fn crops_video_to_a_shape() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "320x180", 1);
    let (done, _) = run("video.crop", &[source], json!({ "aspect": "1:1" }));
    let out = one(&done);
    assert_eq!(name(&out), format!("a-cropped.{VIDEO_EXT}"));
    assert_eq!(size_of(&out.path), (180, 180));
    assert!(stream(&out.path, "audio").is_some());
}

#[test]
fn crops_video_to_an_area_and_stops_at_the_edge() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "320x180", 1);
    let params = json!({ "aspect": "area", "x": 300, "y": 100, "width": 999, "height": 50 });
    let (done, _) = run("video.crop", std::slice::from_ref(&source), params);
    let out = one(&done);
    assert_eq!(size_of(&out.path), (20, 50));
    if cfg!(windows) {
        assert_eq!(name(&out), "a-cropped.webm");
        assert!(out.note.is_some());
    }

    let (done, _) = run("video.crop", &[source], json!({ "aspect": "area", "x": 320 }));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn changes_speed_of_picture_and_sound() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 2);
    let (done, _) = run("video.speed", std::slice::from_ref(&source), json!({ "speed": "2" }));
    let out = one(&done);
    assert_eq!(name(&out), format!("a-2x.{VIDEO_EXT}"));
    assert!((duration(&out.path) - 1.0).abs() < 0.2, "{}", duration(&out.path));
    assert!(stream(&out.path, "audio").is_some());

    // Beyond what one `atempo` can do.
    let (done, _) = run("video.speed", &[source], json!({ "speed": "0.25" }));
    assert!((duration(&one(&done).path) - 8.0).abs() < 0.3);
}

#[test]
fn changes_frame_rate() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 1);
    let (done, _) = run("video.fps", &[source], json!({ "fps": "15" }));
    let out = one(&done);
    assert_eq!(name(&out), format!("a-15fps.{VIDEO_EXT}"));
    assert_eq!(stream(&out.path, "video").unwrap()["avg_frame_rate"], "15/1");
}

#[test]
fn saves_a_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 2);
    let (done, _) = run(
        "video.thumbnail",
        std::slice::from_ref(&source),
        json!({ "format": "png", "time": "1" }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a-thumbnail.png");
    assert_eq!(image::image_dimensions(&out.path).unwrap(), (160, 120));

    let (done, _) = run("video.thumbnail", std::slice::from_ref(&source), json!({}));
    assert_eq!(name(&one(&done)), "a-thumbnail.jpg");

    let (done, _) = run("video.thumbnail", &[source], json!({ "time": "9" }));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn extracts_frames_into_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 3);
    let (done, _) = run(
        "video.frames",
        &[source],
        json!({ "every_seconds": 1.0, "format": "png" }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a-frames");
    let count = fs::read_dir(&out.path).unwrap().count();
    assert!((3..=4).contains(&count), "{count} frames");
    assert!(out.path.join("frame-00001.png").exists());
    assert!(out.size > 0);
}

#[test]
fn moves_the_index_to_the_front() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 1);
    let (done, _) = run("video.faststart", &[source], json!({}));
    let out = one(&done);
    let bytes = fs::read(&out.path).unwrap();
    let at = |tag: &[u8]| bytes.windows(4).position(|w| w == tag).unwrap();
    assert!(at(b"moov") < at(b"mdat"));
}

#[test]
fn turns_a_gif_into_video() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.gif");
    ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=161x121:r=10:d=1"], &source);
    let (done, _) = run("video.from-gif", &[source], json!({}));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(size_of(&out.path), (160, 120));
}

#[test]
fn merges_videos_of_different_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let a = video(dir.path(), "a.mp4", "320x240", 1);
    let b = video(dir.path(), "b.mov", "160x160", 1);
    let c = silent_video(dir.path(), "c.mp4");
    let (done, _) = run("video.merge", &[a, b, c], json!({}));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(name(&out), format!("a-merged.{VIDEO_EXT}"));
    assert_eq!(size_of(&out.path), (320, 240));
    assert!(stream(&out.path, "audio").is_some());
    assert!((duration(&out.path) - 3.0).abs() < 0.3, "{}", duration(&out.path));
}

#[test]
fn merging_needs_two_videos() {
    let dir = tempfile::tempdir().unwrap();
    let a = video(dir.path(), "a.mp4", "160x120", 1);
    let (done, _) = run("video.merge", std::slice::from_ref(&a), json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);

    let sound = audio(dir.path(), "s.mp3", 1);
    let (done, _) = run("video.merge", &[a, sound], json!({}));
    assert!(done.error.unwrap().message.contains("s.mp3"));
}

#[test]
fn replaces_the_sound_of_a_video() {
    let dir = tempfile::tempdir().unwrap();
    let clip = video(dir.path(), "clip.mp4", "160x120", 2);
    let sound = audio(dir.path(), "song.wav", 1);
    // The order of the two files does not matter.
    let (done, _) = run("video.replace-audio", &[sound, clip], json!({}));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(name(&out), "clip-new-audio.mp4");
    assert_eq!(stream(&out.path, "audio").unwrap()["codec_name"], "aac");
    assert!(duration(&out.path) < 1.6, "{}", duration(&out.path));
}

#[test]
fn replacing_audio_needs_a_video_and_a_sound() {
    let dir = tempfile::tempdir().unwrap();
    let a = audio(dir.path(), "a.mp3", 1);
    let b = audio(dir.path(), "b.mp3", 1);
    let (done, _) = run("video.replace-audio", &[a.clone(), b], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
    let (done, _) = run("video.replace-audio", &[a], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn changes_volume() {
    let dir = tempfile::tempdir().unwrap();
    let source = audio(dir.path(), "a.wav", 1);
    let (done, _) = run("audio.volume", std::slice::from_ref(&source), json!({ "gain": 12 }));
    let out = one(&done);
    assert_eq!(name(&out), "a-volume.wav");
    let peak = |p: &Path| {
        let bytes = fs::read(p).unwrap();
        bytes[100..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c).unsigned_abs())
            .max()
            .unwrap()
    };
    let gain = f64::from(peak(&out.path)) / f64::from(peak(&source));
    assert!((3.5..4.5).contains(&gain), "{gain}");
}

#[test]
fn mixes_down_and_resamples() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=d=1", "-ac", "2", "-ar", "44100"], &source);

    let (done, _) = run("audio.to-mono", std::slice::from_ref(&source), json!({}));
    assert_eq!(stream(&one(&done).path, "audio").unwrap()["channels"], 1);

    let (done, _) = run("audio.sample-rate", &[source], json!({ "rate": "16000" }));
    let out = one(&done);
    assert_eq!(name(&out), "a-16000hz.wav");
    assert_eq!(stream(&out.path, "audio").unwrap()["sample_rate"], "16000");
}

/// One second of tone, two of silence, one of tone.
fn gappy(dir: &Path) -> PathBuf {
    let path = dir.join("gap.wav");
    let graph = "sine=d=1[a];anullsrc=r=44100:cl=mono:d=2[b];sine=d=1[c];[a][b][c]concat=n=3:v=0:a=1";
    ffmpeg(&["-f", "lavfi", "-i", graph], &path);
    path
}

#[test]
fn removes_silence() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run("audio.remove-silence", &[gappy(dir.path())], json!({}));
    let length = duration(&one(&done).path);
    assert!(length < 3.0, "{length}");
}

#[test]
fn lists_silence() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run("audio.detect-silence", &[gappy(dir.path())], json!({}));
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error)
    };
    assert!(text.starts_with("gap.wav\n0:00:01"), "{text}");
    assert!(text.contains("(2.0 s)"), "{text}");
}

#[test]
fn says_when_there_is_no_silence() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=d=1"], &source);
    let (done, _) = run("audio.detect-silence", &[source], json!({}));
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error)
    };
    assert!(text.contains("No silence found"), "{text}");
}

#[test]
fn reads_the_silence_log() {
    use saqviro_tools::media::tools::parse_silence;
    let log = "lavfi.silence_start=1.5\nlavfi.silence_end=3\nlavfi.silence_duration=1.5\nlavfi.silence_start=9\n";
    assert_eq!(parse_silence(log, Some(10.0)), [(1.5, 3.0), (9.0, 10.0)]);
    assert_eq!(parse_silence(log, None), [(1.5, 3.0)]);
    assert_eq!(parse_silence("", Some(1.0)), []);
}

#[test]
fn draws_sound() {
    let dir = tempfile::tempdir().unwrap();
    let source = audio(dir.path(), "a.mp3", 1);
    let (done, _) = run(
        "audio.waveform",
        std::slice::from_ref(&source),
        json!({ "width": 400, "height": 100 }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a-waveform.png");
    assert_eq!(image::image_dimensions(&out.path).unwrap(), (400, 100));

    let (done, _) = run("audio.spectrogram", &[source], json!({ "width": 300, "height": 200 }));
    assert_eq!(image::image_dimensions(&one(&done).path).unwrap(), (300, 200));
}

#[test]
fn merges_audio_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let a = audio(dir.path(), "a.mp3", 1);
    let b = audio(dir.path(), "b.wav", 2);
    let (done, _) = run("audio.merge", &[a, b], json!({ "format": "flac" }));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(name(&out), "a-merged.flac");
    assert!((duration(&out.path) - 3.0).abs() < 0.2, "{}", duration(&out.path));
}

#[test]
fn a_tool_for_sound_refuses_silent_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = silent_video(dir.path(), "a.mp4");
    let (done, _) = run("audio.volume", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn prints_text_on_a_video() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "320x240", 1);
    let (done, _) = run("video.watermark", &[source], json!({ "text": "Saqviro", "size": 20 }));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(name(&out), format!("a-watermarked.{VIDEO_EXT}"));
    assert_eq!(size_of(&out.path), (320, 240));
    assert!(stream(&out.path, "audio").is_some());
}

#[test]
fn saves_the_subtitles_of_a_video() {
    let dir = tempfile::tempdir().unwrap();
    let subs = dir.path().join("s.srt");
    fs::write(&subs, "1\n00:00:00,000 --> 00:00:01,000\nHello\n").unwrap();
    let source = dir.path().join("a.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=160x120:r=10:d=1",
            "-i",
            subs.to_str().unwrap(),
            "-c:v",
            "mpeg4",
            "-c:s",
            "srt",
        ],
        &source,
    );
    let (done, _) = run(
        "video.subtitles",
        std::slice::from_ref(&source),
        json!({ "format": "srt" }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a.srt");
    assert!(fs::read_to_string(&out.path).unwrap().contains("Hello"));

    let (done, _) = run("video.subtitles", &[source], json!({ "track": 2 }));
    assert!(done.error.unwrap().message.contains("has 1 subtitle tracks"));

    let plain = video(dir.path(), "b.mp4", "160x120", 1);
    let (done, _) = run("video.subtitles", &[plain], json!({}));
    assert!(done.error.unwrap().message.contains("no subtitles"));
}

#[test]
fn sets_audio_tags_without_touching_the_sound() {
    let dir = tempfile::tempdir().unwrap();
    let source = audio(dir.path(), "a.mp3", 1);
    let (done, _) = run(
        "audio.tags",
        std::slice::from_ref(&source),
        json!({ "title": "Песня", "artist": "Ann" }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a-tagged.mp3");
    let info = streams(&out.path);
    assert_eq!(info["format"]["tags"]["title"], "Песня");
    assert_eq!(info["format"]["tags"]["artist"], "Ann");

    let (done, _) = run("audio.tags", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn lists_every_silence_of_a_long_recording() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long.wav");
    // One second on, one second off, 300 times.
    let tone = "sine=d=600,volume='if(lt(mod(t,2),1),1,0)':eval=frame";
    ffmpeg(&["-f", "lavfi", "-i", tone], &source);
    let (done, _) = run("audio.detect-silence", &[source], json!({ "min_seconds": 0.5 }));
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error)
    };
    assert!(text.lines().count() >= 295, "{} lines", text.lines().count());
    assert!(text.lines().nth(1).unwrap().starts_with("0:00:0"), "{text}");
}

#[test]
fn a_watermark_lands_on_a_video_turned_by_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let plain = video(dir.path(), "plain.mp4", "320x180", 1);
    let turned = dir.path().join("turned.mp4");
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-display_rotation", "90", "-i"])
        .arg(&plain)
        .args(["-c", "copy"])
        .arg(&turned)
        .status()
        .unwrap();
    assert!(status.success());
    let params = json!({ "text": "HHHH", "size": 30, "color": "#ff00ff", "opacity": 100, "position": "center" });
    let (done, _) = run("video.watermark", &[turned], params);
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(size_of(&out.path), (180, 320));
    let frame = dir.path().join("frame.png");
    ffmpeg(&["-i", out.path.to_str().unwrap(), "-frames:v", "1"], &frame);
    let picture = image::open(&frame).unwrap().to_rgb8();
    let magenta = picture
        .pixels()
        .filter(|p| p.0[0] > 150 && p.0[1] < 100 && p.0[2] > 150)
        .count();
    assert!(magenta > 100, "{magenta}");
}

#[test]
fn a_timeline_has_frames_sound_and_a_length() {
    let dir = tempfile::tempdir().unwrap();
    let source = video(dir.path(), "a.mp4", "160x120", 2);
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, |_: &JobSnapshot| {}).unwrap();
    let exec = saqviro_tools::media::timeline::timeline();
    let done = jobs
        .wait(jobs.submit("timeline", exec, JobInput::Files(vec![source]), serde_json::Map::new()))
        .unwrap();
    let Some(JobOutput::Text(text)) = done.output else {
        panic!("{:?}", done.error);
    };
    let found: Value = serde_json::from_str(&text).unwrap();
    assert!((found["duration"].as_f64().unwrap() - 2.0).abs() < 0.2);
    assert!(found["frames"].as_array().unwrap().len() >= 8);
    assert!(!found["peaks"].as_array().unwrap().is_empty());
}
