use base64::{Engine, engine::general_purpose::STANDARD};
use saqviro_core::{manifest::ToolManifest, registry::Registry};
use saqviro_jobs::{Config, ErrorKind, JobError, JobId, JobInput, JobManager, JobOutput, JobSnapshot, Params};
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime, State, path::BaseDirectory};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

use saqviro_tools::{catalog::registry as bundled_registry, is_available};

/// File extensions one tool accepts, or every available tool when `tool_id` is `None`.
fn accepted_extensions(registry: &Registry, tool_id: Option<&str>) -> Vec<String> {
    let mut exts: Vec<String> = registry
        .tools()
        .iter()
        .filter(|t| is_available(t) && tool_id.is_none_or(|id| t.id.as_str() == id))
        .flat_map(|t| t.inputs.iter().flat_map(|i| i.extensions.iter().cloned()))
        // `*` is no filter: an empty list shows every file in the picker.
        .filter(|e| e != "*")
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

/// The UI may only open or reveal files that Saqviro itself created.
fn job_result(jobs: &JobManager, path: &str) -> Result<PathBuf, JobError> {
    let path = PathBuf::from(path);
    if jobs.is_output(&path) {
        Ok(path)
    } else {
        Err(JobError::new(
            ErrorKind::PermissionDenied,
            "only files created by Saqviro can be opened from here",
        ))
    }
}

/// Validates the request against the manifest and queues the job.
fn start_job(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    input: JobInput,
    params: &Params,
    output_dir: Option<PathBuf>,
) -> Result<JobId, JobError> {
    let tool = registry
        .get(tool_id)
        .ok_or_else(|| JobError::new(ErrorKind::InvalidInput, format!("unknown tool `{tool_id}`")))?;
    let executor = saqviro_tools::executor(tool)
        .ok_or_else(|| JobError::new(ErrorKind::EngineMissing, format!("{} is not available yet", tool.name)))?;
    let params = tool.resolve_params(params).map_err(|issues| {
        let message = issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        JobError::new(ErrorKind::InvalidInput, message)
    })?;
    Ok(jobs.submit_to(tool_id, executor, input, params, output_dir))
}

#[tauri::command]
fn list_tools(registry: State<'_, Registry>) -> Vec<ToolManifest> {
    registry.tools().iter().filter(|t| is_available(t)).cloned().collect()
}

/// Smart Drop Lite: ids of available tools that accept every dropped file.
#[tauri::command]
fn tools_for_files(registry: State<'_, Registry>, paths: Vec<String>) -> Vec<String> {
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    registry
        .for_files(&paths)
        .into_iter()
        .filter(|t| is_available(t))
        .map(|t| t.id.to_string())
        .collect()
}

#[tauri::command]
fn run_tool(
    registry: State<'_, Registry>,
    jobs: State<'_, JobManager>,
    tool_id: String,
    input: JobInput,
    params: Params,
    output_dir: Option<PathBuf>,
) -> Result<JobId, JobError> {
    start_job(&registry, &jobs, &tool_id, input, &params, output_dir)
}

/// Checks a workflow and queues it as one job.
fn start_workflow(
    registry: &Registry,
    jobs: &JobManager,
    steps: &[saqviro_workflow::Step],
    input: JobInput,
    output_dir: Option<PathBuf>,
) -> Result<JobId, JobError> {
    let executor = saqviro_workflow::prepare(registry, steps, is_available).map_err(|issues| {
        let message = issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        JobError::new(ErrorKind::InvalidInput, message)
    })?;
    Ok(jobs.submit_to("workflow", executor, input, Params::new(), output_dir))
}

#[tauri::command]
fn run_workflow(
    registry: State<'_, Registry>,
    jobs: State<'_, JobManager>,
    steps: Vec<saqviro_workflow::Step>,
    input: JobInput,
    output_dir: Option<PathBuf>,
) -> Result<JobId, JobError> {
    start_workflow(&registry, &jobs, &steps, input, output_dir)
}

/// A small picture of an image, for choosing an area of it.
#[tauri::command]
async fn image_preview(path: PathBuf) -> Result<saqviro_tools::images::Preview, JobError> {
    tauri::async_runtime::spawn_blocking(move || saqviro_tools::images::preview(&path, PREVIEW_SIZE))
        .await
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// What stands out about dropped files. Reads a little of each, changes nothing.
#[tauri::command]
async fn advise_files(paths: Vec<PathBuf>) -> Vec<saqviro_tools::advice::Advice> {
    tauri::async_runtime::spawn_blocking(move || saqviro_tools::advice::advise(&paths))
        .await
        .unwrap_or_default()
}

/// Small pictures of a PDF's pages, for choosing pages.
#[tauri::command]
async fn pdf_thumbnails(path: PathBuf) -> Result<saqviro_tools::pdf::Thumbnails, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = tempfile::tempdir()?;
        saqviro_tools::pdf::thumbnails(&path, dir.path(), MAX_THUMBNAILS)
    })
    .await
    .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// Native folder picker. Resolves to `None` if the user cancels.
