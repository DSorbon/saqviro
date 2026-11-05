//! File and QR tools, run the way the app runs them.

use std::{
    fs,
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use image::{DynamicImage, GenericImage, GrayImage, ImageFormat, Luma};
use saqviro_core::registry::Registry;
use saqviro_jobs::{
    Config, ErrorKind, JobContext, JobError, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, OutputFile,
    Params,
};
use saqviro_tools::{
    files::{
        archive::{LIMITS, Limits, unpack},
        checksum::parse_expected,
        rename::{Case, Rule, apply, plan},
    },
    qr::read_codes,
};
use serde_json::{Value, json};
use zip::{ZipWriter, write::SimpleFileOptions};

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

fn manager() -> (JobManager, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    (JobManager::new(config, |_: &JobSnapshot| {}).unwrap(), root)
}

fn run_input(tool_id: &str, input: JobInput, params: Value) -> JobSnapshot {
    let registry = registry();
    let tool = registry.get(tool_id).unwrap();
    let params = tool.resolve_params(params.as_object().unwrap()).unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    let (jobs, _root) = manager();
    jobs.wait(jobs.submit(tool_id, exec, input, params)).unwrap()
}

fn run(tool_id: &str, files: &[PathBuf], params: Value) -> JobSnapshot {
    run_input(tool_id, JobInput::Files(files.to_vec()), params)
}

fn one(done: &JobSnapshot) -> OutputFile {
    match &done.output {
        Some(JobOutput::Files(files)) if files.len() == 1 => files[0].clone(),
        other => panic!("expected one file, got {other:?} / {:?}", done.error),
    }
}

fn text(done: &JobSnapshot) -> String {
    match &done.output {
        Some(JobOutput::Text(text)) => text.clone(),
        other => panic!("expected text, got {other:?} / {:?}", done.error),
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

/// Builds an archive the way an attacker would, with raw control of names.
fn archive(dir: &Path, name: &str, build: impl FnOnce(&mut ZipWriter<fs::File>)) -> PathBuf {
    let path = dir.join(name);
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    build(&mut zip);
    zip.finish().unwrap();
    path
}

fn add(zip: &mut ZipWriter<fs::File>, name: &str, content: &[u8]) {
    zip.start_file(name, SimpleFileOptions::default()).unwrap();
    zip.write_all(content).unwrap();
}

#[test]
fn zips_files_and_folders_and_gets_them_back() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "alpha");
    write(dir.path(), "docs/b.txt", "beta");
    write(dir.path(), "docs/deep/c.txt", "gamma");
    fs::create_dir(dir.path().join("docs/empty")).unwrap();

    let done = run("files.zip.create", &[a, dir.path().join("docs")], json!({}));
    let zip = one(&done);
    assert_eq!(zip.path.file_name().unwrap(), "a.zip");
    assert_eq!(zip.source_size, Some(14));

    fs::remove_file(dir.path().join("a.txt")).unwrap();
    let out = one(&run("files.zip.extract", &[zip.path], json!({})));
    assert_eq!(out.path.file_name().unwrap(), "a");
    assert_eq!(fs::read_to_string(out.path.join("a.txt")).unwrap(), "alpha");
    assert_eq!(fs::read_to_string(out.path.join("docs/deep/c.txt")).unwrap(), "gamma");
    assert!(out.path.join("docs/empty").is_dir());
    assert_eq!(out.size, 14);
}

#[test]
fn compression_levels_differ() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(dir.path(), "big.txt", &"saqviro ".repeat(20_000));
    let stored = one(&run(
        "files.zip.create",
        std::slice::from_ref(&src),
        json!({ "level": 0 }),
    ));
    let packed = one(&run("files.zip.create", &[src], json!({ "level": 9 })));
    assert!(packed.size * 20 < stored.size, "{} vs {}", packed.size, stored.size);
}

#[test]
fn two_inputs_with_one_name_both_survive() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "x/same.txt", "one");
    let b = write(dir.path(), "y/same.txt", "two");
    let zip = one(&run("files.zip.create", &[a, b], json!({})));
    let out = one(&run("files.zip.extract", &[zip.path], json!({})));
    assert_eq!(entries(&out.path), ["same (2).txt", "same.txt"]);
}

