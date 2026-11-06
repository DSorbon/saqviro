//! Workflows: tools run one after another, each on what the last one made.
//! Files in between live in the job's temp directory; only what the last
//! step makes reaches the user.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use saqviro_core::{
    manifest::{DataType, ToolManifest},
    registry::Registry,
};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub tool: String,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    #[schemars(with = "std::collections::BTreeMap<String, Value>")]
    pub params: Params,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub steps: Vec<Step>,
    /// One failing file does not stop the others.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_going: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Which step, from 0.
    pub step: usize,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "step {}: {}", self.step + 1, self.message)
    }
}

/// True if what `from` makes can go into `to`.
pub fn can_follow(from: &ToolManifest, to: &ToolManifest) -> bool {
    let (Some(made), Some(wanted)) = (from.outputs.first(), to.inputs.first()) else {
        return false;
    };
    let is_text = |t: DataType| matches!(t, DataType::Text | DataType::Json);
    match (made.data_type, wanted.data_type) {
        (_, DataType::None) | (DataType::None, _) => false,
        (a, b) if is_text(a) || is_text(b) => is_text(a) && is_text(b),
        (_, DataType::Any) => true,
        (a, b) => a == b,
    }
}

struct Ready {
    name: String,
    executor: Arc<dyn Executor>,
    params: Params,
}

/// Checks a workflow against the manifests and makes it runnable.
pub fn prepare(
    registry: &Registry,
    steps: &[Step],
    available: impl Fn(&ToolManifest) -> bool,
) -> Result<Arc<dyn Executor>, Vec<Issue>> {
    prepare_with(registry, steps, available, false)
}

/// True if this tool should run file by file, so that one bad file costs only
/// its own result. Tools that report on all their files together do not.
pub fn runs_file_by_file(tool: &ToolManifest, input: &JobInput) -> bool {
    let reports = tool
        .outputs
        .first()
        .is_some_and(|o| matches!(o.data_type, DataType::Text | DataType::Json));
    let many = matches!(input, JobInput::Files(files) if files.len() > 1);
    many && tool.capabilities.batch && !tool.capabilities.destructive && !reports
}

