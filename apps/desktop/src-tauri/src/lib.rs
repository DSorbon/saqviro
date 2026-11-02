use saqviro_core::{manifest::ToolManifest, registry::Registry};
use saqviro_jobs::{Config, ErrorKind, JobError, JobId, JobInput, JobManager, JobSnapshot, Params};
use std::path::PathBuf;

use tauri::{AppHandle, Builder, Emitter, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

mod catalog {
    include!(concat!(env!("OUT_DIR"), "/manifests.rs"));
}

fn bundled_registry() -> Registry {
    Registry::from_sources(catalog::MANIFESTS.iter().copied())
        .unwrap_or_else(|e| panic!("bundled tool manifests are invalid: {e}"))
}

/// Only tools with an implementation are shown. A manifest alone is not a tool.
fn is_available(tool: &ToolManifest) -> bool {
    saqviro_tools::executor(tool).is_some()
}

/// File extensions one tool accepts, or every available tool when `tool_id` is `None`.
fn accepted_extensions(registry: &Registry, tool_id: Option<&str>) -> Vec<String> {
    let mut exts: Vec<String> = registry
        .tools()
        .iter()
        .filter(|t| is_available(t) && tool_id.is_none_or(|id| t.id.as_str() == id))
        .flat_map(|t| t.inputs.iter().flat_map(|i| i.extensions.iter().cloned()))
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
    Ok(jobs.submit(tool_id, executor, input, params))
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
) -> Result<JobId, JobError> {
    start_job(&registry, &jobs, &tool_id, input, &params)
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
            cancel_job,
            pick_files,
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
        assert_eq!(registry.tools().len(), catalog::MANIFESTS.len());
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
        )
        .unwrap();
        let done = jobs.wait(id).unwrap();
        assert_eq!(done.status, JobStatus::Completed);
        assert_eq!(done.output, Some(JobOutput::Text("[\n  1\n]\n".into())));
    }

    #[test]
    fn rejects_unknown_tools_invalid_params_and_missing_engines() {
        let (jobs, _root) = jobs();
        let registry = bundled_registry();
        let text = || JobInput::Text("[]".into());

        let err = start_job(&registry, &jobs, "pdf.nope", text(), &Params::new()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput);

        let err = start_job(
            &registry,
            &jobs,
            "developer.json.format",
            text(),
            &params(json!({ "indent": "8" })),
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert!(err.message.contains("indent"), "{}", err.message);

        let planned = planned_only_registry();
        let err = start_job(&planned, &jobs, "image.crop", JobInput::Files(vec![]), &Params::new()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::EngineMissing);
    }

    /// A registry with one manifest that has no implementation yet.
    fn planned_only_registry() -> Registry {
        let yaml = r#"
schema_version: 1
id: image.crop
name: Crop Images
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
        assert!(!is_available(planned_only_registry().get("image.crop").unwrap()));
        assert!(accepted_extensions(&planned_only_registry(), None).is_empty());
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