#[cfg(unix)]
#[test]
fn links_are_not_followed_into_an_archive() {
    let dir = tempfile::tempdir().unwrap();
    let secret = write(dir.path(), "outside/secret.txt", "SECRET");
    write(dir.path(), "pack/ok.txt", "ok");
    std::os::unix::fs::symlink(&secret, dir.path().join("pack/link.txt")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("pack/folder")).unwrap();

    let zip = one(&run("files.zip.create", &[dir.path().join("pack")], json!({})));
    let out = one(&run("files.zip.extract", &[zip.path], json!({})));
    assert_eq!(entries(&out.path.join("pack")), ["ok.txt"]);
}

#[test]
fn an_empty_folder_makes_no_archive() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let done = run("files.zip.create", &[dir.path().join("empty")], json!({}));
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn extracting_never_touches_an_existing_folder() {
    let dir = tempfile::tempdir().unwrap();
    let zip = archive(dir.path(), "a.zip", |z| add(z, "new.txt", b"new"));
    write(dir.path(), "a/mine.txt", "mine");
    let out = one(&run("files.zip.extract", &[zip], json!({})));
    assert_eq!(out.path.file_name().unwrap(), "a (2)");
    assert_eq!(entries(&dir.path().join("a")), ["mine.txt"]);
}

#[test]
fn an_entry_cannot_climb_out_of_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let box_dir = dir.path().join("box");
    fs::create_dir(&box_dir).unwrap();
    // An absolute name aimed inside this test's folder, so that a failure
    // here cannot leave anything behind on the machine.
    let absolute = dir.path().join("evil.txt").display().to_string();
    for name in ["../evil.txt", "a/../../evil.txt", absolute.as_str(), "..\\evil.txt"] {
        let zip = archive(&box_dir, "a.zip", |z| add(z, name, b"evil"));
        run("files.zip.extract", &[zip], json!({}));
        assert!(!dir.path().join("evil.txt").exists(), "{name} escaped");
    }
}

#[test]
fn a_traversal_entry_fails_the_job_and_removes_what_was_unpacked() {
    let dir = tempfile::tempdir().unwrap();
    let zip = archive(dir.path(), "a.zip", |z| {
        add(z, "fine.txt", b"fine");
        add(z, "../evil.txt", b"evil");
    });
    let done = run("files.zip.extract", &[zip], json!({}));
    assert_eq!(done.status, JobStatus::Failed);
    assert!(done.error.unwrap().message.contains("outside"));
    assert_eq!(entries(dir.path()), ["a.zip"]);
}

#[test]
fn an_entry_cannot_replace_one_unpacked_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let zip = archive(dir.path(), "a.zip", |z| {
        add(z, "report.txt", b"the real one");
        add(z, "./report.txt", b"the swapped one");
    });
    let done = run("files.zip.extract", &[zip], json!({}));
    assert_eq!(done.status, JobStatus::Failed, "{:?}", done.output);
    assert!(done.error.unwrap().message.contains("twice"));
    assert_eq!(entries(dir.path()), ["a.zip"]);
}

#[test]
fn links_in_an_archive_are_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let zip = archive(dir.path(), "a.zip", |z| {
        z.add_symlink("link", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        add(z, "ok.txt", b"ok");
    });
    let out = one(&run("files.zip.extract", &[zip], json!({})));
    assert_eq!(entries(&out.path), ["ok.txt"]);
    assert!(out.note.unwrap().contains("links"));
}

/// Runs `unpack` inside a job, with its own limits.
fn unpack_with(path: PathBuf, target: PathBuf, limits: Limits) -> JobSnapshot {
    let (jobs, _root) = manager();
    let exec = Arc::new(move |ctx: &JobContext, _: JobInput, _: &Params| {
        let file = fs::File::open(&path)?;
        let size = file.metadata()?.len();
        unpack(ctx, file, size, &target, limits, |_| {})?;
        Ok::<_, JobError>(JobOutput::Text(String::new()))
    });
    jobs.wait(jobs.submit("t", exec, JobInput::Text(String::new()), Params::new()))
        .unwrap()
}

