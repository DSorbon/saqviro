//! Runs every PDF tool through real qpdf and PDFium, on PDFs generated here.
//! Page N of a fixture is 200 + N points wide, so order can be read back.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use image::{DynamicImage, GenericImageView, GrayImage, ImageFormat, Luma, Rgb, RgbImage};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};
use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, ErrorKind, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, OutputFile};
use saqviro_tools::images::{Format, encode};
use serde_json::{Value, json};

const RENDERER: &str = env!("CARGO_BIN_EXE_saqviro-pdf-render");

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

fn run_with_progress(tool_id: &str, files: &[PathBuf], params: Value) -> (JobSnapshot, Vec<f32>) {
    assert!(Path::new(RENDERER).is_file());
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

fn run(tool_id: &str, files: &[PathBuf], params: Value) -> JobSnapshot {
    run_with_progress(tool_id, files, params).0
}

fn results(done: &JobSnapshot) -> Vec<OutputFile> {
    match &done.output {
        Some(JobOutput::Files(files)) => files.clone(),
        other => panic!("expected files, got {other:?} / {:?}", done.error),
    }
}

fn one(done: &JobSnapshot) -> OutputFile {
    let files = results(done);
    assert_eq!(files.len(), 1);
    files[0].clone()
}

fn name(f: &OutputFile) -> String {
    f.path.file_name().unwrap().to_string_lossy().into_owned()
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn add_page(doc: &mut Document, pages: lopdf::ObjectId, width: i64, content: &str, xobjects: Dictionary) -> Object {
    let content = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
    Object::Reference(doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), 400.into()],
        "Contents" => content,
        "Resources" => dictionary! { "XObject" => xobjects },
    }))
}

fn finish(mut doc: Document, pages: lopdf::ObjectId, kids: Vec<Object>, path: &Path) {
    let count = kids.len() as i64;
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let xmp = Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta>XMPSECRET</x:xmpmeta>".to_vec(),
    );
    let xmp = doc.add_object(xmp);
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "Metadata" => xmp });
    doc.trailer.set("Root", catalog);
    let info = doc.add_object(dictionary! { "Author" => Object::string_literal("AUTHORSECRET") });
    doc.trailer.set("Info", info);
    doc.save(path).unwrap();
}

/// A PDF with `count` pages and document properties.
fn pdf(dir: &Path, file: &str, count: i64) -> PathBuf {
    let path = dir.join(file);
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let kids = (1..=count)
        .map(|n| add_page(&mut doc, pages, 200 + n, "1 0 0 rg 0 0 100 100 re f", Dictionary::new()))
        .collect();
    finish(doc, pages, kids, &path);
    path
}

/// Deterministic detail, so images really are expensive to store.
fn noise(w: u32, h: u32) -> RgbImage {
    let mut state = 7u32;
    RgbImage::from_fn(w, h, |x, y| {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let grain = (state >> 24) as u8 / 4;
        Rgb([
            ((x * 255 / w) as u8).saturating_add(grain),
            ((y * 255 / h) as u8).saturating_add(grain),
            120u8.saturating_add(grain),
        ])
    })
}

fn image_dict(w: u32, h: u32, space: &str) -> Dictionary {
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(w),
        "Height" => i64::from(h),
        "BitsPerComponent" => 8,
    };
    dict.set("ColorSpace", Object::Name(space.as_bytes().to_vec()));
    dict
}

fn jpeg_stream(image: &RgbImage) -> Stream {
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image.clone())
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    let mut dict = image_dict(image.width(), image.height(), "DeviceRGB");
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    let mut stream = Stream::new(dict, bytes.into_inner());
    stream.allows_compression = false;
    stream
}

/// One page that shows `image` across its whole area.
fn image_pdf(dir: &Path, file: &str, image: Stream) -> PathBuf {
    let path = dir.join(file);
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let id = doc.add_object(image);
    let draw = "q 400 0 0 400 0 0 cm /Im Do Q";
    let kid = add_page(&mut doc, pages, 400, draw, dictionary! { "Im" => id });
    finish(doc, pages, vec![kid], &path);
    path
}

/// qpdf's verdict on a file, from outside the code under test.
fn check(path: &Path, password: Option<&str>) {
    let mut command = Command::new("qpdf");
    if let Some(password) = password {
        command.arg(format!("--password={password}"));
    }
    let out = command.arg("--check").arg(path).output().unwrap();
    assert!(
        out.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stdout)
    );
}

fn widths(path: &Path) -> Vec<i64> {
    check(path, None);
    let doc = Document::load(path).unwrap();
    pages_of(&doc)
        .values()
        .map(|d| d.get(b"MediaBox").unwrap().as_array().unwrap()[2].as_i64().unwrap())
        .collect()
}

