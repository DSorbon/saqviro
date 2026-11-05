//! Any process can read another's command line, so passwords must never be
//! in one. A wrapper around qpdf records every argument it receives. The
//! outer test starts this binary again with the wrapper first on `PATH`,
//! because the engine is located once per process.
#![cfg(unix)]

use std::{env, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use lopdf::dictionary;
use saqviro_core::registry::Registry;
use saqviro_jobs::{Config, JobInput, JobManager, JobSnapshot, JobStatus};
use serde_json::json;

const PASSWORD: &str = "SECRETWORD";

fn run(tool_id: &str, file: &Path) -> JobSnapshot {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/pdf")
            .join(tool_id.trim_start_matches("pdf.").replace('.', "-"))
            .join("tool.yaml"),
    )
    .unwrap();
    let registry = Registry::from_sources([("tool.yaml", source.as_str())]).unwrap();
    let tool = registry.get(tool_id).unwrap();
    let params = json!({ "password": PASSWORD });
    let params = tool.resolve_params(params.as_object().unwrap()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers: 1,
    };
    let jobs = JobManager::new(config, |_: &JobSnapshot| {}).unwrap();
    let exec = saqviro_tools::executor(tool).unwrap();
    jobs.wait(jobs.submit(tool_id, exec, JobInput::Files(vec![file.into()]), params))
        .unwrap()
}

#[test]
#[ignore = "started by passwords_stay_out_of_command_lines"]
fn add_and_remove_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("doc.pdf");
    let mut doc = lopdf::Document::with_version("1.7");
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
    });
    doc.objects.insert(
        pages,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    doc.save(&src).unwrap();

    let done = run("pdf.password.add", &src);
    assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
    let done = run("pdf.password.remove", &dir.path().join("doc-protected.pdf"));
    assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
}

#[test]
fn passwords_stay_out_of_command_lines() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("arguments.log");
    let real = Command::new("sh").args(["-c", "command -v qpdf"]).output().unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    let wrapper = dir.path().join("qpdf");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexec '{}' \"$@\"\n",
        log.display(),
        real.trim()
    );
    fs::write(&wrapper, script).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    let path = env::join_paths(
        std::iter::once(dir.path().to_path_buf()).chain(env::split_paths(&env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let status = Command::new(env::current_exe().unwrap())
        .args(["--ignored", "--exact", "add_and_remove_a_password"])
        .env("PATH", path)
        .status()
        .unwrap();
    assert!(status.success());

    let arguments = fs::read_to_string(&log).expect("the wrapper was not used");
    assert!(arguments.contains("--decrypt"), "{arguments}");
    assert!(!arguments.contains(PASSWORD), "{arguments}");
}
