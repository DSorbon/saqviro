mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use common::*;
use image::{GenericImageView, ImageFormat};
use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, ErrorKind, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, OutputFile};
use saqviro_tools::images::decode;
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

fn outputs(done: &JobSnapshot) -> Vec<OutputFile> {
    match &done.output {
        Some(JobOutput::Files(files)) => files.clone(),
        other => panic!("expected files, got {other:?} / {:?}", done.error),
    }
}

fn name(f: &OutputFile) -> String {
    f.path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn converts_next_to_the_original_without_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    let original = encoded(&two_tone(40, 20), ImageFormat::Png);
    let src = write(dir.path(), "logo.png", &original);

    let (done, _) = run("image.convert", std::slice::from_ref(&src), json!({ "format": "webp" }));
    let out = outputs(&done);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].path.parent(), Some(dir.path()));
    assert_eq!(name(&out[0]), "logo.webp");
    assert_eq!(out[0].size, fs::metadata(&out[0].path).unwrap().len());
    assert_eq!(out[0].source_size, Some(original.len() as u64));
    assert_eq!(decode(&out[0].path).unwrap().image.dimensions(), (40, 20));
    assert_eq!(fs::read(&src).unwrap(), original);
}

#[test]
fn converting_to_the_same_format_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "photo.jpg", &encoded(&two_tone(40, 20), ImageFormat::Jpeg));
    let (done, _) = run("image.convert", &[src], json!({ "format": "jpg" }));
    assert_eq!(name(&outputs(&done)[0]), "photo (2).jpg");
}

#[test]
fn processes_batches_with_progress() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = (0..3)
        .map(|i| {
            write(
                dir.path(),
                &format!("{i}.png"),
                &encoded(&two_tone(40, 20), ImageFormat::Png),
            )
        })
        .collect();
    let (done, progress) = run("image.to-webp", &files, json!({}));
    assert_eq!(
        outputs(&done).iter().map(name).collect::<Vec<_>>(),
        ["0.webp", "1.webp", "2.webp"]
    );
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
    assert!(progress.contains(&(1.0 / 3.0)), "{progress:?}");
}

#[test]
fn a_bad_file_fails_the_batch_and_leaves_no_partial_results() {
    let dir = tempfile::tempdir().unwrap();
    let good = write(dir.path(), "good.png", &encoded(&two_tone(40, 20), ImageFormat::Png));
    let bad = write(dir.path(), "bad.png", b"not an image");
    let (done, _) = run("image.convert", &[good, bad], json!({ "format": "jpg" }));

    assert_eq!(done.status, JobStatus::Failed);
    let err = done.error.unwrap();
    assert_eq!(err.kind, ErrorKind::UnsupportedFormat);
    assert!(err.message.contains("bad.png"), "{}", err.message);
    assert!(!dir.path().join("good.jpg").exists(), "partial result was left behind");
}

#[test]
fn converts_to_avif_and_from_webp() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "a.png", &encoded(&two_tone(40, 20), ImageFormat::Png));
    let (done, _) = run("image.to-avif", &[src], json!({}));
    let avif = fs::read(&outputs(&done)[0].path).unwrap();
    assert_eq!(&avif[4..12], b"ftypavif");

    let webp = write(dir.path(), "b.webp", &encoded(&two_tone(40, 20), ImageFormat::WebP));
    let (done, _) = run("image.webp-to-jpg", &[webp], json!({ "format": "png" }));
    assert_eq!(name(&outputs(&done)[0]), "b.png");
}

#[test]
fn resizes_and_names_the_result_by_its_size() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "cat.jpg", &encoded(&two_tone(400, 300), ImageFormat::Jpeg));
    let (done, _) = run("image.resize", &[src], json!({ "width": 200 }));
    let out = &outputs(&done)[0];
    assert_eq!(name(out), "cat-200x150.jpg");
    assert_eq!(decode(&out.path).unwrap().image.dimensions(), (200, 150));
}

#[test]
fn resize_writes_unwritable_formats_as_png() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "old.bmp", &encoded(&two_tone(400, 300), ImageFormat::Bmp));
    let (done, _) = run("image.resize", &[src], json!({ "width": 100 }));
    assert_eq!(name(&outputs(&done)[0]), "old-100x75.png");
}

#[test]
fn compress_makes_jpegs_smaller_and_never_bigger() {
    let dir = tempfile::tempdir().unwrap();
    let noisy = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(300, 200, |x, y| {
        image::Rgb([(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x ^ y) % 256) as u8])
    }));
    let mut big = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut big, 100)
        .encode_image(&noisy)
        .unwrap();
    let src = write(dir.path(), "big.jpg", big.get_ref());

    let (done, _) = run("image.compress", &[src], json!({ "quality": 60 }));
    let out = &outputs(&done)[0];
    assert_eq!(name(out), "big-compressed.jpg");
    assert!(
        out.size < out.source_size.unwrap() / 2,
        "{} -> {}",
        out.source_size.unwrap(),
        out.size
    );

    let tiny = write(dir.path(), "tiny.jpg", &encoded(&two_tone(8, 8), ImageFormat::Jpeg));
    let (done, _) = run("image.compress", &[tiny], json!({ "quality": 100 }));
    let out = &outputs(&done)[0];
    assert!(
        out.size <= out.source_size.unwrap(),
        "{} -> {}",
        out.source_size.unwrap(),
        out.size
    );
}

