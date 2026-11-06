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

fn half_red(dir: &Path, file: &str) -> PathBuf {
    let path = dir.join(file);
    let mut picture = image::RgbImage::from_pixel(10, 4, image::Rgb([0, 0, 255]));
    for y in 0..4 {
        for x in 0..5 {
            picture.put_pixel(x, y, image::Rgb([255, 0, 0]));
        }
    }
    picture.save(&path).unwrap();
    path
}

fn rgba(path: &Path) -> image::RgbaImage {
    image::open(path).unwrap().to_rgba8()
}

fn text(done: &JobSnapshot) -> String {
    match &done.output {
        Some(JobOutput::Text(text)) => text.clone(),
        other => panic!("expected text, got {other:?} / {:?}", done.error),
    }
}

#[test]
fn flips_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let (done, _) = run(
        "image.flip",
        std::slice::from_ref(&source),
        json!({ "direction": "horizontal" }),
    );
    assert_eq!(rgba(&outputs(&done)[0].path).get_pixel(0, 0).0, [0, 0, 255, 255]);
    let (done, _) = run("image.flip", &[source], json!({ "direction": "vertical" }));
    assert_eq!(rgba(&outputs(&done)[0].path).get_pixel(0, 0).0, [255, 0, 0, 255]);
}

#[test]
fn removes_color() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let (done, _) = run("image.grayscale", &[source], json!({}));
    let [r, g, b, _] = rgba(&outputs(&done)[0].path).get_pixel(0, 0).0;
    assert!(r == g && g == b, "{r} {g} {b}");
}

#[test]
fn adds_a_border() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let (done, _) = run(
        "image.border",
        std::slice::from_ref(&source),
        json!({ "width": 3, "color": "#0f0" }),
    );
    let out = rgba(&outputs(&done)[0].path);
    assert_eq!(out.dimensions(), (16, 10));
    assert_eq!(out.get_pixel(0, 0).0, [0, 255, 0, 255]);
    assert_eq!(out.get_pixel(3, 3).0, [255, 0, 0, 255]);

    let (done, _) = run("image.border", &[source], json!({ "color": "green" }));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn reads_colors() {
    use saqviro_tools::images::tools::parse_color;
    assert_eq!(parse_color("#ff8000").unwrap(), [255, 128, 0]);
    assert_eq!(parse_color("0af").unwrap(), [0, 170, 255]);
    assert!(parse_color("#12").is_err());
    assert!(parse_color("#ggg").is_err());
    assert!(parse_color("#фф").is_err());
}

#[test]
fn rounds_corners_with_transparency() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.jpg");
    image::RgbImage::from_pixel(100, 100, image::Rgb([200, 0, 0]))
        .save(&source)
        .unwrap();
    let (done, _) = run("image.round-corners", &[source], json!({ "radius": 50 }));
    let file = &outputs(&done)[0];
    assert_eq!(file.path.file_name().unwrap(), "a-rounded.png");
    let out = rgba(&file.path);
    assert_eq!(out.get_pixel(0, 0).0[3], 0);
    assert_eq!(out.get_pixel(50, 50).0[3], 255);
    assert!(out.get_pixel(50, 0).0[3] >= 250);
}

#[test]
fn flattens_transparency() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 0]))
        .save(&source)
        .unwrap();
    let (done, _) = run("image.remove-alpha", &[source], json!({ "color": "#00ff00" }));
    let out = image::open(&outputs(&done)[0].path).unwrap();
    assert!(!out.color().has_alpha());
    assert_eq!(out.to_rgb8().get_pixel(0, 0).0, [0, 255, 0]);
}

#[test]
fn describes_images() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let (done, _) = run("image.info", &[source], json!({}));
    let out = text(&done);
    assert!(out.starts_with("a.png\nSize: 10 × 4\nFormat: PNG\n"), "{out}");
    assert!(out.contains("Transparency: no"), "{out}");
}