fn pages_of(doc: &Document) -> BTreeMap<u32, Dictionary> {
    doc.get_pages()
        .into_iter()
        .map(|(n, id)| (n, doc.get_dictionary(id).unwrap().clone()))
        .collect()
}

fn images_in(path: &Path) -> Vec<Stream> {
    Document::load(path)
        .unwrap()
        .objects
        .into_values()
        .filter_map(|o| match o {
            Object::Stream(s) if s.dict.get(b"Subtype").is_ok_and(|t| t.as_name().unwrap() == b"Image") => Some(s),
            _ => None,
        })
        .collect()
}

/// A page as pixels, drawn by the same tool users get.
fn rendered(path: &Path, dpi: u32) -> DynamicImage {
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("page.pdf");
    fs::copy(path, &copy).unwrap();
    let done = run("pdf.to-images", &[copy], json!({ "dpi": dpi }));
    image::open(&results(&done)[0].path).unwrap()
}

#[test]
fn merges_in_the_order_given() {
    let dir = tempfile::tempdir().unwrap();
    let a = pdf(dir.path(), "a.pdf", 2);
    let b = pdf(dir.path(), "b.pdf", 1);
    let done = run("pdf.merge", &[b, a], json!({}));
    let out = one(&done);
    assert_eq!(name(&out), "b-merged.pdf");
    assert_eq!(widths(&out.path), [201, 201, 202]);
}

#[test]
fn merging_needs_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let a = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.merge", &[a], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn merging_names_the_file_that_failed() {
    let dir = tempfile::tempdir().unwrap();
    let a = pdf(dir.path(), "a.pdf", 1);
    let bad = dir.path().join("bad.pdf");
    fs::write(&bad, "not a pdf").unwrap();
    let done = run("pdf.merge", &[a, bad], json!({}));
    let e = done.error.unwrap();
    assert!(e.message.starts_with("bad.pdf: "), "{}", e.message);
    assert_eq!(entries(dir.path()), ["a.pdf", "bad.pdf"]);
}

#[test]
fn splits_into_single_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 12);
    let (done, progress) = run_with_progress("pdf.split", &[src], json!({}));
    let out = results(&done);
    assert_eq!(out.len(), 12);
    assert_eq!(
        (name(&out[0]), name(&out[11])),
        ("doc-01.pdf".into(), "doc-12.pdf".into())
    );
    assert_eq!(widths(&out[4].path), [205]);
    assert_eq!(out[0].source_size, None);
    assert!(
        progress.iter().filter(|p| **p > 0.0 && **p < 1.0).count() > 3,
        "{progress:?}"
    );
}

#[test]
fn splits_into_equal_parts_with_a_shorter_last_one() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 5);
    let out = results(&run("pdf.split", &[src], json!({ "pages": 2 })));
    assert_eq!(out.len(), 3);
    assert_eq!(widths(&out[0].path), [201, 202]);
    assert_eq!(widths(&out[2].path), [205]);
}

#[test]
fn deletes_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 5);
    let done = run(
        "pdf.delete-pages",
        std::slice::from_ref(&src),
        json!({ "pages": "2, 4-5" }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "doc-edited.pdf");
    assert_eq!(widths(&out.path), [201, 203]);
    assert_eq!(widths(&src).len(), 5, "the original changed");
}

#[test]
fn cannot_delete_every_page() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 2);
    let done = run("pdf.delete-pages", &[src], json!({ "pages": "1-2" }));
    assert!(done.error.unwrap().message.contains("at least one page"));
    assert_eq!(entries(dir.path()), ["doc.pdf"]);
}

#[test]
fn page_tools_need_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 2);
    let done = run("pdf.extract-pages", &[src], json!({}));
    assert!(done.error.unwrap().message.contains("enter page numbers"));
}

#[test]
fn extracts_pages_in_the_order_listed() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 5);
    let done = run("pdf.extract-pages", &[src], json!({ "pages": "4, 1-2" }));
    assert_eq!(widths(&one(&done).path), [204, 201, 202]);
}

#[test]
fn reports_a_page_that_does_not_exist() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    let done = run("pdf.extract-pages", &[src], json!({ "pages": "7" }));
    assert_eq!(
        done.error.unwrap().message,
        "doc.pdf: there is no page 7, this PDF has 3 pages"
    );
}

#[test]
fn reorders_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 4);
    let done = run(
        "pdf.reorder-pages",
        std::slice::from_ref(&src),
        json!({ "order": "3, 1" }),
    );
    assert_eq!(widths(&one(&done).path), [203, 201, 202, 204]);
    let done = run("pdf.reorder-pages", &[src], json!({ "reverse": true }));
    assert_eq!(widths(&one(&done).path), [204, 203, 202, 201]);
}

