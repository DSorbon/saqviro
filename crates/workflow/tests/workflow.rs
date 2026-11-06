use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, ErrorKind, Executor, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, Params};
use saqviro_workflow::{Issue, Step, Workflow, can_follow, prepare, prepare_with};
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

fn step(tool: &str, params: Value) -> Step {
    Step {
        tool: tool.into(),
        params: params.as_object().unwrap().clone(),
    }
}

fn ready(steps: &[Step]) -> Arc<dyn Executor> {
    prepare(&registry(), steps, |_| true).unwrap_or_else(|i| panic!("{i:?}"))
}

fn issues(steps: &[Step]) -> Vec<Issue> {
    prepare(&registry(), steps, |_| true).err().expect("should be refused")
}

fn run(exec: Arc<dyn Executor>, input: JobInput, output_dir: Option<PathBuf>) -> (JobSnapshot, Vec<f32>) {
    let root = tempfile::tempdir().unwrap();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let seen = progress.clone();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, move |s: &JobSnapshot| seen.lock().unwrap().push(s.progress)).unwrap();
    let id = jobs.submit_to("workflow", exec, input, Params::new(), output_dir);
    let done = jobs.wait(id).unwrap();
    let progress = progress.lock().unwrap().clone();
    (done, progress)
}

fn picture(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let path = dir.join(name);
    image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]))
        .save(&path)
        .unwrap();
    path
}

fn files(done: &JobSnapshot) -> Vec<saqviro_jobs::OutputFile> {
    match &done.output {
        Some(JobOutput::Files(files)) => files.clone(),
        other => panic!("expected files, got {other:?} / {:?}", done.error),
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

#[test]
fn runs_steps_on_each_others_results() {
    let dir = tempfile::tempdir().unwrap();
    let source = picture(dir.path(), "a.png", 40, 20);
    let exec = ready(&[
        step("image.rotate", json!({ "angle": "90" })),
        step("image.resize", json!({ "width": 10 })),
        step("image.to-webp", json!({})),
    ]);
    let (done, progress) = run(exec, JobInput::Files(vec![source.clone()]), None);
    assert_eq!(done.error, None);
    let out = &files(&done)[0];
    assert_eq!(out.path.parent().unwrap(), dir.path());
    assert_eq!(out.path.extension().unwrap(), "webp");
    assert_eq!(out.source_size, Some(fs::metadata(&source).unwrap().len()));
    assert_eq!(image::image_dimensions(&out.path).unwrap(), (10, 20));
    // Only the source and the final result are left in the folder.
    assert_eq!(entries(dir.path()).len(), 2, "{:?}", entries(dir.path()));
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
    assert!(progress.iter().any(|p| *p > 0.3 && *p < 0.7), "{progress:?}");
}

#[test]
fn results_go_to_the_chosen_folder() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let source = picture(dir.path(), "a.png", 4, 4);
    let exec = ready(&[step("image.grayscale", json!({})), step("image.flip", json!({}))]);
    let (done, _) = run(
        exec,
        JobInput::Files(vec![source]),
        Some(elsewhere.path().to_path_buf()),
    );
    assert_eq!(files(&done)[0].path.parent().unwrap(), elsewhere.path());
    assert_eq!(entries(dir.path()), ["a.png"]);
}

#[test]
fn handles_many_files_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let sources = vec![picture(dir.path(), "a.png", 4, 4), picture(dir.path(), "b.png", 4, 4)];
    let exec = ready(&[
        step("image.grayscale", json!({})),
        step("image.border", json!({ "width": 1 })),
    ]);
    let (done, _) = run(exec, JobInput::Files(sources), None);
    assert_eq!(files(&done).len(), 2);
}

#[test]
fn a_folder_of_results_feeds_the_next_step_file_by_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.gif");
    let sources = [picture(dir.path(), "x.png", 4, 4), picture(dir.path(), "y.png", 4, 4)];
    let gif = ready(&[step("image.frames-to-gif", json!({}))]);
    let (done, _) = run(gif, JobInput::Files(sources.to_vec()), None);
    fs::rename(&files(&done)[0].path, &source).unwrap();

    let exec = ready(&[step("image.gif-frames", json!({})), step("image.grayscale", json!({}))]);
    let (done, _) = run(exec, JobInput::Files(vec![source]), None);
    assert_eq!(done.error, None);
    assert!(!files(&done).is_empty());
    assert_eq!(files(&done)[0].source_size, None);
}

#[test]
fn text_flows_through_text_tools() {
    let exec = ready(&[
        step("text.lines.sort", json!({ "by": "text" })),
        step("text.lines.dedupe", json!({})),
        step("text.case", json!({ "case": "upper" })),
    ]);
    let (done, _) = run(exec, JobInput::Text("b\na\nb".into()), None);
    assert_eq!(done.output, Some(JobOutput::Text("A\nB".into())));
}