#[test]
fn compress_keeps_png_pixels_identical() {
    let dir = tempfile::tempdir().unwrap();
    let img = two_tone(64, 64);
    let src = write(dir.path(), "flat.png", &encoded(&img, ImageFormat::Png));
    let (done, _) = run("image.compress", &[src], json!({}));
    let out = decode(&outputs(&done)[0].path).unwrap().image;
    assert_eq!(out.to_rgb8(), img.to_rgb8());
}

#[test]
fn rotates_right() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "wide.png", &encoded(&two_tone(40, 20), ImageFormat::Png));
    let (done, _) = run("image.rotate", &[src], json!({ "angle": "90" }));
    let out = &outputs(&done)[0];
    assert_eq!(name(out), "wide-rotated.png");
    let img = decode(&out.path).unwrap().image;
    assert_eq!(img.dimensions(), (20, 40));
    let top = img.get_pixel(10, 5);
    assert!(
        top[0] > 150 && top[2] < 100,
        "the red left half should be on top: {top:?}"
    );
}

#[test]
fn removes_metadata_and_verifies_it() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = jpeg_with_segments(
        &encoded(&two_tone(40, 20), ImageFormat::Jpeg),
        &[exif_app1(6, true), xmp_app1()],
    );
    let src = write(dir.path(), "IMG_0001.jpg", &jpeg);
    let (done, _) = run("image.metadata.remove", std::slice::from_ref(&src), json!({}));
    let out = &outputs(&done)[0];
    assert_eq!(name(out), "IMG_0001-clean.jpg");
    let clean = fs::read(&out.path).unwrap();
    assert!(!clean.windows(3).any(|w| w == b"Bob"));
    assert_eq!(decode(&out.path).unwrap().image.dimensions(), (20, 40));
    assert_eq!(fs::read(&src).unwrap(), jpeg, "the original was modified");
}

#[test]
fn file_tools_reject_text_input() {
    let registry = registry();
    let tool = registry.get("image.convert").unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = JobManager::new(
        Config {
            temp_root: root.path().join("j"),
            workers: 1,
        },
        |_: &JobSnapshot| {},
    )
    .unwrap();
    let params = tool.resolve_params(&serde_json::Map::new()).unwrap();
    let done = jobs
        .wait(jobs.submit(tool.id.as_str(), exec, JobInput::Text("x".into()), params))
        .unwrap();
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
    let done_empty = {
        let exec = saqviro_tools::executor(tool).unwrap();
        let params = tool.resolve_params(&serde_json::Map::new()).unwrap();
        jobs.wait(jobs.submit(tool.id.as_str(), exec, JobInput::Files(vec![]), params))
            .unwrap()
    };
    assert_eq!(done_empty.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn crops_a_selection_and_stops_at_the_edge() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("wide.png");
    image::RgbImage::from_pixel(40, 20, image::Rgb([9, 9, 9]))
        .save(&source)
        .unwrap();

    let (done, _) = run(
        "image.crop",
        std::slice::from_ref(&source),
        json!({ "x": 30, "y": 5, "width": 100, "height": 10 }),
    );
    let file = &outputs(&done)[0];
    assert_eq!(file.path.file_name().unwrap(), "wide-cropped.png");
    assert_eq!(image::open(&file.path).unwrap().dimensions(), (10, 10));
}

#[test]
fn crops_to_a_centered_shape() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("wide.png");
    let mut picture = image::RgbImage::from_pixel(40, 20, image::Rgb([0, 0, 0]));
    picture.put_pixel(10, 0, image::Rgb([255, 0, 0]));
    picture.save(&source).unwrap();

    let (done, _) = run("image.crop", &[source], json!({ "aspect": "1:1" }));
    let result = image::open(&outputs(&done)[0].path).unwrap();
    assert_eq!(result.dimensions(), (20, 20));
    assert_eq!(result.to_rgb8().get_pixel(0, 0), &image::Rgb([255, 0, 0]));
}

#[test]
fn refuses_an_area_outside_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("small.png");
    image::RgbImage::new(10, 10).save(&source).unwrap();

    let (done, _) = run("image.crop", &[source], json!({ "x": 10, "width": 5 }));
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn a_preview_is_small_but_reports_the_real_size() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("big.png");
    image::RgbImage::new(300, 100).save(&source).unwrap();

    let preview = saqviro_tools::images::preview(&source, 60).unwrap();
    assert_eq!((preview.width, preview.height), (300, 100));
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(preview.data).unwrap();
    assert_eq!(image::load_from_memory(&bytes).unwrap().dimensions(), (60, 20));
}

#[test]
fn results_go_to_the_chosen_folder() {
    let registry = registry();
    let tool = registry.get("image.rotate").unwrap();
    let params = tool.resolve_params(json!({}).as_object().unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.png");
    image::RgbImage::new(4, 2).save(&source).unwrap();

    let config = Config {
        temp_root: dir.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, |_: &JobSnapshot| {}).unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let id = jobs.submit_to(
        tool.id.as_str(),
        exec,
        JobInput::Files(vec![source]),
        params,
        Some(elsewhere.path().to_path_buf()),
    );
    let done = jobs.wait(id).unwrap();
    assert_eq!(outputs(&done)[0].path.parent().unwrap(), elsewhere.path());
    assert!(!dir.path().join("a-rotated.png").exists());
}