#[test]
fn shows_camera_details_and_location() {
    let dir = tempfile::tempdir().unwrap();
    let jpeg = encoded(&two_tone(8, 8), ImageFormat::Jpeg);
    // One tag: Model, four ASCII bytes stored in the entry itself.
    let mut payload = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    payload.extend([0x01, 0x10, 0, 2, 0, 0, 0, 4]);
    payload.extend(b"Cam\0");
    payload.extend([0, 0, 0, 0]);
    let bytes = jpeg_with_segments(&jpeg, &[(0xE1, payload)]);
    let source = write(dir.path(), "a.jpg", &bytes);
    let (done, _) = run("image.info", &[source], json!({}));
    let out = text(&done);
    assert!(out.contains("Camera: \"Cam\""), "{out}");
}

#[test]
fn makes_an_icon_with_every_size() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let (done, _) = run("image.to-ico", &[source], json!({}));
    let file = &outputs(&done)[0];
    assert_eq!(file.path.file_name().unwrap(), "a.ico");
    let bytes = fs::read(&file.path).unwrap();
    assert_eq!(&bytes[..4], [0, 0, 1, 0]);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 7);
    let largest = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(largest.dimensions(), (256, 256));
    // A small picture is enlarged to fill the icon, not left in the middle.
    let [r, _, _, a] = largest.get_pixel(5, 128).0;
    assert!(r > 200 && a == 255, "{r} {a}");
}

#[test]
fn makes_the_icons_of_a_website() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "logo.png");
    let (done, _) = run("image.favicon", &[source], json!({}));
    let folder = &outputs(&done)[0].path;
    assert_eq!(folder.file_name().unwrap(), "logo-favicon");
    assert_eq!(
        image::image_dimensions(folder.join("apple-touch-icon.png")).unwrap(),
        (180, 180)
    );
    assert_eq!(
        image::image_dimensions(folder.join("icon-512.png")).unwrap(),
        (512, 512)
    );
    assert!(folder.join("favicon.ico").exists());
    let manifest: Value = serde_json::from_slice(&fs::read(folder.join("manifest.webmanifest")).unwrap()).unwrap();
    assert_eq!(manifest["icons"][1]["sizes"], "512x512");
}

#[test]
fn lays_images_out_on_a_sheet() {
    let dir = tempfile::tempdir().unwrap();
    let sources: Vec<PathBuf> = (0..3).map(|i| half_red(dir.path(), &format!("{i}.png"))).collect();
    let params = json!({ "columns": 2, "tile": 20, "gap": 2, "format": "png", "color": "#000000" });
    let (done, _) = run("image.contact-sheet", &sources, params);
    let file = &outputs(&done)[0];
    assert_eq!(file.path.file_name().unwrap(), "0-sheet.png");
    let out = rgba(&file.path);
    assert_eq!(out.dimensions(), (46, 46));
    assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0, 255]);
    assert_eq!(out.get_pixel(3, 12).0, [255, 0, 0, 255]);
}

#[test]
fn a_sheet_is_never_wider_than_its_images_need() {
    let dir = tempfile::tempdir().unwrap();
    let source = half_red(dir.path(), "a.png");
    let params = json!({ "columns": 9, "tile": 20, "gap": 0, "format": "png" });
    let (done, _) = run("image.contact-sheet", &[source], params);
    assert_eq!(rgba(&outputs(&done)[0].path).dimensions(), (20, 20));
}

#[test]
fn gif_frames_go_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let sources = [half_red(dir.path(), "a.png"), half_red(dir.path(), "b.png")];
    let (done, _) = run("image.frames-to-gif", &sources, json!({ "delay_ms": 100 }));
    let gif = outputs(&done)[0].path.clone();
    assert_eq!(gif.file_name().unwrap(), "a.gif");

    let (done, _) = run("image.gif-frames", &[gif], json!({}));
    let folder = &outputs(&done)[0].path;
    assert_eq!(folder.file_name().unwrap(), "a-frames");
    let frame = image::open(folder.join("frame-0001.png")).unwrap();
    assert_eq!(frame.dimensions(), (10, 4));
    assert!(fs::read_dir(folder).unwrap().count() >= 1);
}