#[test]
fn a_zip_bomb_is_stopped_by_what_it_really_writes() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");
    fs::create_dir(&target).unwrap();
    let zip = archive(dir.path(), "bomb.zip", |z| add(z, "zeros", &vec![0; 8 * 1024 * 1024]));
    let limits = Limits {
        ratio: 10,
        ratio_floor: 1024,
        ..LIMITS
    };
    let done = unpack_with(zip, target, limits);
    assert_eq!(done.status, JobStatus::Failed);
    assert!(done.error.unwrap().message.contains("expands"));
}

#[test]
fn too_many_entries_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");
    fs::create_dir(&target).unwrap();
    let zip = archive(dir.path(), "many.zip", |z| {
        for i in 0..5 {
            add(z, &format!("{i}.txt"), b"x");
        }
    });
    let limits = Limits { entries: 4, ..LIMITS };
    assert!(
        unpack_with(zip, target, limits)
            .error
            .unwrap()
            .message
            .contains("too many")
    );
}

#[test]
fn a_corrupted_archive_fails_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let zip = archive(dir.path(), "a.zip", |z| {
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file("data.txt", stored).unwrap();
        z.write_all(b"AAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
    });
    let mut bytes = fs::read(&zip).unwrap();
    let at = bytes.windows(4).position(|w| w == b"AAAA").unwrap();
    bytes[at] = b'B';
    fs::write(&zip, bytes).unwrap();

    let done = run("files.zip.extract", &[zip], json!({}));

    assert_eq!(done.status, JobStatus::Failed, "{:?}", done.output);
    assert_eq!(entries(dir.path()), ["a.zip"]);
}

#[test]
fn a_file_that_is_not_a_zip_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bad = write(dir.path(), "bad.zip", "not a zip");
    let e = run("files.zip.extract", &[bad], json!({})).error.unwrap();
    assert_eq!(e.kind, ErrorKind::UnsupportedFormat);
    assert!(e.message.starts_with("bad.zip: "), "{}", e.message);
    assert_eq!(entries(dir.path()), ["bad.zip"]);
}

#[test]
fn checksums_match_a_known_value() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "abc");
    let b = write(dir.path(), "b.txt", "");
    let out = text(&run("files.sha256", &[a, b], json!({})));
    assert_eq!(
        out,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  a.txt\n\
         e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  b.txt"
    );
}

#[test]
fn folders_have_no_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let e = run("files.sha256", &[dir.path().to_path_buf()], json!({}))
        .error
        .unwrap();
    assert!(e.message.contains("folders"), "{}", e.message);
}

#[test]
fn verifies_a_checksum_with_the_algorithm_its_length_tells() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "abc");
    let good = [
        "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  a.txt",
        "900150983cd24fb0d6963f7d28e17f72",
    ];
    for expected in good {
        let out = text(&run(
            "files.checksum.verify",
            std::slice::from_ref(&a),
            json!({ "expected": expected }),
        ));
        assert!(out.starts_with("✓ a.txt matches"), "{expected}: {out}");
    }
}

#[test]
fn a_mismatch_is_an_answer_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "abd");
    let expected = "900150983cd24fb0d6963f7d28e17f72";
    let done = run("files.checksum.verify", &[a], json!({ "expected": expected }));
    assert_eq!(done.status, JobStatus::Completed);
    let out = text(&done);
    assert!(out.contains("does NOT match") && out.contains(expected), "{out}");
}

#[test]
fn rejects_what_is_not_a_checksum() {
    assert!(parse_expected("").is_err());
    assert!(parse_expected("xyz").unwrap_err().contains("0-9"));
    assert!(parse_expected("abc").unwrap_err().contains("3 characters"));
}

fn rule(find: &str, replace: &str) -> Rule {
    Rule {
        find: find.into(),
        replace: replace.into(),
        ..Rule::default()
    }
}

#[test]
fn rules_change_the_name_and_keep_the_extension() {
    let r = Rule {
        prefix: "new-".into(),
        suffix: "-v2".into(),
        ..rule("old", "fresh")
    };
    assert_eq!(
        r.apply("old photo.old.jpg", 0, 1).unwrap(),
        "new-fresh photo.fresh-v2.jpg"
    );
    assert_eq!(rule("jpg", "png").apply("a.jpg", 0, 1).unwrap(), "a.jpg");
    assert_eq!(rule("env", "x").apply(".env", 0, 1).unwrap(), ".x");
}

