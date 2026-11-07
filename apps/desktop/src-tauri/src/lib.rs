use base64::{Engine, engine::general_purpose::STANDARD};
use saqviro_core::{
    manifest::{DataType, ToolManifest},
    registry::Registry,
};
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

/// Queues a tool. One bad file in a batch costs only its own result.
fn start_tool(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    input: JobInput,
    params: Params,
    output_dir: Option<PathBuf>,
) -> Result<JobId, JobError> {
    if registry
        .get(tool_id)
        .is_some_and(|tool| saqviro_workflow::runs_file_by_file(tool, &input))
    {
        let steps = [saqviro_workflow::Step {
            tool: tool_id.to_owned(),
            params,
        }];
        return start_workflow(registry, jobs, tool_id, &steps, input, output_dir, true);
    }
    start_job(registry, jobs, tool_id, input, &params, output_dir)
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
    start_tool(&registry, &jobs, &tool_id, input, params, output_dir)
}

/// Checks a workflow and queues it as one job.
fn start_workflow(
    registry: &Registry,
    jobs: &JobManager,
    name: &str,
    steps: &[saqviro_workflow::Step],
    input: JobInput,
    output_dir: Option<PathBuf>,
    keep_going: bool,
) -> Result<JobId, JobError> {
    let executor = saqviro_workflow::prepare_with(registry, steps, is_available, keep_going).map_err(|issues| {
        let message = issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        JobError::new(ErrorKind::InvalidInput, message)
    })?;
    Ok(jobs.submit_to(name, executor, input, Params::new(), output_dir))
}

#[tauri::command]
fn run_workflow(
    registry: State<'_, Registry>,
    jobs: State<'_, JobManager>,
    steps: Vec<saqviro_workflow::Step>,
    input: JobInput,
    output_dir: Option<PathBuf>,
    keep_going: bool,
) -> Result<JobId, JobError> {
    start_workflow(&registry, &jobs, "workflow", &steps, input, output_dir, keep_going)
}

/// What a tool would make of one file, before anything is saved.
#[derive(Serialize)]
struct ToolPreview {
    /// JPEGs in Base64, when they can be shown.
    data: Option<String>,
    before: Option<String>,
    size: u64,
    source_size: Option<u64>,
    /// The size comes from a few seconds of the film, not from all of it.
    estimated: bool,
    /// The shape of the sound before and after, on one scale.
    peaks_before: Option<Vec<f32>>,
    peaks_after: Option<Vec<f32>>,
    /// The start of a result that is text.
    text: Option<String>,
    /// Small pictures of a result that is a folder, and how many files it has.
    items: Option<Vec<String>>,
    count: Option<usize>,
}

impl ToolPreview {
    fn sized(size: u64, source_size: Option<u64>) -> Self {
        Self {
            data: None,
            before: None,
            size,
            source_size,
            estimated: false,
            peaks_before: None,
            peaks_after: None,
            text: None,
            items: None,
            count: None,
        }
    }
}

/// How much of a text result is worth reading before it is made.
const TEXT_SAMPLE: usize = 2000;
/// Small pictures shown of a folder of results.
const FOLDER_SAMPLE: usize = 12;
const SMALL_PICTURE: u32 = 200;

