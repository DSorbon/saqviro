//! The parts of the media engine that need no FFmpeg.

use saqviro_jobs::{ErrorKind, Params};
use saqviro_tools::media::{
    Encoders, failure, parse_progress,
    probe::{Probe, describe},
    tools::{Container, Range, parse_loudness, parse_time, plan_video},
};
use saqviro_tools::process::find_binary;
use serde_json::json;

fn params(value: serde_json::Value) -> Params {
    value.as_object().unwrap().clone()
}

#[test]
fn reads_times_people_type() {
    assert_eq!(parse_time("90"), Some(90.0));
    assert_eq!(parse_time("1:30"), Some(90.0));
    assert_eq!(parse_time("01:00:02.5"), Some(3602.5));
    assert_eq!(parse_time(" 0:05 "), Some(5.0));
}

#[test]
fn rejects_what_is_not_a_time() {
    for text in ["", "abc", "1:2:3:4", "-5", "1:-2", ":30", "1e3", "1.5:00", "inf", "NaN"] {
        assert_eq!(parse_time(text), None, "{text}");
    }
}

#[test]
fn checks_the_selected_range() {
    let read = |v, duration| Range::from_params(&params(v), duration);
    let range = read(json!({ "start": "0:10", "end": "0:20" }), Some(60.0)).unwrap();
    assert_eq!((range.start, range.end), (Some(10.0), Some(20.0)));

    assert!(read(json!({ "start": "0:20", "end": "0:10" }), None).is_err());
    assert!(read(json!({ "start": "0:10", "end": "0:10" }), None).is_err());
    assert!(read(json!({ "start": "2:00" }), Some(60.0)).is_err());
    let e = read(json!({ "end": "soon" }), None).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    assert!(e.message.contains("soon"));
}

#[test]
fn empty_time_fields_mean_not_set() {
    let range = Range::from_params(&params(json!({ "start": "  ", "end": "" })), None).unwrap();
    assert_eq!((range.start, range.end), (None, None));
}

const MAC: Encoders = Encoders {
    h264: Some("h264_videotoolbox"),
    vp9: true,
};
const LINUX: Encoders = Encoders { h264: None, vp9: true };

#[test]
fn uses_the_system_h264_encoder_when_there_is_one() {
    let plan = plan_video(MAC, Container::Mp4).unwrap();
    assert_eq!(
        (plan.container, plan.encoder, plan.note),
        (Container::Mp4, "h264_videotoolbox", None)
    );
}

#[test]
fn webm_is_always_vp9() {
    assert_eq!(plan_video(MAC, Container::Webm).unwrap().encoder, "libvpx-vp9");
}

#[test]
fn falls_back_to_webm_and_says_so() {
    for wanted in [Container::Mp4, Container::Mov] {
        let plan = plan_video(LINUX, wanted).unwrap();
        assert_eq!((plan.container, plan.encoder), (Container::Webm, "libvpx-vp9"));
        assert!(plan.note.unwrap().contains("WebM"));
    }
}

#[test]
fn mkv_holds_vp9_so_it_needs_no_fallback() {
    let plan = plan_video(LINUX, Container::Mkv).unwrap();
    assert_eq!(
        (plan.container, plan.encoder, plan.note),
        (Container::Mkv, "libvpx-vp9", None)
    );
}

#[test]
fn fails_clearly_without_any_encoder() {
    let none = Encoders { h264: None, vp9: false };
    assert_eq!(
        plan_video(none, Container::Mp4).unwrap_err().kind,
        ErrorKind::EngineMissing
    );
}

#[test]
fn reads_progress_lines() {
    assert_eq!(parse_progress("out_time_us=2500000"), Some(2.5));
    assert_eq!(parse_progress("out_time_us=N/A"), None);
    assert_eq!(parse_progress("out_time_us=-100"), None);
    assert_eq!(parse_progress("frame=12"), None);
}