fn rotations(path: &Path) -> Vec<i64> {
    let doc = Document::load(path).unwrap();
    pages_of(&doc)
        .values()
        .map(|d| d.get(b"Rotate").map_or(0, |r| r.as_i64().unwrap()))
        .collect()
}

#[test]
fn rotates_listed_pages_or_all_of_them() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    let done = run(
        "pdf.rotate-pages",
        std::slice::from_ref(&src),
        json!({ "angle": "90", "pages": "2" }),
    );
    assert_eq!(rotations(&one(&done).path), [0, 90, 0]);
    let done = run("pdf.rotate-pages", &[src], json!({ "angle": "270" }));
    let out = one(&done);
    assert_eq!(rotations(&out.path), [270, 270, 270]);
    check(&out.path, None);
}

#[test]
fn a_password_protects_a_pdf_and_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 2);
    let password = "пароль with space \"quotes\" --and-dashes";

    let done = run("pdf.password.add", &[src], json!({ "password": password }));
    let locked = one(&done);
    assert_eq!(name(&locked), "doc-protected.pdf");
    check(&locked.path, Some(password));
    let info = Command::new("qpdf")
        .arg(format!("--password={password}"))
        .arg("--show-encryption")
        .arg(&locked.path)
        .output()
        .unwrap();
    let info = String::from_utf8_lossy(&info.stdout);
    assert!(info.contains("AESv3"), "{info}");
    assert!(info.contains("Supplied password is user password"), "{info}");
    let needs = Command::new("qpdf")
        .arg("--requires-password")
        .arg(&locked.path)
        .status()
        .unwrap();
    assert_eq!(needs.code(), Some(0), "opens without a password");

    let done = run(
        "pdf.password.remove",
        std::slice::from_ref(&locked.path),
        json!({ "password": password }),
    );
    assert_eq!(widths(&one(&done).path), [201, 202]);
}

#[test]
fn a_wrong_password_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 1);
    let locked = one(&run("pdf.password.add", &[src], json!({ "password": "right" }))).path;

    let done = run("pdf.password.remove", &[locked], json!({ "password": "wrong" }));

    let e = done.error.unwrap();
    assert_eq!(e.kind, ErrorKind::PermissionDenied);
    assert_eq!(e.message, "doc-protected.pdf: the password is not correct");
}

#[test]
fn password_tools_need_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 1);
    let done = run("pdf.password.add", std::slice::from_ref(&src), json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
    let done = run("pdf.password.add", &[src], json!({ "password": "two\nlines" }));
    assert!(done.error.unwrap().message.contains("line breaks"));
}

#[test]
fn other_tools_explain_a_protected_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 2);
    let locked = one(&run("pdf.password.add", &[src], json!({ "password": "secret" }))).path;
    let tools = [
        ("pdf.split", json!({})),
        ("pdf.compress", json!({})),
        ("pdf.metadata.remove", json!({})),
        ("pdf.to-images", json!({})),
    ];
    for (tool, params) in tools {
        let e = run(tool, std::slice::from_ref(&locked), params).error.unwrap();
        assert_eq!(e.kind, ErrorKind::PermissionDenied, "{tool}: {e:?}");
        assert!(e.message.contains("Remove PDF Password"), "{tool}: {}", e.message);
    }
}

/// Everything in a file, with streams decoded, so nothing can hide.
fn full_dump(path: &Path) -> Vec<u8> {
    let out = Command::new("qpdf")
        .args(["--qdf", "--object-streams=disable", "--stream-data=uncompress"])
        .arg(path)
        .arg("-")
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn removes_metadata_and_keeps_the_pages() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    assert!(
        contains(&full_dump(&src), b"AUTHORSECRET"),
        "the fixture must have metadata"
    );
    assert!(contains(&full_dump(&src), b"XMPSECRET"));

    let done = run("pdf.metadata.remove", &[src], json!({}));
    let out = one(&done);

    assert_eq!(name(&out), "doc-clean.pdf");
    let dump = full_dump(&out.path);
    assert!(!contains(&dump, b"AUTHORSECRET"));
    assert!(!contains(&dump, b"XMPSECRET"));
    assert_eq!(widths(&out.path), [201, 202, 203]);
    let raw = fs::read(&out.path).unwrap();
    assert!(!contains(&raw, b"AUTHORSECRET"), "a copy survived in the raw file");
}

#[test]
fn metadata_in_an_earlier_revision_goes_too() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 1);
    // An incremental update leaves the old properties in the file's bytes.
    let mut bytes = fs::read(&src).unwrap();
    bytes.extend_from_slice(b"\n% AUTHORSECRET in an old revision\n");
    fs::write(&src, bytes).unwrap();

    let out = one(&run("pdf.metadata.remove", &[src], json!({})));
    assert!(!contains(&fs::read(&out.path).unwrap(), b"AUTHORSECRET"));
}

