//! Every tool run is a job. The manager runs jobs on a fixed pool of worker
//! threads, reports progress through a sink, supports cancellation, and gives
//! each job a private temp directory that is removed however the job ends.

use std::{
    collections::{HashMap, VecDeque},
    fs, io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

pub type Params = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidInput,
    UnsupportedFormat,
    EngineMissing,
    PermissionDenied,
    ProcessFailed,
    OutputExists,
    DiskFull,
    Cancelled,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct JobError {
    pub kind: ErrorKind,
    pub message: String,
}

impl JobError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "cancelled")
    }
}

impl From<io::Error> for JobError {
    fn from(e: io::Error) -> Self {
        let kind = match e.kind() {
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            io::ErrorKind::AlreadyExists => ErrorKind::OutputExists,
            io::ErrorKind::StorageFull => ErrorKind::DiskFull,
            io::ErrorKind::NotFound | io::ErrorKind::InvalidData => ErrorKind::InvalidInput,
            _ => ErrorKind::Unknown,
        };
        Self::new(kind, e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum JobInput {
    Text(String),
    Files(Vec<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum JobOutput {
    Text(String),
    Files(Vec<OutputFile>),
    /// A picture made from text, shown in the app until the user saves it.
    Image(OutputImage),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputImage {
    /// File extension: `png` or `svg`.
    pub ext: String,
    /// The encoded file, in Base64.
    pub data: String,
}

/// A file a job created, with sizes so the UI can show what changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputFile {
    pub path: PathBuf,
    /// Bytes.
    pub size: u64,
    /// Size of the file it was made from, when there is exactly one.
    pub source_size: Option<u64>,
    /// Something the user should know about this result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The state of a job at one moment. Every event sent to the sink is a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobSnapshot {
    pub id: JobId,
    pub tool_id: String,
    pub status: JobStatus,
    /// 0.0 to 1.0.
    pub progress: f32,
    pub output: Option<JobOutput>,
    pub error: Option<JobError>,
    /// Milliseconds since the Unix epoch.
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
}

/// Runs one tool. Called on a worker thread; may block.
pub trait Executor: Send + Sync {
    fn run(&self, ctx: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError>;
}

impl<F> Executor for F
where
    F: Fn(&JobContext, JobInput, &Params) -> Result<JobOutput, JobError> + Send + Sync,
{
    fn run(&self, ctx: &JobContext, input: JobInput, params: &Params) -> Result<JobOutput, JobError> {
        self(ctx, input, params)
    }
}

/// What an executor can see and do while it runs.
pub struct JobContext {
    cancel: Arc<AtomicBool>,
    temp_root: PathBuf,
    temp: OnceLock<TempDir>,
    report: Box<dyn Fn(f32) + Send + Sync>,
}

impl JobContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Call between units of work: `ctx.check_cancelled()?`.
    pub fn check_cancelled(&self) -> Result<(), JobError> {
        if self.is_cancelled() {
            Err(JobError::cancelled())
        } else {
            Ok(())
        }
    }

    /// Reports progress from 0.0 to 1.0; values outside are clamped.
    pub fn progress(&self, fraction: f32) {
        (self.report)(fraction.clamp(0.0, 1.0));
    }

    /// A private directory with a random name, created on first use and
    /// removed when the job ends — on success, failure, cancellation or panic.
    pub fn temp_dir(&self) -> Result<&Path, JobError> {
        if self.temp.get().is_none() {
            let dir = tempfile::Builder::new().prefix("job-").tempdir_in(&self.temp_root)?;
            let _ = self.temp.set(dir);
        }
        Ok(self.temp.get().map(TempDir::path).unwrap_or(&self.temp_root))
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Parent of all job temp directories. Leftovers from a crash are removed on start.
    pub temp_root: PathBuf,
    /// How many jobs run at once. At least one.
    pub workers: usize,
}

type Work = (Arc<dyn Executor>, JobInput, Params);

struct Entry {
    snapshot: JobSnapshot,
    cancel: Arc<AtomicBool>,
    work: Option<Work>,
}

#[derive(Default)]
struct State {
    jobs: HashMap<JobId, Entry>,
    queue: VecDeque<JobId>,
    next_id: u64,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    sink: Box<dyn Fn(&JobSnapshot) + Send + Sync>,
    temp_root: PathBuf,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Applies `f` to a job's snapshot and emits the result. The sink is called without the lock held.
    fn update(&self, id: JobId, f: impl FnOnce(&mut JobSnapshot)) {
        let snapshot = {
            let mut state = self.lock();
            let Some(entry) = state.jobs.get_mut(&id) else { return };
            f(&mut entry.snapshot);
            entry.snapshot.clone()
        };
        self.changed.notify_all();
        (self.sink)(&snapshot);
    }
}

pub struct JobManager {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl JobManager {
    pub fn new(config: Config, sink: impl Fn(&JobSnapshot) + Send + Sync + 'static) -> io::Result<Self> {
        if config.temp_root.exists() {
            for entry in fs::read_dir(&config.temp_root)? {
                let path = entry?.path();
                if path.is_dir() {
                    fs::remove_dir_all(&path)?
                } else {
                    fs::remove_file(&path)?
                }
            }
        }
        fs::create_dir_all(&config.temp_root)?;

        let shared = Arc::new(Shared {
            state: Mutex::default(),
            changed: Condvar::new(),
            sink: Box::new(sink),
            temp_root: config.temp_root,
        });
        let workers = (0..config.workers.max(1))
            .map(|_| {
                let shared = shared.clone();
                std::thread::spawn(move || worker(&shared))
            })
            .collect();
        Ok(Self { shared, workers })
    }

    pub fn submit(&self, tool_id: &str, executor: Arc<dyn Executor>, input: JobInput, params: Params) -> JobId {
        let snapshot = {
            let mut state = self.shared.lock();
            state.next_id += 1;
            let id = JobId(state.next_id);
            let snapshot = JobSnapshot {
                id,
                tool_id: tool_id.to_owned(),
                status: JobStatus::Queued,
                progress: 0.0,
                output: None,
                error: None,
                started_at: None,
                finished_at: None,
            };
            let entry = Entry {
                snapshot: snapshot.clone(),
                cancel: Arc::default(),
                work: Some((executor, input, params)),
            };
            state.jobs.insert(id, entry);
            snapshot
        };
        // Report `Queued` before any worker can see the job, so events stay in order.
        (self.shared.sink)(&snapshot);
        self.shared.lock().queue.push_back(snapshot.id);
        self.shared.changed.notify_all();
        snapshot.id
    }

    /// Cancels a queued or running job. Returns false if the job is unknown or already finished.
    pub fn cancel(&self, id: JobId) -> bool {
        let mut state = self.shared.lock();
        let Some(entry) = state.jobs.get_mut(&id) else {
            return false;
        };
        match entry.snapshot.status {
            JobStatus::Queued => {
                entry.work = None;
                state.queue.retain(|q| *q != id);
                drop(state);
                self.shared.update(id, |s| {
                    s.status = JobStatus::Cancelled;
                    s.finished_at = Some(now_ms());
                });
                true
            }
            JobStatus::Running => {
                entry.cancel.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    pub fn snapshot(&self, id: JobId) -> Option<JobSnapshot> {
        self.shared.lock().jobs.get(&id).map(|e| e.snapshot.clone())
    }

    /// True if a completed job created this file. The app only opens or
    /// reveals such files, never arbitrary paths sent by the UI.
    pub fn is_output(&self, path: &Path) -> bool {
        self.shared.lock().jobs.values().any(|e| match &e.snapshot.output {
            Some(JobOutput::Files(files)) => files.iter().any(|f| f.path == path),
            _ => false,
        })
    }

    /// Blocks until the job finishes. Returns `None` for an unknown job.
    pub fn wait(&self, id: JobId) -> Option<JobSnapshot> {
        let mut state = self.shared.lock();
        loop {
            let snapshot = &state.jobs.get(&id)?.snapshot;
            if snapshot.status.is_finished() {
                return Some(snapshot.clone());
            }
            state = self.shared.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.shutdown = true;
            for entry in state.jobs.values() {
                entry.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.shared.changed.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker(shared: &Arc<Shared>) {
    loop {
        let (id, cancel, (executor, input, params)) = {
            let mut state = shared.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(id) = state.queue.pop_front() {
                    let entry = state.jobs.get_mut(&id).expect("queued job exists");
                    if let Some(work) = entry.work.take() {
                        break (id, entry.cancel.clone(), work);
                    }
                    continue;
                }
                state = shared.changed.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };

        shared.update(id, |s| {
            s.status = JobStatus::Running;
            s.started_at = Some(now_ms());
        });

        let report = {
            let shared = shared.clone();
            move |p: f32| shared.update(id, |s| s.progress = p)
        };
        let ctx = JobContext {
            cancel,
            temp_root: shared.temp_root.clone(),
            temp: OnceLock::new(),
            report: Box::new(report),
        };
        let result = catch_unwind(AssertUnwindSafe(|| executor.run(&ctx, input, &params)))
            .unwrap_or_else(|_| Err(JobError::new(ErrorKind::Unknown, "the tool crashed")));
        let cancelled = ctx.is_cancelled();
        drop(ctx); // removes the temp dir before the job is reported as finished

        shared.update(id, |s| {
            s.finished_at = Some(now_ms());
            match result {
                _ if cancelled => s.status = JobStatus::Cancelled,
                Err(e) if e.kind == ErrorKind::Cancelled => s.status = JobStatus::Cancelled,
                Ok(output) => {
                    s.status = JobStatus::Completed;
                    s.progress = 1.0;
                    s.output = Some(output);
                }
                Err(e) => {
                    s.status = JobStatus::Failed;
                    s.error = Some(e);
                }
            }
        });
    }
}