#[test]
fn explains_failures() {
    assert_eq!(
        failure("x\nav_interleaved: No space left on device\n").kind,
        ErrorKind::DiskFull
    );
    assert_eq!(
        failure("a.mp4: Invalid data found when processing input").kind,
        ErrorKind::UnsupportedFormat
    );
    assert_eq!(
        failure("[concat] Format not on whitelist 'mov'").kind,
        ErrorKind::UnsupportedFormat
    );
    let e = failure("first\nthe real reason\n\n");
    assert_eq!(
        (e.kind, e.message.as_str()),
        (ErrorKind::ProcessFailed, "the real reason")
    );
    assert_eq!(failure("").kind, ErrorKind::ProcessFailed);
}

#[test]
fn finds_binaries_in_the_first_directory_that_has_them() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let file = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::fs::write(b.path().join(&file), "").unwrap();
    let dirs = [a.path().to_path_buf(), b.path().to_path_buf()];
    assert_eq!(find_binary(dirs.clone(), "ffmpeg"), Some(b.path().join(&file)));
    assert_eq!(find_binary(dirs, "ffprobe"), None);
}

#[test]
fn reads_the_loudness_report_at_the_end_of_the_log() {
    let log = r#"[Parsed_loudnorm_0 @ 0x1] { not json
{
    "input_i" : "-27.61",
    "input_tp" : "-4.47",
    "input_lra" : "18.06",
    "input_thresh" : "-39.20",
    "target_offset" : "0.58"
}
"#;
    let l = parse_loudness(log).unwrap();
    assert_eq!((l.integrated, l.true_peak, l.offset), (-27.61, -4.47, 0.58));
}

#[test]
fn silence_cannot_be_normalized() {
    let log = r#"{ "input_i": "-inf", "input_tp": "-inf", "input_lra": "0.00", "input_thresh": "-inf", "target_offset": "inf" }"#;
    let e = parse_loudness(log).unwrap_err();
    assert!(e.message.contains("silent"), "{e:?}");
}

#[test]
fn a_log_without_a_report_is_a_failure() {
    assert_eq!(
        parse_loudness("nothing here").unwrap_err().kind,
        ErrorKind::ProcessFailed
    );
}

const PROBED: &str = r#"{
  "streams": [
    { "index": 0, "codec_type": "audio", "codec_name": "aac", "sample_rate": "44100", "channels": 2, "bit_rate": "128000" },
    { "index": 1, "codec_type": "video", "codec_name": "mjpeg", "width": 500, "height": 500,
      "disposition": { "attached_pic": 1 } },
    { "index": 2, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
      "avg_frame_rate": "30000/1001", "disposition": { "attached_pic": 0 } }
  ],
  "format": { "format_name": "mov,mp4", "duration": "61.500000", "size": "8000000", "bit_rate": "1128000" }
}"#;

#[test]
fn reads_streams_and_skips_cover_art() {
    let probe = Probe::parse(PROBED).unwrap();
    let video = probe.video.as_ref().unwrap();
    assert_eq!((video.index, video.codec.as_str(), video.width), (2, "h264", 1920));
    assert!((video.fps.unwrap() - 29.97).abs() < 0.01);
    assert_eq!(probe.duration, Some(61.5));
    assert_eq!(probe.audio[0].sample_rate, Some(44100));
}

#[test]
fn estimates_the_video_bitrate_from_the_total() {
    assert_eq!(Probe::parse(PROBED).unwrap().video_bit_rate(), Some(1_000_000));
}

#[test]
fn an_audio_file_with_cover_art_has_no_video() {
    let json = r#"{ "streams": [
        { "index": 0, "codec_type": "video", "codec_name": "png", "disposition": { "attached_pic": 1 } }
    ], "format": {} }"#;
    assert_eq!(Probe::parse(json).unwrap().video, None);
}

#[test]
fn describes_a_file_for_people() {
    let text = describe("/a/clip.mp4".as_ref(), &Probe::parse(PROBED).unwrap());
    assert!(text.starts_with("clip.mp4\n"), "{text}");
    for part in [
        "00:01:01.500",
        "7.6 MB",
        "1920×1080",
        "29.97 fps",
        "1.0 Mbit/s",
        "aac, 44100 Hz, 2 ch",
    ] {
        assert!(text.contains(part), "{part} missing from:\n{text}");
    }
}
