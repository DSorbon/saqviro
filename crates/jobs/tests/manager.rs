use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use saqviro_jobs::{
    Config, ErrorKind, Executor, JobContext, JobError, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, Params,
};

type Events = Arc<Mutex<Vec<JobSnapshot>>>;

fn manager(workers: usize) -> (JobManager, Events, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let events: Events = Arc::default();
    let sink = events.clone();
    let config = Config {
        temp_root: root.path().join("jobs"),
        workers,
    };
    let jobs = JobManager::new(config, move |e: &JobSnapshot| sink.lock().unwrap().push(e.clone())).unwrap();
    (jobs, events, root)
}

fn exec(
    f: impl Fn(&JobContext, JobInput, &Params) -> Result<JobOutput, JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(f)
}

fn text(s: &str) -> JobInput {
    JobInput::Text(s.into())
}

#[test]
fn runs_a_job_and_reports_each_status() {
    let (jobs, events, _root) = manager(2);
    let upper = exec(|ctx, input, _| {
        ctx.progress(0.5);
        match input {
            JobInput::Text(s) => Ok(JobOutput::Text(s.to_uppercase())),
            JobInput::Files(_) => unreachable!(),
        }
    });

    let id = jobs.submit("text.upper", upper, text("hi"), Params::new());
    let done = jobs.wait(id).unwrap();

    assert_eq!(done.status, JobStatus::Completed);
    assert_eq!(done.output, Some(JobOutput::Text("HI".into())));
    assert_eq!(done.progress, 1.0);
    assert!(done.started_at.is_some() && done.finished_at.is_some());

    let statuses: Vec<_> = events.lock().unwrap().iter().map(|e| (e.status, e.progress)).collect();
    assert_eq!(
        statuses,
        [
            (JobStatus::Queued, 0.0),
            (JobStatus::Running, 0.0),
            (JobStatus::Running, 0.5),
            (JobStatus::Completed, 1.0)
        ]
    );
}

#[test]
fn executor_errors_fail_the_job() {
    let (jobs, _, _root) = manager(1);
    let fail = exec(|_, _, _| Err(JobError::new(ErrorKind::InvalidInput, "not JSON")));
    let done = jobs.wait(jobs.submit("x.y", fail, text(""), Params::new())).unwrap();
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn a_panicking_executor_fails_the_job_but_not_the_worker() {
    let (jobs, _, _root) = manager(1);
    let boom = exec(|_, _, _| panic!("engine crashed"));
    let done = jobs.wait(jobs.submit("x.y", boom, text(""), Params::new())).unwrap();
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::Unknown);

    let ok = exec(|_, _, _| Ok(JobOutput::Text("ok".into())));
    assert_eq!(
        jobs.wait(jobs.submit("x.y", ok, text(""), Params::new()))
            .unwrap()
            .status,
        JobStatus::Completed
    );
}