#[test]
fn a_generated_picture_can_be_processed() {
    let dir = tempfile::tempdir().unwrap();
    let exec = ready(&[
        step("qr.generate", json!({ "format": "png" })),
        step("image.border", json!({ "width": 5 })),
    ]);
    let (done, _) = run(exec, JobInput::Text("hello".into()), Some(dir.path().to_path_buf()));
    assert_eq!(done.error, None);
    assert_eq!(files(&done)[0].path.parent().unwrap(), dir.path());
}

#[test]
fn a_failing_step_is_named_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = picture(dir.path(), "a.png", 4, 4);
    let exec = ready(&[
        step("image.grayscale", json!({})),
        step("image.crop", json!({ "x": 100, "aspect": "selection" })),
    ]);
    let (done, _) = run(exec, JobInput::Files(vec![source]), None);
    assert_eq!(done.status, JobStatus::Failed);
    let error = done.error.unwrap();
    assert!(error.message.starts_with("Crop Images: "), "{}", error.message);
    assert_eq!(error.kind, ErrorKind::InvalidInput);
    assert_eq!(entries(dir.path()), ["a.png"]);
}

#[test]
fn says_which_step_does_not_fit() {
    let found = issues(&[step("image.resize", json!({})), step("pdf.merge", json!({}))]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].step, 1);
    assert!(
        found[0]
            .message
            .contains("Merge PDF cannot use what Resize Images makes"),
        "{}",
        found[0]
    );
}

#[test]
fn refuses_what_cannot_run() {
    assert!(issues(&[])[0].message.contains("at least one"));
    assert!(
        issues(&[step("nope.nope", json!({}))])[0]
            .message
            .contains("not available")
    );
    assert!(
        issues(&[step("image.resize", json!({ "width": "wide" }))])[0]
            .message
            .contains("width")
    );
    assert!(
        issues(&[step("files.rename", json!({}))])[0]
            .message
            .contains("in place")
    );
    let hidden = prepare(&registry(), &[step("image.resize", json!({}))], |_| false);
    assert!(hidden.is_err());
}

#[test]
fn knows_what_can_follow_what() {
    let registry = registry();
    let tool = |id| registry.get(id).unwrap();
    assert!(can_follow(tool("image.resize"), tool("image.compress")));
    assert!(can_follow(tool("pdf.to-images"), tool("image.resize")));
    assert!(can_follow(tool("image.resize"), tool("pdf.from-images")));
    assert!(!can_follow(tool("image.resize"), tool("pdf.merge")));
    assert!(can_follow(tool("image.resize"), tool("files.sha256")));
    assert!(can_follow(tool("text.case"), tool("developer.base64")));
    assert!(!can_follow(tool("video.compress"), tool("pdf.merge")));
    assert!(!can_follow(tool("text.case"), tool("image.resize")));
    assert!(!can_follow(tool("image.resize"), tool("text.case")));
    assert!(!can_follow(tool("text.case"), tool("developer.uuid")));
}

#[test]
fn the_starter_workflows_are_valid() {
    let text = include_str!("../../../packages/workflow-schema/starters.json");
    let starters: Vec<Workflow> = serde_json::from_str(text).unwrap();
    assert!(starters.len() >= 3);
    for workflow in starters {
        assert_eq!(workflow.schema_version, saqviro_workflow::SCHEMA_VERSION);
        if let Err(found) = prepare(&registry(), &workflow.steps, |_| true) {
            panic!("{}: {found:?}", workflow.name);
        }
    }
}

#[test]
fn committed_schema_matches_rust_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/workflow-schema/workflow.schema.json");
    let generated = serde_json::to_string_pretty(&schemars::schema_for!(Workflow)).unwrap() + "\n";
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        fs::write(&path, &generated).unwrap();
    }
    assert!(
        fs::read_to_string(&path).unwrap_or_default() == generated,
        "run with UPDATE_SCHEMA=1"
    );
}

#[test]
fn can_carry_on_past_a_file_that_fails() {
    let dir = tempfile::tempdir().unwrap();
    let good = picture(dir.path(), "good.png", 4, 4);
    let bad = dir.path().join("bad.png");
    fs::write(&bad, "not a picture").unwrap();
    let steps = [step("image.grayscale", json!({})), step("image.flip", json!({}))];

    let exec = prepare_with(&registry(), &steps, |_| true, true).unwrap();
    let (done, _) = run(exec, JobInput::Files(vec![bad.clone(), good.clone()]), None);
    assert_eq!(done.status, JobStatus::Completed);
    let made = files(&done);
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].note.as_deref(), Some("1 failed: bad.png"));
    assert_eq!(made[0].source_size, Some(fs::metadata(&good).unwrap().len()));

    let (done, _) = run(ready(&steps), JobInput::Files(vec![bad, good]), None);
    assert_eq!(done.status, JobStatus::Failed);
}

#[test]
fn fails_when_every_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.png");
    fs::write(&bad, "not a picture").unwrap();
    let exec = prepare_with(&registry(), &[step("image.flip", json!({}))], |_| true, true).unwrap();
    let (done, _) = run(exec, JobInput::Files(vec![bad]), None);
    assert_eq!(done.status, JobStatus::Failed);
}