#[tauri::command]
async fn pick_folder<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    let folder = app.dialog().file().blocking_pick_folder()?;
    Some(folder.into_path().ok()?.display().to_string())
}

#[tauri::command]
fn cancel_job(jobs: State<'_, JobManager>, id: JobId) -> bool {
    jobs.cancel(id)
}

/// Native file picker, filtered to what the tool (or any tool) accepts.
#[tauri::command]
async fn pick_files<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, Registry>,
    tool_id: Option<String>,
) -> Result<Vec<String>, JobError> {
    let exts = accepted_extensions(&registry, tool_id.as_deref());
    let mut dialog = app.dialog().file();
    if !exts.is_empty() {
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        dialog = dialog.add_filter("Supported files", &exts);
    }
    let picked = dialog.blocking_pick_files().unwrap_or_default();
    Ok(picked
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
        .collect())
}

/// The bytes and extension of a finished job's image.
fn job_image(jobs: &JobManager, id: JobId) -> Result<(Vec<u8>, String), JobError> {
    let Some(JobSnapshot {
        output: Some(JobOutput::Image(image)),
        ..
    }) = jobs.snapshot(id)
    else {
        return Err(JobError::new(ErrorKind::InvalidInput, "this job has no image to save"));
    };
    let bytes = STANDARD
        .decode(&image.data)
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?;
    Ok((bytes, image.ext))
}

/// Saves a job's image where the user chooses. The bytes come from the job,
/// so the UI cannot write anything else to disk.
#[tauri::command]
async fn save_image<R: Runtime>(app: AppHandle<R>, jobs: State<'_, JobManager>, id: JobId) -> Result<bool, JobError> {
    let (bytes, ext) = job_image(&jobs, id)?;
    let picked = app
        .dialog()
        .file()
        .set_file_name(format!("qr-code.{ext}"))
        .add_filter(ext.to_uppercase(), &[ext.as_str()])
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(false);
    };
    std::fs::write(path, bytes)?;
    Ok(true)
}

#[tauri::command]
fn reveal_file<R: Runtime>(app: AppHandle<R>, jobs: State<'_, JobManager>, path: String) -> Result<(), JobError> {
    let path = job_result(&jobs, &path)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))
}

#[tauri::command]
fn open_file<R: Runtime>(app: AppHandle<R>, jobs: State<'_, JobManager>, path: String) -> Result<(), JobError> {
    let path = job_result(&jobs, &path)?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))
}

/// Longest side of the picture shown for choosing an area, in pixels.
const PREVIEW_SIZE: u32 = 1000;

/// Pages of a PDF shown as pictures; beyond this, page numbers are quicker.
const MAX_THUMBNAILS: usize = 500;

/// Set when building a release. Without it the app has no way to trust an
/// update, so it does not offer any.
const UPDATE_KEY: Option<&str> = update_key(option_env!("SAQVIRO_UPDATER_PUBKEY"));
const UPDATE_MANIFEST: &str = "https://github.com/DSorbon/saqviro/releases/latest/download/latest.json";