#[test]
fn compress_makes_a_scanned_pdf_much_smaller_and_it_still_looks_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let src = image_pdf(dir.path(), "scan.pdf", jpeg_stream(&noise(2400, 2400)));

    let (done, progress) = run_with_progress(
        "pdf.compress",
        std::slice::from_ref(&src),
        json!({ "dpi": 150, "quality": 60 }),
    );
    let out = one(&done);

    assert_eq!(name(&out), "scan-compressed.pdf");
    assert!(
        out.size * 3 < out.source_size.unwrap(),
        "{} → {}",
        out.source_size.unwrap(),
        out.size
    );
    check(&out.path, None);
    let image = &images_in(&out.path)[0];
    assert_eq!(
        image.dict.get(b"Width").unwrap().as_i64().unwrap(),
        834,
        "400 pt at 150 dpi"
    );
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");

    let (before, after) = (rendered(&src, 72).to_rgb8(), rendered(&out.path, 72).to_rgb8());
    assert_eq!(before.dimensions(), after.dimensions());
    let difference: u64 = before
        .pixels()
        .zip(after.pixels())
        .map(|(a, b)| (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>())
        .sum();
    let mean = difference as f64 / (before.len() as f64);
    assert!(mean < 8.0, "pages differ by {mean} per channel");
}

#[test]
fn compress_handles_raw_rgb_and_gray_images() {
    let dir = tempfile::tempdir().unwrap();
    let rgb = noise(900, 900);
    let mut stream = Stream::new(image_dict(900, 900, "DeviceRGB"), rgb.as_raw().clone());
    stream.compress().unwrap();
    let src = image_pdf(dir.path(), "rgb.pdf", stream);
    let out = one(&run("pdf.compress", &[src], json!({})));
    check(&out.path, None);
    assert!(out.size < out.source_size.unwrap());
    assert_eq!(
        images_in(&out.path)[0].dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"DCTDecode"
    );

    let gray = GrayImage::from_fn(900, 900, |x, y| Luma([((x * 31 + y * 17) % 251) as u8]));
    let stream = Stream::new(image_dict(900, 900, "DeviceGray"), gray.into_raw());
    let src = image_pdf(dir.path(), "gray.pdf", stream);
    let out = one(&run("pdf.compress", &[src], json!({})));
    let image = &images_in(&out.path)[0];
    assert_eq!(image.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceRGB");
    rendered(&out.path, 72);
}

#[test]
fn compress_leaves_images_it_cannot_treat_alone() {
    let dir = tempfile::tempdir().unwrap();
    let cmyk: Vec<u8> = (0..300 * 300 * 4).map(|i| (i % 251) as u8).collect();
    let src = image_pdf(
        dir.path(),
        "cmyk.pdf",
        Stream::new(image_dict(300, 300, "DeviceCMYK"), cmyk),
    );
    let out = one(&run("pdf.compress", &[src], json!({})));
    let image = &images_in(&out.path)[0];
    assert_eq!(image.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceCMYK");
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 300);
}

#[test]
fn compress_keeps_soft_masks_exact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("masked.pdf");
    let mask: Vec<u8> = (0..500 * 500).map(|i| ((i * 7) % 253) as u8).collect();
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let mask_id = doc.add_object(Stream::new(image_dict(500, 500, "DeviceGray"), mask));
    let mut picture = jpeg_stream(&noise(1000, 1000));
    picture.dict.set("SMask", mask_id);
    let picture = doc.add_object(picture);
    let kid = add_page(
        &mut doc,
        pages,
        400,
        "q 400 0 0 400 0 0 cm /Im Do Q",
        dictionary! { "Im" => picture },
    );
    finish(doc, pages, vec![kid], &path);

    let out = one(&run("pdf.compress", &[path], json!({ "dpi": 72 })));

    let images = images_in(&out.path);
    let mask = images.iter().find(|i| !i.dict.has(b"SMask")).unwrap();
    assert_eq!(mask.dict.get(b"Width").unwrap().as_i64().unwrap(), 500);
    assert!(mask.dict.get(b"Filter").unwrap().as_name().unwrap() != b"DCTDecode");
    assert!(images.iter().any(|i| i.dict.has(b"SMask")), "the picture lost its mask");
}

#[test]
fn compress_keeps_an_image_that_would_only_grow() {
    let dir = tempfile::tempdir().unwrap();
    let image = DynamicImage::ImageRgb8(noise(600, 600));
    let original = encode(&image, None, Format::Jpeg, 20).unwrap();
    let mut dict = image_dict(600, 600, "DeviceRGB");
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    let mut stream = Stream::new(dict, original.clone());
    stream.allows_compression = false;
    let src = image_pdf(dir.path(), "small.pdf", stream);

    let out = one(&run("pdf.compress", &[src], json!({ "dpi": 600, "quality": 100 })));

    assert_eq!(images_in(&out.path)[0].content, original);
}

