use saqviro_core::{manifest::ToolManifest, registry::Registry};
use saqviro_jobs::{Config, ErrorKind, JobError, JobId, JobInput, JobManager, JobSnapshot, Params};
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime, State};

mod catalog {
    include!(concat!(env!("OUT_DIR"), "/manifests.rs"));
}

fn bundled_registry() -> Registry {
    Registry::from_sources(catalog::MANIFESTS.iter().copied())
        .unwrap_or_else(|e| panic!("bundled tool manifests are invalid: {e}"))
}

/// Only tools with an implementation are shown. A manifest alone is not a tool.
fn is_available(tool: &ToolManifest) -> bool {
    saqviro_tools::executor(tool.id.as_str()).is_some()
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
    let executor = saqviro_tools::executor(tool_id)
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
            cancel_job
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
        assert_eq!(ids, ["developer.json.format"]);

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

        let err = start_job(
            &registry,
            &jobs,
            "image.resize",
            JobInput::Files(vec![]),
            &Params::new(),
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::EngineMissing);
    }

    #[test]
    fn only_implemented_tools_are_available() {
        let registry = bundled_registry();
        assert!(is_available(registry.get("developer.json.format").unwrap()));
        assert!(!is_available(registry.get("image.resize").unwrap()));
    }
}