/// CI passes a variable nobody has set as an empty string.
const fn update_key(key: Option<&'static str>) -> Option<&'static str> {
    match key {
        Some(key) if !key.is_empty() => Some(key),
        _ => None,
    }
}

#[derive(Debug, Serialize)]
struct AvailableUpdate {
    version: String,
    notes: Option<String>,
}

fn update_error(e: impl std::fmt::Display) -> JobError {
    JobError::new(ErrorKind::Unknown, format!("could not check for updates: {e}"))
}

async fn find_update<R: Runtime>(app: &AppHandle<R>) -> Result<Option<tauri_plugin_updater::Update>, JobError> {
    let key = UPDATE_KEY.ok_or_else(|| update_error("this build has no update key"))?;
    let url = UPDATE_MANIFEST.parse().map_err(update_error)?;
    app.updater_builder()
        .pubkey(key)
        .endpoints(vec![url])
        .map_err(update_error)?
        .build()
        .map_err(update_error)?
        .check()
        .await
        .map_err(update_error)
}

#[tauri::command]
fn updates_enabled() -> bool {
    UPDATE_KEY.is_some()
}

/// Asks the update server only when the user does: the app never calls out
/// on its own.
#[tauri::command]
async fn check_update<R: Runtime>(app: AppHandle<R>) -> Result<Option<AvailableUpdate>, JobError> {
    let update = find_update(&app).await?;
    Ok(update.map(|u| AvailableUpdate {
        version: u.version,
        notes: u.body,
    }))
}

/// The plugin checks the signature before anything is installed.
#[tauri::command]
async fn install_update<R: Runtime>(app: AppHandle<R>) -> Result<(), JobError> {
    let Some(update) = find_update(&app).await? else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(update_error)?;
    app.restart()
}

/// Opens the licenses that ship with the app.
#[tauri::command]
fn open_licenses<R: Runtime>(app: AppHandle<R>) -> Result<(), JobError> {
    let path = app
        .path()
        .resolve("binaries/THIRD-PARTY-NOTICES.html", BaseDirectory::Resource)
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?;
    if !path.is_file() {
        return Err(JobError::new(
            ErrorKind::InvalidInput,
            "licenses are included in release builds",
        ));
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))
}

fn job_manager<R: Runtime>(app: &AppHandle<R>) -> Result<JobManager, Box<dyn std::error::Error>> {
    let workers = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4));
    let config = Config {
        temp_root: app.path().app_cache_dir()?.join("jobs"),
        workers,
    };
    let handle = app.clone();
    Ok(JobManager::new(config, move |snapshot: &JobSnapshot| {
        // Every job change reaches the UI as a `job` event.
        let _ = handle.emit("job", snapshot);
    })?)
}