#[test]
fn compress_never_returns_a_bigger_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "plain.pdf", 1);
    let first = one(&run("pdf.compress", &[src], json!({})));
    let again = one(&run("pdf.compress", std::slice::from_ref(&first.path), json!({})));
    assert!(again.size <= again.source_size.unwrap());
    assert!(again.note.unwrap().contains("left as it is"));
}

#[test]
fn draws_pages_as_images() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    let (done, progress) = run_with_progress("pdf.to-images", &[src], json!({ "dpi": 144 }));
    let out = results(&done);
    assert_eq!(
        out.iter().map(name).collect::<Vec<_>>(),
        ["doc-1.png", "doc-2.png", "doc-3.png"]
    );
    let page = image::open(&out[1].path).unwrap();
    assert_eq!(page.dimensions(), (404, 800));
    assert_eq!(
        page.to_rgb8().get_pixel(50, 750).0,
        [255, 0, 0],
        "the red square at the bottom left"
    );
    assert_eq!(page.to_rgb8().get_pixel(300, 100).0, [255, 255, 255]);
    assert!(progress.iter().any(|p| *p > 0.0 && *p < 1.0), "{progress:?}");
}

#[test]
fn draws_pages_as_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 1);
    let done = run("pdf.to-images", &[src], json!({ "format": "jpg" }));
    let out = one(&done);
    assert_eq!(name(&out), "doc-1.jpg");
    assert_eq!(&fs::read(&out.path).unwrap()[..2], [0xFF, 0xD8]);
}

#[test]
fn a_damaged_file_table_is_repaired_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    let bytes = fs::read(&src).unwrap();
    let at = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    let mut broken = bytes[..at].to_vec();
    broken.extend_from_slice(b"startxref\n7\n%%EOF\n");
    fs::write(&src, broken).unwrap();

    let done = run("pdf.rotate-pages", &[src], json!({}));

    assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
    assert_eq!(widths(&one(&done).path), [201, 202, 203]);
}

#[test]
fn a_file_that_is_not_a_pdf_gets_a_clear_error_from_every_tool() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.pdf");
    fs::write(&bad, "this is not a pdf").unwrap();
    let tools = [
        ("pdf.split", json!({})),
        ("pdf.compress", json!({})),
        ("pdf.metadata.remove", json!({})),
        ("pdf.rotate-pages", json!({})),
        ("pdf.to-images", json!({})),
        ("pdf.password.add", json!({ "password": "x" })),
    ];
    for (tool, params) in tools {
        let done = run(tool, std::slice::from_ref(&bad), params);
        assert_eq!(done.status, JobStatus::Failed, "{tool}");
        let e = done.error.unwrap();
        assert_eq!(e.kind, ErrorKind::UnsupportedFormat, "{tool}: {e:?}");
        assert!(e.message.starts_with("bad.pdf: "), "{tool}: {}", e.message);
    }
    assert_eq!(entries(dir.path()), ["bad.pdf"]);
}

#[test]
fn a_truncated_pdf_is_repaired_or_refused_but_never_crashes() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 3);
    let bytes = fs::read(&src).unwrap();
    for keep in [bytes.len() / 4, bytes.len() / 2, bytes.len() - 20] {
        fs::write(&src, &bytes[..keep]).unwrap();
        for tool in ["pdf.compress", "pdf.split", "pdf.to-images"] {
            let done = run(tool, std::slice::from_ref(&src), json!({}));
            assert!(
                matches!(done.status, JobStatus::Completed | JobStatus::Failed),
                "{tool}"
            );
            if let Some(e) = done.error {
                assert_ne!(e.kind, ErrorKind::Unknown, "{tool}: {e:?}");
            }
        }
    }
}

#[test]
fn file_names_are_never_interpreted() {
    let dir = tempfile::tempdir().unwrap();
    for file in [
        "--decrypt.pdf",
        "@args.pdf",
        "{stem}.pdf",
        "a b.pdf",
        "файл.pdf",
        ".pdf",
    ] {
        let src = pdf(dir.path(), "plain.pdf", 2);
        let tricky = dir.path().join(file);
        fs::rename(src, &tricky).unwrap();
        let done = run("pdf.rotate-pages", &[tricky], json!({}));
        assert_eq!(done.status, JobStatus::Completed, "{file}: {:?}", done.error);
    }
}

