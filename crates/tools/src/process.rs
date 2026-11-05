//! Running engines as separate processes. Untrusted files are parsed there,
//! so a crash fails one job and cancelling is a matter of killing a process.

use std::{
    env,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use saqviro_jobs::{JobContext, JobError};

/// How much of an engine's stderr is kept, from the end.
const STDERR_TAIL: usize = 32 * 1024;

/// Finds `name` in the first directory that has it.
pub fn find_binary(dirs: impl IntoIterator<Item = PathBuf>, name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", env::consts::EXE_SUFFIX);
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// Finds an engine shipped with the app, under a name of its own so that it
/// never collides with a system package. With `allow_system`, for development,
/// the plain name is accepted too. A release must never take it: next to
/// `/usr/bin/saqviro-desktop` it would be the system's `ffmpeg`.
pub fn find_engine(dirs: &[PathBuf], name: &str, allow_system: bool) -> Option<PathBuf> {
    find_binary(dirs.iter().cloned(), &format!("saqviro-{name}"))
        .or_else(|| allow_system.then(|| find_binary(dirs.iter().cloned(), name)).flatten())
}

/// [`find_engine`] for this process.
pub fn engine(name: &str) -> Option<PathBuf> {
    find_engine(&search_dirs(), name, cfg!(debug_assertions))
}

/// Where engines are looked for: next to the app, and in debug builds also
/// on `PATH`. Release builds run only the binaries they ship with.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .into_iter()
        .collect();
    if cfg!(debug_assertions) {
        // Test binaries live one level below the workspace's other binaries.
        if let Some(parent) = dirs.first().and_then(|d| d.parent()).map(Path::to_path_buf) {
            dirs.push(parent);
        }
        if let Some(path) = env::var_os("PATH") {
            dirs.extend(env::split_paths(&path));
        }
    }
    dirs
}

fn keep_tail(mut pipe: impl Read) -> String {
    let mut tail = Vec::new();
    let mut chunk = [0; 8192];
    while let Ok(n) = pipe.read(&mut chunk) {
        if n == 0 {
            break;
        }
        tail.extend_from_slice(&chunk[..n]);
        if tail.len() > 2 * STDERR_TAIL {
            tail.drain(..tail.len() - STDERR_TAIL);
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Waits for the child, killing it if the job is cancelled.
fn wait(ctx: &JobContext, child: &mut Child) -> Result<ExitStatus, JobError> {
    loop {
        if ctx.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(JobError::cancelled());
        }
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Runs a process to the end, passing each stdout line to `on_line`. Returns
/// its exit status and stderr. Stops with an error if the job is cancelled.
pub fn supervise(
    ctx: &JobContext,
    command: &mut Command,
    on_line: impl FnMut(&str) + Send,
) -> Result<(ExitStatus, String), JobError> {
    hide_console(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");

    thread::scope(|scope| {
        let mut on_line = on_line;
        scope.spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                on_line(&line);
            }
        });
        let stderr = scope.spawn(move || keep_tail(stderr));
        let status = wait(ctx, &mut child);
        let stderr = stderr.join().unwrap_or_default();
        Ok((status?, stderr))
    })
}