const VIDEO_TYPES: [&str; 6] = ["mp4", "mov", "m4v", "mkv", "webm", "avi"];
const AUDIO_TYPES: [&str; 9] = ["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "aiff", "aif"];

fn extension(path: &std::path::Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Runs one helper step as a job and returns the text it made.
fn helper_text(
    jobs: &JobManager,
    executor: std::sync::Arc<dyn saqviro_jobs::Executor>,
    path: &std::path::Path,
    params: Params,
) -> Result<String, JobError> {
    let id = jobs.submit("preview", executor, JobInput::Files(vec![path.to_path_buf()]), params);
    let done = jobs
        .wait(id)
        .ok_or_else(|| JobError::new(ErrorKind::Unknown, "the preview was lost"))?;
    match done.output {
        Some(JobOutput::Text(text)) => Ok(text),
        _ => Err(done
            .error
            .unwrap_or_else(|| JobError::new(ErrorKind::Unknown, "nothing to show"))),
    }
}

/// Runs the tool on `source` into `dir` and returns its first result.
fn tool_result(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    sources: Vec<PathBuf>,
    params: &Params,
    dir: &std::path::Path,
) -> Result<saqviro_jobs::OutputFile, JobError> {
    let input = JobInput::Files(sources);
    let id = start_job(registry, jobs, tool_id, input, params, Some(dir.to_path_buf()))?;
    let done = jobs
        .wait(id)
        .ok_or_else(|| JobError::new(ErrorKind::Unknown, "the preview was lost"))?;
    let Some(JobOutput::Files(files)) = done.output else {
        return Err(done
            .error
            .unwrap_or_else(|| JobError::new(ErrorKind::Unknown, "nothing to show")));
    };
    files
        .into_iter()
        .next()
        .ok_or_else(|| JobError::new(ErrorKind::Unknown, "nothing to show"))
}

/// Cuts a few seconds out of `path` and runs the tool on them.
fn sample_result(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    path: &std::path::Path,
    params: &Params,
    dir: &std::path::Path,
) -> Result<(PathBuf, saqviro_jobs::OutputFile), JobError> {
    let sample = dir.join(format!("sample.{}", extension(path)));
    let to = sample.to_string_lossy().into_owned();
    let mut clip_params = Params::new();
    clip_params.insert("to".into(), serde_json::Value::String(to));
    helper_text(jobs, saqviro_tools::media::timeline::clip(), path, clip_params)?;
    let out = dir.join("out");
    std::fs::create_dir_all(&out)?;
    let result = tool_result(registry, jobs, tool_id, vec![sample.clone()], params, &out)?;
    Ok((sample, result))
}

/// What the whole file would weigh, going by a sample of it.
fn scaled_size(path: &std::path::Path, sample: &std::path::Path, made: u64) -> Result<(u64, u64), JobError> {
    let whole = std::fs::metadata(path)?.len();
    let part = std::fs::metadata(sample)?.len().max(1);
    Ok(((whole as f64 * made as f64 / part as f64) as u64, whole))
}

/// A film is judged on a few seconds of it.
fn preview_video(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    path: &std::path::Path,
    params: &Params,
    dir: &std::path::Path,
) -> Result<ToolPreview, JobError> {
    use saqviro_tools::media::timeline::still;

    let (sample, result) = sample_result(registry, jobs, tool_id, path, params, dir)?;
    let (size, source_size) = scaled_size(path, &sample, result.size)?;
    let sound = AUDIO_TYPES.contains(&extension(&result.path).as_str());
    Ok(ToolPreview {
        data: if sound {
            None
        } else {
            helper_text(jobs, still(), &result.path, Params::new()).ok()
        },
        before: helper_text(jobs, still(), &sample, Params::new()).ok(),
        estimated: true,
        peaks_after: if sound { sound_shape(jobs, &result.path) } else { None },
        ..ToolPreview::sized(size, Some(source_size))
    })
}

/// The shape of the sound of a file, on the same scale for every file.
fn sound_shape(jobs: &JobManager, path: &std::path::Path) -> Option<Vec<f32>> {
    let json = helper_text(jobs, saqviro_tools::media::timeline::timeline(), path, Params::new()).ok()?;
    let found: serde_json::Value = serde_json::from_str(&json).ok()?;
    let loudest = found["loudest"].as_f64()? as f32;
    let peaks = found["peaks"].as_array()?;
    Some(
        peaks
            .iter()
            .filter_map(|p| p.as_f64())
            .map(|p| p as f32 * loudest)
            .collect(),
    )
}

/// A recording is judged the same way, by the shape of its sound.
fn preview_audio(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    path: &std::path::Path,
    params: &Params,
    dir: &std::path::Path,
) -> Result<ToolPreview, JobError> {
    let (sample, result) = sample_result(registry, jobs, tool_id, path, params, dir)?;
    let (size, source_size) = scaled_size(path, &sample, result.size)?;
    Ok(ToolPreview {
        estimated: true,
        peaks_before: sound_shape(jobs, &sample),
        peaks_after: sound_shape(jobs, &result.path),
        ..ToolPreview::sized(size, Some(source_size))
    })
}

/// Pictures and a count for a result that is a folder of files.
fn folder_preview(folder: &std::path::Path) -> Result<(Vec<String>, usize), JobError> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(|e| Some(e.ok()?.path()))
        .collect();
    names.sort();
    let pictures = names
        .iter()
        .filter_map(|f| saqviro_tools::images::preview(f, SMALL_PICTURE).ok())
        .take(FOLDER_SAMPLE)
        .map(|p| p.data)
        .collect();
    Ok((pictures, names.len()))
}

/// Runs the tool into a temp folder that goes away with this call. Several
/// files are for tools that make one result from all of them.
fn preview_tool_now(
    registry: &Registry,
    jobs: &JobManager,
    tool_id: &str,
    paths: Vec<PathBuf>,
    params: &Params,
) -> Result<ToolPreview, JobError> {
    let dir = tempfile::tempdir()?;
    let first_path = paths
        .first()
        .cloned()
        .ok_or_else(|| JobError::new(ErrorKind::InvalidInput, "add a file"))?;
    let kind = extension(&first_path);
    // A picture or the words of a recording need all of it.
    let needs_all = registry
        .get(tool_id)
        .and_then(|t| t.outputs.first())
        .is_some_and(|o| matches!(o.data_type, DataType::Image | DataType::Text));
    if paths.len() == 1 && !needs_all {
        if VIDEO_TYPES.contains(&kind.as_str()) {
            return preview_video(registry, jobs, tool_id, &first_path, params, dir.path());
        }
        if AUDIO_TYPES.contains(&kind.as_str()) {
            return preview_audio(registry, jobs, tool_id, &first_path, params, dir.path());
        }
    }
    let first = tool_result(registry, jobs, tool_id, paths, params, dir.path())?;
    let mut preview = ToolPreview::sized(first.size, first.source_size);
    let page = |file: &std::path::Path| {
        let pictures = tempfile::tempdir().ok()?;
        saqviro_tools::pdf::first_page(file, pictures.path()).ok()
    };
    if kind == "pdf" {
        preview.before = page(&first_path);
    }
    let made = extension(&first.path);
    if first.path.is_dir() {
        let (items, count) = folder_preview(&first.path)?;
        preview.items = Some(items);
        preview.count = Some(count);
    } else if made == "pdf" {
        preview.data = page(&first.path);
    } else if matches!(made.as_str(), "txt" | "srt" | "vtt") {
        let text = std::fs::read_to_string(&first.path).unwrap_or_default();
        preview.text = Some(text.chars().take(TEXT_SAMPLE).collect());
    } else {
        preview.data = saqviro_tools::images::preview(&first.path, PREVIEW_SIZE)
            .ok()
            .map(|p| p.data);
    }
    Ok(preview)
}

/// What is inside an archive, without unpacking it.
#[tauri::command]
async fn archive_contents(path: PathBuf) -> Result<saqviro_tools::files::archive::Contents, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        if extension(&path) == "zip" {
            saqviro_tools::files::archive::contents(&path)
        } else {
            saqviro_tools::files::tarball::contents(&path)
        }
    })
    .await
    .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// What a reporting tool says about a file, without keeping anything.
