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
    assert!(out.note.unwrap().contains("left as it is"));
    assert_eq!(fs::read(&out.path).unwrap(), fs::read(&src).unwrap());
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
    assert_eq!(name(&out), "clip-rotated.mp4");
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
    for file in ["-i evil.mp4", "100%d {stem}.mp4", "concat:a.mp4", "пример видео.mp4"] {
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