fn app<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    let builder = match UPDATE_KEY {
        Some(key) => builder.plugin(tauri_plugin_updater::Builder::new().pubkey(key).build()),
        None => builder,
    };
    builder
        // The UI gets no plugin permissions; only the Rust commands below use these.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(bundled_registry())
        .setup(|app| {
            let jobs = job_manager(app.handle())?;
            app.manage(jobs);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tools,
            tools_for_files,
            run_tool,
            run_workflow,
            image_preview,
            advise_files,
            pdf_thumbnails,
            pick_folder,
            cancel_job,
            pick_files,
            save_image,
            updates_enabled,
            check_update,
            install_update,
            open_licenses,
            reveal_file,
            open_file
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app(Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use saqviro_jobs::{JobOutput, JobStatus};
    use serde_json::json;

    use super::*;

    #[test]
    fn an_empty_update_key_is_no_key() {
        assert_eq!(update_key(None), None);
        assert_eq!(update_key(Some("")), None);
        assert_eq!(update_key(Some("key")), Some("key"));
    }

    #[test]
    fn release_scripts_publish_where_the_app_looks_for_updates() {
        let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts");
        let base = UPDATE_MANIFEST.strip_suffix("/latest.json").unwrap();
        let build = std::fs::read_to_string(scripts.join("build-release.sh")).unwrap();
        let manifest = std::fs::read_to_string(scripts.join("release-manifest.py")).unwrap();
        assert!(build.contains(UPDATE_MANIFEST));
        assert!(manifest.contains(base));
    }

    fn jobs() -> (JobManager, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let config = Config {
            temp_root: root.path().join("jobs"),
            workers: 1,
        };
        (JobManager::new(config, |_: &JobSnapshot| {}).unwrap(), root)
    }

    fn params(v: serde_json::Value) -> Params {
        v.as_object().unwrap().clone()
    }

    /// Sends commands exactly as `src/lib/api.ts` does, so argument names and JSON shapes stay in sync.
    #[test]
    fn ipc_accepts_the_frontend_payloads() {
        use tauri::{
            ipc::{CallbackFn, InvokeBody},
            test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets},
            webview::InvokeRequest,
        };

        let app = app(mock_builder()).build(mock_context(noop_assets())).unwrap();
        // `setup` only runs once the event loop starts, so the job manager is added here.
        let (jobs, _root) = jobs();
        app.manage(jobs);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let invoke = |cmd: &str, body: serde_json::Value| {
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    // The app origin differs per OS (tauri://localhost, http://tauri.localhost).
                    url: webview.url().unwrap(),
                    body: InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
        };

        let tools: Vec<serde_json::Value> = invoke("list_tools", json!({})).unwrap().deserialize().unwrap();
        let ids: Vec<_> = tools.iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert!(
            ids.contains(&"developer.json.format") && ids.contains(&"image.compress"),
            "{ids:?}"
        );
        // `tauri dev` builds without default features, so HEIC must not depend on them.
        assert_eq!(ids.contains(&"image.heic-to-jpg"), cfg!(not(windows)), "{ids:?}");

        let body = json!({
            "toolId": "developer.json.format",
            "input": { "kind": "text", "value": r#"{"b":1,"a":2}"# },
            "params": { "indent": "minify", "sort_keys": true }
        });
        let id: u64 = invoke("run_tool", body).unwrap().deserialize().unwrap();
        let done = app.state::<JobManager>().wait(JobId(id)).unwrap();
        assert_eq!(
            serde_json::to_value(&done).unwrap()["output"],
            json!({ "kind": "text", "value": r#"{"a":2,"b":1}"# })
        );

        let bad = json!({ "toolId": "pdf.nope", "input": { "kind": "files", "value": [] }, "params": {} });
        assert_eq!(invoke("run_tool", bad).unwrap_err()["kind"], "invalid_input");

        let cancelled: bool = invoke("cancel_job", json!({ "id": 9999 }))
            .unwrap()
            .deserialize()
            .unwrap();
        assert!(!cancelled);

        // Files the UI did not get from a job cannot be opened or revealed.
        for cmd in ["open_file", "reveal_file"] {
            let err = invoke(cmd, json!({ "path": "/etc/passwd" })).unwrap_err();
            assert_eq!(err["kind"], "permission_denied", "{cmd}");
        }
    }

    #[test]
    fn bundled_manifests_load() {
        let registry = bundled_registry();
        assert_eq!(registry.tools().len(), saqviro_tools::catalog::MANIFESTS.len());
        assert!(registry.get("image.resize").is_some());
    }

    #[test]
    fn runs_an_available_tool_with_defaults_filled_in() {
        let (jobs, _root) = jobs();
        let id = start_job(
            &bundled_registry(),
            &jobs,
            "developer.json.format",
            JobInput::Text("[1]".into()),
            &params(json!({})),
            None,
        )
        .unwrap();
        let done = jobs.wait(id).unwrap();
        assert_eq!(done.status, JobStatus::Completed);
        assert_eq!(done.output, Some(JobOutput::Text("[\n  1\n]\n".into())));
    }

    #[test]
    fn runs_a_workflow_and_refuses_a_broken_one() {
        let (jobs, _root) = jobs();
        let registry = bundled_registry();
        let step = |tool: &str| saqviro_workflow::Step {
            tool: tool.into(),
            params: Params::new(),
        };
        let input = || JobInput::Text("b\na\na".into());
        let steps = [step("text.lines.dedupe"), step("text.lines.reverse")];
        let id = start_workflow(&registry, &jobs, &steps, input(), None).unwrap();
        let done = jobs.wait(id).unwrap();
        assert_eq!(done.output, Some(JobOutput::Text("a\nb".into())));

        let steps = [step("text.lines.dedupe"), step("image.resize")];
        let err = start_workflow(&registry, &jobs, &steps, input(), None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert!(err.message.starts_with("step 2: "), "{}", err.message);
    }

    #[test]
    fn rejects_unknown_tools_invalid_params_and_missing_engines() {
        let (jobs, _root) = jobs();
        let registry = bundled_registry();
        let text = || JobInput::Text("[]".into());

        let err = start_job(&registry, &jobs, "pdf.nope", text(), &Params::new(), None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput);

        let err = start_job(
            &registry,
            &jobs,
            "developer.json.format",
            text(),
            &params(json!({ "indent": "8" })),
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert!(err.message.contains("indent"), "{}", err.message);

        let planned = planned_only_registry();
        let err = start_job(
            &planned,
            &jobs,
            "image.planned",
            JobInput::Files(vec![]),
            &Params::new(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::EngineMissing);
    }

    /// A registry with one manifest that has no implementation yet.
    fn planned_only_registry() -> Registry {
        let yaml = r#"
schema_version: 1
id: image.planned
name: Planned
category: image
tool_version: 1
description: Crop.
keywords: [crop]
inputs: [{ type: image, mime: [image/png], extensions: [png] }]
outputs: [{ type: image }]
engine: builtin
capabilities: { batch: true, preview: true, offline: true, destructive: false }
permissions: { network: false }
platforms: [macos]
presets: [{ id: default, name: Default }]
"#;
        Registry::from_sources([("crop", yaml)]).unwrap()
    }

    #[test]
    fn only_implemented_tools_are_available() {
        assert!(is_available(bundled_registry().get("image.resize").unwrap()));
        assert!(!is_available(planned_only_registry().get("image.planned").unwrap()));
        assert!(accepted_extensions(&planned_only_registry(), None).is_empty());
    }

    #[test]
    fn only_a_job_with_an_image_can_be_saved() {
        let (jobs, _root) = jobs();
        let registry = bundled_registry();
        let qr = Params::from_iter([("format".to_owned(), "png".into())]);
        let id = start_job(&registry, &jobs, "qr.generate", JobInput::Text("hi".into()), &qr, None).unwrap();
        jobs.wait(id).unwrap();
        let (bytes, ext) = job_image(&jobs, id).unwrap();
        assert_eq!((&bytes[1..4], ext.as_str()), (&b"PNG"[..], "png"));

        let text = start_job(
            &registry,
            &jobs,
            "text.word-count",
            JobInput::Text("hi".into()),
            &Params::new(),
            None,
        )
        .unwrap();
        jobs.wait(text).unwrap();
        assert_eq!(job_image(&jobs, text).unwrap_err().kind, ErrorKind::InvalidInput);
        assert!(job_image(&jobs, JobId(9999)).is_err());
    }

    #[test]
    fn picking_files_for_a_tool_that_takes_anything_has_no_filter() {
        let registry = bundled_registry();
        assert!(accepted_extensions(&registry, Some("files.zip.create")).is_empty());
        assert!(!accepted_extensions(&registry, None).contains(&"*".to_owned()));
    }

    #[test]
    fn file_picker_filters_come_from_manifests() {
        let registry = bundled_registry();
        assert_eq!(accepted_extensions(&registry, Some("image.webp-to-jpg")), ["webp"]);
        let all = accepted_extensions(&registry, None);
        assert!(
            all.contains(&"jpg".to_string()) && all.contains(&"png".to_string()),
            "{all:?}"
        );
        assert!(all.windows(2).all(|w| w[0] < w[1]), "not sorted and unique: {all:?}");
    }

    #[test]
    fn only_job_results_can_be_opened() {
        use saqviro_jobs::OutputFile;
        let (jobs, _root) = jobs();
        let made = OutputFile {
            path: "/tmp/x-clean.jpg".into(),
            size: 1,
            source_size: None,
            note: None,
        };
        let exec: std::sync::Arc<dyn saqviro_jobs::Executor> =
            std::sync::Arc::new(move |_: &saqviro_jobs::JobContext, _: JobInput, _: &Params| {
                Ok(JobOutput::Files(vec![made.clone()]))
            });
        jobs.wait(jobs.submit("x.y", exec, JobInput::Files(vec![]), Params::new()))
            .unwrap();

        assert!(job_result(&jobs, "/tmp/x-clean.jpg").is_ok());
        assert_eq!(
            job_result(&jobs, "/tmp/other.jpg").unwrap_err().kind,
            ErrorKind::PermissionDenied
        );
    }
}