#[tauri::command]
async fn file_facts<R: Runtime>(app: AppHandle<R>, tool_id: String, path: PathBuf) -> Result<String, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        let registry = app.state::<Registry>();
        let jobs = app.state::<JobManager>();
        let input = JobInput::Files(vec![path]);
        let id = start_job(&registry, &jobs, &tool_id, input, &Params::new(), None)?;
        let done = jobs
            .wait(id)
            .ok_or_else(|| JobError::new(ErrorKind::Unknown, "the file could not be read"))?;
        match done.output {
            Some(JobOutput::Text(text)) => Ok(text),
            _ => Err(done
                .error
                .unwrap_or_else(|| JobError::new(ErrorKind::Unknown, "nothing to show"))),
        }
    })
    .await
    .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// The size of each file; folders have none.
#[tauri::command]
fn file_sizes(paths: Vec<PathBuf>) -> Vec<Option<u64>> {
    paths
        .iter()
        .map(|p| std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len()))
        .collect()
}

/// The new name of each file, or why the renaming cannot be done.
#[tauri::command]
fn plan_rename(paths: Vec<PathBuf>, params: Params) -> Result<Vec<(String, String)>, JobError> {
    saqviro_tools::files::rename::names(&paths, &params).map_err(|e| JobError::new(ErrorKind::InvalidInput, e))
}

#[tauri::command]
async fn preview_tool<R: Runtime>(
    app: AppHandle<R>,
    tool_id: String,
    paths: Vec<PathBuf>,
    params: Params,
) -> Result<ToolPreview, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        preview_tool_now(
            &app.state::<Registry>(),
            &app.state::<JobManager>(),
            &tool_id,
            paths,
            &params,
        )
    })
    .await
    .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// Frames and sound of a video or audio file, as JSON.
#[tauri::command]
async fn media_timeline<R: Runtime>(app: AppHandle<R>, path: PathBuf) -> Result<String, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        let jobs = app.state::<JobManager>();
        let executor = saqviro_tools::media::timeline::timeline();
        let id = jobs.submit("timeline", executor, JobInput::Files(vec![path]), Params::new());
        let done = jobs
            .wait(id)
            .ok_or_else(|| JobError::new(ErrorKind::Unknown, "the timeline was lost"))?;
        match done.output {
            Some(JobOutput::Text(json)) => Ok(json),
            _ => Err(done
                .error
                .unwrap_or_else(|| JobError::new(ErrorKind::Unknown, "no timeline"))),
        }
    })
    .await
    .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?
}