#[test]
fn never_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = pdf(dir.path(), "doc.pdf", 2);
    fs::write(dir.path().join("doc-1.pdf"), "mine").unwrap();
    let out = results(&run("pdf.split", &[src], json!({})));
    assert_eq!(name(&out[0]), "doc-1 (2).pdf");
    assert_eq!(fs::read(dir.path().join("doc-1.pdf")).unwrap(), b"mine");
}

#[test]
fn one_bad_file_removes_the_results_of_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let good = pdf(dir.path(), "good.pdf", 2);
    let bad = dir.path().join("bad.pdf");
    fs::write(&bad, "nope").unwrap();
    let done = run("pdf.split", &[good, bad], json!({}));
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(entries(dir.path()), ["bad.pdf", "good.pdf"]);
}

fn photo(dir: &Path, file: &str, w: u32, h: u32, format: ImageFormat) -> PathBuf {
    let path = dir.join(file);
    DynamicImage::ImageRgb8(noise(w, h))
        .save_with_format(&path, format)
        .unwrap();
    path
}

#[test]
fn builds_a_pdf_from_images_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let a = photo(dir.path(), "a.png", 400, 300, ImageFormat::Png);
    let b = photo(dir.path(), "b.jpg", 200, 300, ImageFormat::Jpeg);

    let done = run("pdf.from-images", &[a, b], json!({}));
    let out = one(&done);

    assert_eq!(name(&out), "a.pdf");
    check(&out.path, None);
    let doc = Document::load(&out.path).unwrap();
    let sizes: Vec<f32> = pages_of(&doc)
        .values()
        .map(|d| d.get(b"MediaBox").unwrap().as_array().unwrap()[2].as_float().unwrap())
        .collect();
    assert_eq!(sizes, [300.0, 150.0]);
}

#[test]
fn a_jpeg_goes_into_the_pdf_without_being_encoded_again() {
    let dir = tempfile::tempdir().unwrap();
    let src = photo(dir.path(), "a.jpg", 200, 300, ImageFormat::Jpeg);
    let original = fs::read(&src).unwrap();
    let out = one(&run("pdf.from-images", &[src], json!({})));
    assert_eq!(images_in(&out.path)[0].content, original);
}

/// A JPEG whose EXIF says it must be turned a quarter to the right.
fn turned_jpeg(dir: &Path) -> PathBuf {
    let plain = fs::read(photo(dir, "plain.jpg", 200, 100, ImageFormat::Jpeg)).unwrap();
    let mut exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec();
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1];
    bytes.extend(((exif.len() + 2) as u16).to_be_bytes());
    bytes.append(&mut exif);
    bytes.extend_from_slice(&plain[2..]);
    let path = dir.join("turned.jpg");
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_turned_photo_stays_upright_in_the_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let out = one(&run("pdf.from-images", &[turned_jpeg(dir.path())], json!({})));
    let image = &images_in(&out.path)[0];
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 100);
    assert_eq!(image.dict.get(b"Height").unwrap().as_i64().unwrap(), 200);
    let page = rendered(&out.path, 96).to_rgb8();
    assert_eq!(page.dimensions(), (100, 200));
    // Red grows from left to right in the file, so top to bottom once turned.
    let (top, bottom) = (page.get_pixel(50, 5)[0], page.get_pixel(50, 195)[0]);
    assert!(bottom > top.saturating_add(100), "top {top}, bottom {bottom}");
}

#[test]
fn images_on_paper_are_centered_and_transparency_turns_white() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("logo.png");
    let clear = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 0]));
    DynamicImage::ImageRgba8(clear).save(&src).unwrap();

    let out = one(&run("pdf.from-images", &[src], json!({ "page_size": "a4" })));

    let page = rendered(&out.path, 72).to_rgb8();
    assert_eq!(page.dimensions().0, 595);
    assert_eq!(page.get_pixel(297, 420).0, [255, 255, 255]);
}

#[test]
fn a_broken_image_stops_the_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let good = photo(dir.path(), "good.png", 40, 40, ImageFormat::Png);
    let bad = dir.path().join("bad.png");
    fs::write(&bad, "nope").unwrap();
    let done = run("pdf.from-images", &[good, bad], json!({}));
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(entries(dir.path()), ["bad.png", "good.png"]);
}

/// What PDFium reads from a PDF.
fn text_of(path: &Path) -> String {
    let done = run("pdf.to-text", &[path.to_path_buf()], json!({}));
    fs::read_to_string(one(&done).path).unwrap()
}

/// A picture of the first page.
fn picture_of(path: &Path) -> image::RgbImage {
    let done = run(
        "pdf.to-images",
        &[path.to_path_buf()],
        json!({ "format": "png", "dpi": 72 }),
    );
    image::open(&results(&done)[0].path).unwrap().to_rgb8()
}