#[test]
fn a_gif_needs_two_frames() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run("image.frames-to-gif", &[half_red(dir.path(), "a.png")], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn compares_images() {
    let dir = tempfile::tempdir().unwrap();
    let a = half_red(dir.path(), "a.png");
    let b = dir.path().join("b.png");
    let mut other = image::open(&a).unwrap().to_rgb8();
    other.put_pixel(0, 0, image::Rgb([0, 255, 0]));
    other.save(&b).unwrap();

    let (done, _) = run("image.compare", &[a.clone(), b], json!({}));
    let file = &outputs(&done)[0];
    assert_eq!(file.note.as_deref(), Some("2.50% of the pixels differ."));
    assert_eq!(rgba(&file.path).get_pixel(0, 0).0, [255, 0, 0, 255]);

    let (done, _) = run("image.compare", &[a.clone(), a.clone()], json!({}));
    assert_eq!(outputs(&done)[0].note.as_deref(), Some("The images are the same."));
}

#[test]
fn only_compares_what_can_be_compared() {
    let dir = tempfile::tempdir().unwrap();
    let a = half_red(dir.path(), "a.png");
    let b = dir.path().join("b.png");
    image::RgbImage::new(3, 3).save(&b).unwrap();
    let (done, _) = run("image.compare", &[a.clone(), b], json!({}));
    assert!(done.error.unwrap().message.contains("different sizes"));
    let (done, _) = run("image.compare", &[a], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#ff0000"/></svg>"##;

#[test]
fn draws_svg_at_any_size() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.svg");
    fs::write(&source, SVG).unwrap();
    let (done, _) = run("image.svg-to-png", std::slice::from_ref(&source), json!({}));
    assert_eq!(image::image_dimensions(&outputs(&done)[0].path).unwrap(), (20, 10));

    let (done, _) = run("image.svg-to-png", &[source], json!({ "width": 200 }));
    let out = rgba(&outputs(&done)[0].path);
    assert_eq!(out.dimensions(), (200, 100));
    assert_eq!(out.get_pixel(100, 50).0, [255, 0, 0, 255]);
}

#[test]
fn an_svg_cannot_pull_in_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let secret = half_red(dir.path(), "secret.png");
    let source = dir.path().join("a.svg");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="4"><image width="10" height="4" xlink:href="{}"/></svg>"#,
        secret.display()
    );
    fs::write(&source, svg).unwrap();
    let (done, _) = run("image.svg-to-png", &[source], json!({}));
    assert_eq!(rgba(&outputs(&done)[0].path).get_pixel(1, 1).0[3], 0);
}

#[test]
fn explains_a_broken_svg() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.svg");
    fs::write(&source, "<svg").unwrap();
    let (done, _) = run("image.svg-to-png", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::UnsupportedFormat);
}

#[test]
fn prints_text_on_a_picture_in_any_language() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.png");
    image::RgbImage::from_pixel(300, 200, image::Rgb([0, 0, 0]))
        .save(&source)
        .unwrap();
    let params = json!({ "text": "Привет", "size": 20, "opacity": 100, "position": "top-left" });
    let (done, _) = run("image.watermark", &[source], params);
    let out = rgba(&outputs(&done)[0].path);
    assert_eq!(out.dimensions(), (300, 200));
    let (mut left, mut right) = (0, 0);
    for (x, y, pixel) in out.enumerate_pixels() {
        if pixel.0[0] > 200 {
            if x < 150 && y < 100 { left += 1 } else { right += 1 }
        }
    }
    assert!(left > 100 && right < left / 10, "{left} {right}");
}

#[test]
fn a_watermark_needs_text() {
    let dir = tempfile::tempdir().unwrap();
    let (done, _) = run(
        "image.watermark",
        &[half_red(dir.path(), "a.png")],
        json!({ "text": " " }),
    );
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn a_long_watermark_stays_inside_the_picture() {
    use saqviro_tools::images::overlay::Watermark;
    let p = json!({ "text": "x".repeat(200), "size": 50, "opacity": 100, "position": "center" });
    let layer = Watermark::from_params(p.as_object().unwrap())
        .unwrap()
        .layer((100, 100))
        .unwrap();
    assert_eq!(layer.dimensions(), (100, 100));
    assert_eq!(layer.get_pixel(0, 50).0[3], 0, "the edge stays clear");
}