/// A small picture of an image or a film, for choosing an area of it.
#[tauri::command]
async fn image_preview<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
) -> Result<saqviro_tools::images::Preview, JobError> {
    tauri::async_runtime::spawn_blocking(move || {
        if !VIDEO_TYPES.contains(&extension(&path).as_str()) {
            return saqviro_tools::images::preview(&path, PREVIEW_SIZE);
        }
        let jobs = app.state::<JobManager>();
        let json = helper_text(&jobs, saqviro_tools::media::timeline::poster(), &path, Params::new())?;
        serde_json::from_str(&json).map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))
    })
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
            preview_tool,
            plan_rename,
            file_facts,
            file_sizes,
            archive_contents,
            media_timeline,
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

    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 119,
        83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0, 3, 1, 1, 0, 201, 254, 146, 239, 0, 0,
        0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    #[test]
    fn a_preview_shows_the_result_and_saves_nothing() {
        let (jobs, _root) = jobs();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.png");
        std::fs::write(&source, PNG).unwrap();
        let found = preview_tool_now(
            &bundled_registry(),
            &jobs,
            "image.grayscale",
            vec![source],
            &Params::new(),
        )
        .unwrap();
        assert!(found.data.is_some());
        assert!(found.size > 0);
        assert_eq!(found.source_size, Some(PNG.len() as u64));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "only the original");
    }

    #[test]
    fn a_film_is_previewed_from_a_few_seconds_of_it() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("film.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=s=320x240:r=25:d=8",
            ])
            .args(["-pix_fmt", "yuv420p"])
            .arg(&source)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        let (jobs, _root) = jobs();
        let found = preview_tool_now(
            &bundled_registry(),
            &jobs,
            "video.compress",
            vec![source.clone()],
            &Params::new(),
        )
        .unwrap();
        assert!(found.estimated);
        assert!(found.data.is_some() && found.before.is_some());
        assert!(found.size > 0);
        assert_eq!(found.source_size, std::fs::metadata(&source).ok().map(|m| m.len()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "only the original");
    }

    #[test]
    fn a_recording_is_previewed_by_the_shape_of_its_sound() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("talk.wav");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=6",
            ])
            .arg(&source)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        let (jobs, _root) = jobs();
        let louder = params(json!({ "gain": 6 }));
        let found = preview_tool_now(&bundled_registry(), &jobs, "audio.volume", vec![source], &louder).unwrap();
        let (before, after) = (found.peaks_before.unwrap(), found.peaks_after.unwrap());
        let top = |p: &[f32]| p.iter().copied().fold(0.0, f32::max);
        assert!(top(&after) > top(&before), "{} {}", top(&after), top(&before));
        assert!(found.estimated);
    }

    #[test]
    fn one_picture_can_be_made_from_many() {
        let (jobs, _root) = jobs();
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = ["a.png", "b.png"].iter().map(|n| dir.path().join(n)).collect();
        for file in &files {
            std::fs::write(file, PNG).unwrap();
        }
        let found = preview_tool_now(&bundled_registry(), &jobs, "image.contact-sheet", files, &Params::new()).unwrap();
        assert!(found.data.is_some());
    }

    #[test]
    fn a_folder_of_results_shows_as_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("made");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("a.png"), PNG).unwrap();
        std::fs::write(folder.join("b.txt"), "x").unwrap();
        let (pictures, count) = folder_preview(&folder).unwrap();
        assert_eq!((pictures.len(), count), (1, 2));
    }

    #[test]
    fn sizes_are_known_for_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "abc").unwrap();
        assert_eq!(file_sizes(vec![file, dir.path().to_path_buf()]), [Some(3), None]);
    }

    #[test]
    fn one_bad_file_costs_a_batch_only_its_own_result() {
        let (jobs, _root) = jobs();
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.png");
        let bad = dir.path().join("bad.png");
        // One red pixel.
        std::fs::write(&good, PNG).unwrap();
        std::fs::write(&bad, "not a picture").unwrap();
        let input = JobInput::Files(vec![bad, good]);
        let id = start_tool(
            &bundled_registry(),
            &jobs,
            "image.grayscale",
            input,
            params(json!({})),
            None,
        )
        .unwrap();
        let done = jobs.wait(id).unwrap();
        assert_eq!(done.status, JobStatus::Completed, "{:?}", done.error);
        let Some(JobOutput::Files(files)) = done.output else {
            panic!("expected files");
        };
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].note.as_deref(), Some("1 failed: bad.png"));
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
        let id = start_workflow(&registry, &jobs, "workflow", &steps, input(), None, false).unwrap();
        let done = jobs.wait(id).unwrap();
        assert_eq!(done.output, Some(JobOutput::Text("a\nb".into())));

        let steps = [step("text.lines.dedupe"), step("image.resize")];
        let err = start_workflow(&registry, &jobs, "workflow", &steps, input(), None, false).unwrap_err();
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