fn turned_page(dir: &Path, angle: i64) -> PathBuf {
    let path = dir.join(format!("turned-{angle}.pdf"));
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 400.into()],
        "Rotate" => angle,
    });
    finish(doc, pages, vec![page.into()], &path);
    path
}

#[test]
fn numbers_pages() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 3);
    let done = run(
        "pdf.page-numbers",
        &[source],
        json!({ "style": "of-total", "start_at": 5 }),
    );
    let out = one(&done);
    assert_eq!(name(&out), "a-numbered.pdf");
    let text = text_of(&out.path);
    for expected in ["5 / 7", "6 / 7", "7 / 7"] {
        assert!(text.contains(expected), "{text}");
    }
}

#[test]
fn a_page_number_lands_where_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    for (angle, position) in [
        (0, "top-left"),
        (90, "top-left"),
        (180, "top-left"),
        (270, "top-left"),
        (90, "bottom-right"),
    ] {
        let source = turned_page(dir.path(), angle);
        let done = run(
            "pdf.page-numbers",
            &[source],
            json!({ "position": position, "size": 40 }),
        );
        let page = picture_of(&one(&done).path);
        let (w, h) = page.dimensions();
        let ink = page.enumerate_pixels().filter(|(_, _, p)| p.0[0] < 128);
        let (sum_x, sum_y, count) = ink.fold((0u64, 0u64, 0u64), |(x, y, n), (px, py, _)| {
            (x + u64::from(px), y + u64::from(py), n + 1)
        });
        assert!(count > 0, "nothing printed at {angle}");
        let (x, y) = ((sum_x / count) as u32, (sum_y / count) as u32);
        let left = x < w / 2;
        let top = y < h / 2;
        assert_eq!(
            (left, top),
            (position.ends_with("left"), position.starts_with("top")),
            "{angle} {position}: {x},{y} of {w}x{h}"
        );
    }
}

#[test]
fn stamps_keep_the_old_content_and_survive_a_changed_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scaled.pdf");
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    // Leaves a scale behind that would throw a careless stamp off the page.
    let page = add_page(
        &mut doc,
        pages,
        300,
        "1 0 0 rg 0 0 50 50 re f 0.01 0 0 0.01 0 0 cm",
        Dictionary::new(),
    );
    finish(doc, pages, vec![page], &path);

    let done = run("pdf.watermark", &[path], json!({ "text": "DRAFT", "opacity": 100 }));
    let out = one(&done);
    assert!(text_of(&out.path).contains("DRAFT"));
    let picture = picture_of(&out.path);
    let (_, h) = picture.dimensions();
    assert_eq!(picture.get_pixel(5, h - 5).0, [255, 0, 0], "the old content is gone");
    let gray = picture.pixels().filter(|p| p.0[0] == p.0[1] && p.0[0] < 200).count();
    assert!(gray > 200, "the stamp is not on the page: {gray}");
}

#[test]
fn a_watermark_can_be_faint() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run(
        "pdf.watermark",
        &[source],
        json!({ "text": "HHHH", "opacity": 10, "size": 80 }),
    );
    let picture = picture_of(&one(&done).path);
    let darkest = picture
        .pixels()
        .filter(|p| p.0[1] == p.0[2] && p.0[0] == p.0[1])
        .map(|p| p.0[0])
        .min()
        .unwrap();
    assert!(darkest > 200, "{darkest}");
}

#[test]
fn explains_text_the_base_font_cannot_print() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.watermark", &[source], json!({ "text": "Привет" }));
    let error = done.error.unwrap();
    assert_eq!(error.kind, ErrorKind::InvalidInput);
    assert!(error.message.contains("Western European"), "{}", error.message);
}

#[test]
fn escapes_brackets_in_a_watermark() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.watermark", &[source], json!({ "text": r"a(b)c\d" }));
    assert!(text_of(&one(&done).path).contains(r"a(b)c\d"));
}

#[test]
fn crops_the_visible_page() {
    let dir = tempfile::tempdir().unwrap();
    let source = turned_page(dir.path(), 0);
    let params = json!({ "top": 10.0, "right": 0.0, "bottom": 20.0, "left": 5.0 });
    let done = run("pdf.crop", &[source], params);
    let doc = Document::load(one(&done).path).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let view = saqviro_tools::pdf::stamp::PageView::of(&doc, page).unwrap();
    let mm = 72.0 / 25.4;
    assert!(
        (view.left - 5.0 * mm).abs() < 0.1 && (view.bottom - 20.0 * mm).abs() < 0.1,
        "{view:?}"
    );
    assert!(
        (view.right - 300.0).abs() < 0.1 && (view.top - (400.0 - 10.0 * mm)).abs() < 0.1,
        "{view:?}"
    );
}