#[test]
fn cancelling_a_running_job_stops_it_and_removes_its_temp_dir() {
    let (jobs, _, _root) = manager(1);
    let (started_tx, started_rx) = mpsc::channel();
    let temp_seen: Arc<Mutex<Option<PathBuf>>> = Arc::default();
    let seen = temp_seen.clone();
    let slow = exec(move |ctx, _, _| {
        let dir = ctx.temp_dir().unwrap().to_path_buf();
        fs::write(dir.join("partial.bin"), b"half").unwrap();
        *seen.lock().unwrap() = Some(dir);
        started_tx.send(()).unwrap();
        loop {
            ctx.check_cancelled()?;
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    let id = jobs.submit("x.slow", slow, text(""), Params::new());
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(jobs.cancel(id));
    let done = jobs.wait(id).unwrap();

    assert_eq!(done.status, JobStatus::Cancelled);
    assert!(done.error.is_none());
    let dir = temp_seen.lock().unwrap().clone().unwrap();
    assert!(!dir.exists(), "temp dir {} was not removed", dir.display());
}

#[test]
fn a_cancelled_queued_job_never_runs() {
    let (jobs, _, _root) = manager(1);
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let blocker = exec(move |_, _, _| {
        release_rx.lock().unwrap().recv().unwrap();
        Ok(JobOutput::Text(String::new()))
    });
    let ran = Arc::new(Mutex::new(false));
    let flag = ran.clone();
    let queued = exec(move |_, _, _| {
        *flag.lock().unwrap() = true;
        Ok(JobOutput::Text(String::new()))
    });

    let first = jobs.submit("x.block", blocker, text(""), Params::new());
    let second = jobs.submit("x.queued", queued, text(""), Params::new());
    assert!(jobs.cancel(second));
    release_tx.send(()).unwrap();

    assert_eq!(jobs.wait(first).unwrap().status, JobStatus::Completed);
    assert_eq!(jobs.wait(second).unwrap().status, JobStatus::Cancelled);
    assert!(!*ran.lock().unwrap());
}

#[test]
fn temp_dirs_are_removed_after_success_and_failure() {
    let (jobs, _, root) = manager(2);
    let use_temp = |fail: bool| {
        exec(move |ctx, _, _| {
            fs::write(ctx.temp_dir().unwrap().join("tmp"), b"x").unwrap();
            if fail {
                Err(JobError::new(ErrorKind::ProcessFailed, "exit 1"))
            } else {
                Ok(JobOutput::Text("ok".into()))
            }
        })
    };
    let a = jobs.submit("x.y", use_temp(false), text(""), Params::new());
    let b = jobs.submit("x.y", use_temp(true), text(""), Params::new());
    jobs.wait(a).unwrap();
    jobs.wait(b).unwrap();
    assert_eq!(fs::read_dir(root.path().join("jobs")).unwrap().count(), 0);
}

#[test]
fn leftover_temp_dirs_from_a_crash_are_removed_on_start() {
    let root = tempfile::tempdir().unwrap();
    let stale = root.path().join("jobs").join("job-stale");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("half.mp4"), b"x").unwrap();

    let _jobs = JobManager::new(
        Config {
            temp_root: root.path().join("jobs"),
            workers: 1,
        },
        |_: &JobSnapshot| {},
    )
    .unwrap();
    assert!(!stale.exists());
}

#[test]
fn never_runs_more_jobs_than_workers() {
    let (jobs, _, _root) = manager(2);
    let running = Arc::new(Mutex::new((0usize, 0usize)));
    let ids: Vec<_> = (0..6)
        .map(|_| {
            let running = running.clone();
            let job = exec(move |_, _, _| {
                {
                    let mut r = running.lock().unwrap();
                    r.0 += 1;
                    r.1 = r.1.max(r.0);
                }
                std::thread::sleep(Duration::from_millis(20));
                running.lock().unwrap().0 -= 1;
                Ok(JobOutput::Text(String::new()))
            });
            jobs.submit("x.y", job, text(""), Params::new())
        })
        .collect();
    for id in ids {
        assert_eq!(jobs.wait(id).unwrap().status, JobStatus::Completed);
    }
    assert_eq!(running.lock().unwrap().1, 2);
}

#[test]
fn unknown_jobs_are_reported_as_missing() {
    let (jobs, _, _root) = manager(1);
    let done = jobs.wait(jobs.submit(
        "x.y",
        exec(|_, _, _| Ok(JobOutput::Text(String::new()))),
        text(""),
        Params::new(),
    ));
    let unknown = saqviro_jobs::JobId(done.unwrap().id.0 + 100);
    assert!(jobs.wait(unknown).is_none());
    assert!(!jobs.cancel(unknown));
}

#[test]
fn progress_is_clamped() {
    let (jobs, events, _root) = manager(1);
    let wild = exec(|ctx, _, _| {
        ctx.progress(7.0);
        ctx.progress(-1.0);
        Ok(JobOutput::Text(String::new()))
    });
    jobs.wait(jobs.submit("x.y", wild, text(""), Params::new())).unwrap();
    assert!(events.lock().unwrap().iter().all(|e| (0.0..=1.0).contains(&e.progress)));
}

#[test]
fn knows_which_files_its_jobs_created() {
    use saqviro_jobs::OutputFile;
    let (jobs, _, _root) = manager(1);
    let made = PathBuf::from("/tmp/out/a.png");
    let file = OutputFile {
        path: made.clone(),
        size: 1,
        source_size: None,
        note: None,
    };
    let job = exec(move |_, _, _| Ok(JobOutput::Files(vec![file.clone()])));
    jobs.wait(jobs.submit("x.y", job, text(""), Params::new())).unwrap();

    assert!(jobs.is_output(&made));
    assert!(!jobs.is_output(std::path::Path::new("/etc/passwd")));
}