/// Like `prepare`. With `keep_going`, each file goes through all the steps on
/// its own, and a file that fails is left out and named in a note.
pub fn prepare_with(
    registry: &Registry,
    steps: &[Step],
    available: impl Fn(&ToolManifest) -> bool,
    keep_going: bool,
) -> Result<Arc<dyn Executor>, Vec<Issue>> {
    let mut issues = Vec::new();
    let mut ready = Vec::with_capacity(steps.len());
    let mut issue = |step: usize, message: String| issues.push(Issue { step, message });
    if steps.is_empty() {
        issue(0, "add at least one step".into());
    }
    let mut last: Option<&ToolManifest> = None;
    for (i, step) in steps.iter().enumerate() {
        let Some(tool) = registry.get(&step.tool).filter(|t| available(t)) else {
            issue(i, format!("the tool `{}` is not available", step.tool));
            last = None;
            continue;
        };
        if tool.capabilities.destructive {
            issue(
                i,
                format!(
                    "{} changes files in place, so it cannot be part of a workflow",
                    tool.name
                ),
            );
        }
        if let Some(previous) = last
            && !can_follow(previous, tool)
        {
            issue(i, format!("{} cannot use what {} makes", tool.name, previous.name));
        }
        match tool.resolve_params(&step.params) {
            Ok(params) => {
                if let Some(executor) = saqviro_tools::executor(tool) {
                    ready.push(Ready {
                        name: tool.name.clone(),
                        executor,
                        params,
                    });
                }
            }
            Err(found) => {
                let text = found.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
                issue(i, text);
            }
        }
        last = Some(tool);
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let runner = Runner { steps: ready };
    Ok(if keep_going {
        Arc::new(EachFile { runner })
    } else {
        Arc::new(runner)
    })
}

struct Runner {
    steps: Vec<Ready>,
}

/// `path` if it is a file, and every file under it if it is a folder.
fn files_in(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), JobError> {
    if !path.is_dir() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let mut inside: Vec<PathBuf> = fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    inside.sort();
    inside.iter().try_for_each(|child| files_in(child, out))
}

/// Files and folders of `output`, as input of the next step. A folder stands
/// for the files in it.
fn next_input(ctx: &JobContext, output: JobOutput, step: usize) -> Result<JobInput, JobError> {
    match output {
        JobOutput::Text(text) => Ok(JobInput::Text(text)),
        JobOutput::Files(files) => {
            let mut paths = Vec::with_capacity(files.len());
            for file in files {
                files_in(&file.path, &mut paths)?;
            }
            Ok(JobInput::Files(paths))
        }
        JobOutput::Image(image) => {
            let bytes = STANDARD
                .decode(image.data)
                .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?;
            let path = ctx.temp_dir()?.join(format!("step-{step}.{}", image.ext));
            fs::write(&path, bytes)?;
            Ok(JobInput::Files(vec![path]))
        }
    }
}

/// Results compare with the files the user gave, not with the step before.
fn compare_with_sources(output: JobOutput, sources: &[PathBuf]) -> JobOutput {
    match output {
        JobOutput::Files(mut files) if files.len() == sources.len() => {
            for (file, source) in files.iter_mut().zip(sources) {
                file.source_size = fs::metadata(source).ok().map(|m| m.len());
            }
            JobOutput::Files(files)
        }
        JobOutput::Files(files) => JobOutput::Files(
            files
                .into_iter()
                .map(|f| OutputFile { source_size: None, ..f })
                .collect(),
        ),
        other => other,
    }
}

/// Results of a job that did not finish are not kept.
fn remove_results(results: &[OutputFile]) {
    for result in results {
        let _ = if result.path.is_dir() {
            fs::remove_dir_all(&result.path)
        } else {
            fs::remove_file(&result.path)
        };
    }
}

/// Runs the chain once per file.
struct EachFile {
    runner: Runner,
}

impl Executor for EachFile {
    fn run(&self, ctx: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError> {
        let JobInput::Files(files) = input else {
            return self.runner.run(ctx, input, params);
        };
        let results_dir = ctx.output_dir().map(Path::to_path_buf);
        let share = 1.0 / files.len().max(1) as f32;
        let mut results: Vec<OutputFile> = Vec::with_capacity(files.len());
        let mut texts: Vec<String> = Vec::new();
        let mut pictures: Vec<JobOutput> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        let mut last_error = None;
        for (i, file) in files.iter().enumerate() {
            if let Err(e) = ctx.check_cancelled() {
                remove_results(&results);
                return Err(e);
            }
            let dir = results_dir.clone().or_else(|| file.parent().map(Path::to_path_buf));
            let inner = ctx.scoped(dir, (i as f32 * share, (i + 1) as f32 * share));
            match self.runner.run(&inner, JobInput::Files(vec![file.clone()]), params) {
                Ok(JobOutput::Files(made)) => results.extend(made),
                Ok(JobOutput::Text(text)) => texts.push(text),
                Ok(picture) => pictures.push(picture),
                Err(e) if e.kind == ErrorKind::Cancelled => {
                    remove_results(&results);
                    return Err(e);
                }
                Err(e) => {
                    failed.push(
                        file.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    last_error = Some(e);
                }
            }
        }
        let failures = format!("{} failed: {}", failed.len(), failed.join(", "));
        if !texts.is_empty() {
            if !failed.is_empty() {
                texts.push(failures);
            }
            return Ok(JobOutput::Text(texts.join("\n\n")));
        }
        if results.is_empty() {
            return match (pictures.pop(), last_error) {
                (Some(picture), _) => Ok(picture),
                (None, Some(e)) => Err(e),
                (None, None) => Ok(JobOutput::Files(results)),
            };
        }
        if let (false, Some(first)) = (failed.is_empty(), results.first_mut()) {
            first.note = Some(failures);
        }
        Ok(JobOutput::Files(results))
    }
}

impl Executor for Runner {
    fn run(&self, ctx: &JobContext, input: JobInput, _: &Params) -> Result<JobOutput, JobError> {
        let sources = match &input {
            JobInput::Files(files) => files.clone(),
            JobInput::Text(_) => Vec::new(),
        };
        let results_dir = ctx
            .output_dir()
            .or_else(|| sources.first().and_then(|f| f.parent()))
            .map(Path::to_path_buf);
        let share = 1.0 / self.steps.len() as f32;
        let mut current = input;
        for (i, step) in self.steps.iter().enumerate() {
            ctx.check_cancelled()?;
            let last = i + 1 == self.steps.len();
            let dir = if last {
                results_dir.clone()
            } else {
                let dir = ctx.temp_dir()?.join(format!("step-{i}"));
                fs::create_dir_all(&dir)?;
                Some(dir)
            };
            let span = (i as f32 * share, (i + 1) as f32 * share);
            let inner = ctx.scoped(dir, span);
            let output = step
                .executor
                .run(&inner, current, &step.params)
                .map_err(|e| match e.kind {
                    ErrorKind::Cancelled => e,
                    kind => JobError::new(kind, format!("{}: {}", step.name, e.message)),
                })?;
            if last {
                return Ok(compare_with_sources(output, &sources));
            }
            current = next_input(ctx, output, i)?;
            // Temp files of the step are gone with `inner`; results are in `dir`.
        }
        Err(JobError::new(ErrorKind::InvalidInput, "add at least one step"))
    }
}