#[test]
fn crop_margins_follow_the_page_as_shown() {
    let dir = tempfile::tempdir().unwrap();
    let source = turned_page(dir.path(), 90);
    let done = run(
        "pdf.crop",
        &[source],
        json!({ "top": 10.0, "right": 0.0, "bottom": 0.0, "left": 0.0 }),
    );
    let doc = Document::load(one(&done).path).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let view = saqviro_tools::pdf::stamp::PageView::of(&doc, page).unwrap();
    // The top of a page turned 90° clockwise is its left side.
    assert!(view.left > 20.0 && view.bottom == 0.0, "{view:?}");
}

#[test]
fn refuses_margins_that_leave_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.crop", &[source], json!({ "left": 500.0, "right": 500.0 }));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn sets_properties_in_any_language() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 2);
    let done = run(
        "pdf.metadata.edit",
        std::slice::from_ref(&source),
        json!({ "title": "Отчёт", "author": "Ann" }),
    );
    let out = one(&done);
    let info = run("pdf.info", std::slice::from_ref(&out.path), json!({}));
    let Some(JobOutput::Text(text)) = info.output else {
        panic!("{:?}", info.error)
    };
    assert!(text.contains("Title: Отчёт"), "{text}");
    assert!(text.contains("Author: Ann"), "{text}");
    assert!(text.contains("Pages: 2"), "{text}");

    let done = run("pdf.metadata.edit", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn describes_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let source = turned_page(dir.path(), 90);
    let info = run("pdf.info", &[source], json!({}));
    let Some(JobOutput::Text(text)) = info.output else {
        panic!("{:?}", info.error)
    };
    assert!(text.starts_with("turned-90.pdf\nPages: 1\n"), "{text}");
    assert!(text.contains("Page size: 141 × 106 mm"), "{text}");
    assert!(text.contains("Author: AUTHORSECRET"), "{text}");
}

#[test]
fn repairs_a_pdf_with_a_broken_index() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 2);
    let mut bytes = fs::read(&source).unwrap();
    let at = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    bytes[at + 10..at + 13].copy_from_slice(b"999");
    fs::write(&source, &bytes).unwrap();

    let done = run("pdf.repair", &[source], json!({}));
    assert_eq!(done.error, None);
    let out = one(&done);
    assert_eq!(name(&out), "a-repaired.pdf");
    let count = Command::new("qpdf")
        .args(["--show-npages"])
        .arg(&out.path)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");
}

#[test]
fn does_not_call_a_hopeless_pdf_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 2);
    let bytes = fs::read(&source).unwrap();
    let at = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    fs::write(&source, &bytes[..at]).unwrap();

    let done = run("pdf.repair", &[source], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::UnsupportedFormat);
    assert_eq!(entries(dir.path()), ["a.pdf"]);
}

#[test]
fn a_pdf_without_text_is_called_out() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.to-text", &[source], json!({}));
    let out = one(&done);
    assert_eq!(name(&out), "a.txt");
    assert!(out.note.unwrap().contains("scanned"));
}

#[test]
fn saves_the_pictures_inside_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let picture = dir.path().join("p.png");
    noise(40, 30).save(&picture).unwrap();
    let done = run("pdf.from-images", &[picture], json!({}));
    let source = one(&done).path;

    let done = run("pdf.extract-images", &[source], json!({}));
    assert_eq!(done.error, None);
    let folder = one(&done).path;
    let found = entries(&folder);
    assert_eq!(found, ["page-1-1.png"]);
    assert_eq!(image::image_dimensions(folder.join(&found[0])).unwrap(), (40, 30));
}

#[test]
fn says_when_a_pdf_has_no_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 1);
    let done = run("pdf.extract-images", &[source], json!({}));
    assert!(done.error.unwrap().message.contains("no images"));
    assert_eq!(entries(dir.path()), ["a.pdf"]);
}

#[test]
fn draws_small_pictures_of_the_pages() {
    let dir = tempfile::tempdir().unwrap();
    let source = pdf(dir.path(), "a.pdf", 3);
    let temp = tempfile::tempdir().unwrap();
    let all = saqviro_tools::pdf::thumbnails(&source, temp.path(), 2).unwrap();
    assert_eq!(all.count, 3);
    assert_eq!(all.pages.len(), 2);
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&all.pages[0]).unwrap();
    assert!(image::load_from_memory(&bytes).unwrap().width() < 150);
}

#[test]
fn a_pdf_that_cannot_be_drawn_has_no_thumbnails() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.pdf");
    fs::write(&source, "not a pdf").unwrap();
    let temp = tempfile::tempdir().unwrap();
    assert!(saqviro_tools::pdf::thumbnails(&source, temp.path(), 2).is_err());
}