#[test]
fn numbers_are_padded_to_the_longest() {
    let r = Rule {
        number_from: Some(9),
        ..Rule::default()
    };
    assert_eq!(r.apply("a.jpg", 0, 3).unwrap(), "a-09.jpg");
    assert_eq!(r.apply("a.jpg", 2, 3).unwrap(), "a-11.jpg");
}

#[test]
fn case_applies_to_the_whole_name() {
    let r = Rule {
        case: Case::Lower,
        ..Rule::default()
    };
    assert_eq!(r.apply("IMG 1.JPG", 0, 1).unwrap(), "img 1.jpg");
}

#[test]
fn names_that_cannot_exist_are_refused() {
    for replace in ["a/b", "a\\b", "..\\x", "a:b", "a\0b", "a\nb"] {
        assert!(rule("x", replace).apply("x.txt", 0, 1).is_err(), "{replace:?}");
    }
    assert!(
        rule("x", "")
            .apply("x.txt", 0, 1)
            .unwrap_err()
            .contains("without a name")
    );
}

#[test]
fn preview_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "");
    let out = text(&run(
        "files.rename",
        &[a],
        json!({ "find": "a", "replace": "b", "preview": true }),
    ));
    assert_eq!(out, "a.txt → b.txt");
    assert_eq!(entries(dir.path()), ["a.txt"]);
}

#[test]
fn the_tool_previews_unless_told_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "");
    run("files.rename", &[a], json!({ "find": "a", "replace": "b" }));
    assert_eq!(entries(dir.path()), ["a.txt"]);
}

#[test]
fn renames_and_says_what_each_file_was() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "alpha");
    let done = run("files.rename", &[a], json!({ "prefix": "x-", "preview": false }));
    let out = one(&done);
    assert_eq!(entries(dir.path()), ["x-a.txt"]);
    assert_eq!(fs::read_to_string(&out.path).unwrap(), "alpha");
    assert_eq!(out.note.unwrap(), "Was a.txt");
}

#[test]
fn an_existing_file_stops_the_whole_batch_before_anything_moves() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a1.txt", "one");
    let b = write(dir.path(), "a2.txt", "two");
    write(dir.path(), "b2.txt", "mine");

    let done = run(
        "files.rename",
        &[a, b],
        json!({ "find": "a", "replace": "b", "preview": false }),
    );

    assert!(done.error.unwrap().message.contains("b2.txt"));
    assert_eq!(entries(dir.path()), ["a1.txt", "a2.txt", "b2.txt"]);
    assert_eq!(fs::read_to_string(dir.path().join("b2.txt")).unwrap(), "mine");
}

#[test]
fn two_files_cannot_end_up_with_one_name() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "");
    let b = write(dir.path(), "A1.txt", "");
    let e = plan(&[a, b], &rule("1", "")).unwrap_err();
    assert!(e.contains("two files"), "{e}");
}

#[test]
fn names_can_swap_inside_a_batch() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "1.txt", "was one");
    let b = write(dir.path(), "2.txt", "was two");
    // 1 becomes 2 while 2 becomes 3 is a chain through a taken name.
    let moves = plan(
        &[a.clone(), b.clone()],
        &Rule {
            suffix: "x".into(),
            ..Rule::default()
        },
    )
    .unwrap();
    assert_eq!(moves.len(), 2);

    let swap = vec![(a.clone(), b.clone()), (b.clone(), a.clone())];
    apply(&swap, |from, to| fs::rename(from, to)).unwrap();
    assert_eq!(fs::read_to_string(&a).unwrap(), "was two");
    assert_eq!(fs::read_to_string(&b).unwrap(), "was one");
}

#[test]
fn only_the_case_can_change() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "photo.jpg", "x");
    let upper = Rule {
        case: Case::Upper,
        ..Rule::default()
    };
    let done = run("files.rename", &[a], json!({ "case": "upper", "preview": false }));
    assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
    assert_eq!(entries(dir.path()), ["PHOTO.JPG"]);
    assert_eq!(upper.apply("photo.jpg", 0, 1).unwrap(), "PHOTO.JPG");
}

#[test]
fn a_failure_in_the_middle_puts_every_name_back() {
    for fail_at in 0..6 {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["a.txt", "b.txt", "c.txt"]
            .iter()
            .map(|n| write(dir.path(), n, n))
            .collect();
        let moves = plan(
            &files,
            &Rule {
                prefix: "x-".into(),
                ..Rule::default()
            },
        )
        .unwrap();

        let calls = AtomicUsize::new(0);
        let result = apply(&moves, |from, to| {
            if calls.fetch_add(1, Ordering::SeqCst) == fail_at {
                return Err(io::Error::other("disk went away"));
            }
            fs::rename(from, to)
        });

        assert!(result.is_err());
        assert_eq!(
            entries(dir.path()),
            ["a.txt", "b.txt", "c.txt"],
            "failed at step {fail_at}"
        );
        assert_eq!(fs::read_to_string(&files[1]).unwrap(), "b.txt");
    }
}

#[test]
fn a_rule_that_does_nothing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.txt", "");
    assert!(plan(&[a], &Rule::default()).unwrap_err().contains("choose"));
}

fn generated(params: Value, input: &str) -> (Vec<u8>, String) {
    let done = run_input("qr.generate", JobInput::Text(input.into()), params);
    let Some(JobOutput::Image(image)) = done.output else {
        panic!("{:?}", done.error);
    };
    use base64::Engine;
    (
        base64::engine::general_purpose::STANDARD.decode(image.data).unwrap(),
        image.ext,
    )
}

#[test]
fn a_generated_code_scans_back() {
    for input in ["https://example.com/?q=1&b=2", "привет, мир 👋", "x"] {
        let (bytes, ext) = generated(json!({}), input);
        assert_eq!(ext, "png");
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!(read_codes(&image), [input]);
    }
}

#[test]
fn the_size_and_correction_are_used() {
    let (small, _) = generated(json!({ "size": 128, "correction": "low" }), "hello");
    let (large, _) = generated(json!({ "size": 1024, "correction": "high" }), "hello");
    let (small, large) = (
        image::load_from_memory(&small).unwrap(),
        image::load_from_memory(&large).unwrap(),
    );
    assert!(
        small.width() <= 128 && large.width() > 900,
        "{} {}",
        small.width(),
        large.width()
    );
    assert_eq!(read_codes(&large), ["hello"]);
}

#[test]
fn makes_svg() {
    let (bytes, ext) = generated(json!({ "format": "svg" }), "hello");
    assert_eq!(ext, "svg");
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<svg") && !svg.contains("<script"), "{}", &svg[..80]);
}

#[test]
fn text_too_long_for_a_code_is_refused() {
    let done = run_input(
        "qr.generate",
        JobInput::Text("x".repeat(5000)),
        json!({}).as_object().map(|_| json!({})).unwrap(),
    );
    assert!(done.error.unwrap().message.contains("too long"));
}

#[test]
fn scans_a_code_inside_a_larger_picture_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let (bytes, _) = generated(json!({ "size": 300 }), "inside");
    let code = image::load_from_memory(&bytes).unwrap().to_luma8();
    let mut canvas = GrayImage::from_pixel(900, 700, Luma([235]));
    canvas.copy_from(&code, 250, 180).unwrap();
    let path = dir.path().join("photo.png");
    DynamicImage::ImageLuma8(canvas)
        .save_with_format(&path, ImageFormat::Png)
        .unwrap();

    assert_eq!(text(&run("qr.scan", &[path], json!({}))), "inside");
}

#[test]
fn scans_a_turned_code() {
    let (bytes, _) = generated(json!({}), "turned");
    let image = image::load_from_memory(&bytes).unwrap().rotate90();
    assert_eq!(read_codes(&image), ["turned"]);
}

#[test]
fn says_when_there_is_no_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.png");
    DynamicImage::ImageLuma8(GrayImage::from_pixel(200, 200, Luma([128])))
        .save(&path)
        .unwrap();
    let e = run("qr.scan", &[path], json!({})).error.unwrap();
    assert_eq!(e.message, "plain.png: no QR code was found in this image");
}

#[test]
fn several_images_are_labelled() {
    let dir = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for name in ["one", "two"] {
        let (bytes, _) = generated(json!({}), name);
        let path = dir.path().join(format!("{name}.png"));
        let mut cursor = Cursor::new(bytes);
        fs::write(&path, cursor.get_mut()).unwrap();
        files.push(path);
    }
    assert_eq!(text(&run("qr.scan", &files, json!({}))), "one.png\none\n\ntwo.png\ntwo");
}
