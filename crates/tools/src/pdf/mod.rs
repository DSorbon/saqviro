//! PDF tools. See `docs/decisions/0004-pdf-engine.md` for who does what.

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use saqviro_jobs::{ErrorKind, JobContext, JobError};

use crate::process::{engine, supervise};

pub mod edit;
pub mod pages;
pub mod stamp;
pub mod tools;

/// The exit code of `saqviro-pdf-render` for a PDF it cannot open without a password.
pub const RENDER_NEEDS_PASSWORD: i32 = 3;

const NEEDS_PASSWORD: &str = "this PDF is protected by a password, remove it first with Remove PDF Password";

/// Turns qpdf's stderr into an error a person can read.
pub fn failure(stderr: &str) -> JobError {
    let lower = stderr.to_lowercase();
    if lower.contains("invalid password") {
        JobError::new(ErrorKind::PermissionDenied, NEEDS_PASSWORD)
    } else if lower.contains("no space left") {
        JobError::new(ErrorKind::DiskFull, "not enough disk space")
    } else if lower.contains("can't find pdf header")
        || lower.contains("unable to find trailer")
        || lower.contains("unable to find /root")
        || lower.contains("unable to recover")
    {
        JobError::new(ErrorKind::UnsupportedFormat, "this file is damaged or is not a PDF")
    } else {
        let line = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty());
        JobError::new(
            ErrorKind::ProcessFailed,
            line.unwrap_or("qpdf stopped without saying why"),
        )
    }
}

/// An absolute path can never be read as a qpdf option or an `@file`.
fn absolute(path: &Path) -> Result<PathBuf, JobError> {
    Ok(std::path::absolute(path)?)
}

/// A located qpdf binary.
#[derive(Debug)]
pub struct Qpdf {
    binary: PathBuf,
}

impl Qpdf {
    pub fn get() -> Result<&'static Self, JobError> {
        static QPDF: OnceLock<Option<Qpdf>> = OnceLock::new();
        QPDF.get_or_init(|| engine("qpdf").map(|binary| Self { binary }))
            .as_ref()
            .ok_or_else(|| JobError::new(ErrorKind::EngineMissing, "qpdf is missing from this installation"))
    }

    /// Runs qpdf and returns its stdout. Warnings about a damaged file that
    /// it recovered from are not failures.
    pub fn run(&self, ctx: &JobContext, args: &[OsString]) -> Result<String, JobError> {
        let mut out = String::new();
        let mut command = Command::new(&self.binary);
        command.arg("--warning-exit-0").args(args);
        let (status, stderr) = supervise(ctx, &mut command, |line| {
            out.push_str(line);
            out.push('\n');
        })?;
        if status.success() {
            Ok(out)
        } else {
            Err(failure(&stderr))
        }
    }

    pub fn page_count(&self, ctx: &JobContext, path: &Path) -> Result<u32, JobError> {
        let args = [OsString::from("--show-npages"), absolute(path)?.into()];
        let out = self.run(ctx, &args)?;
        out.trim()
            .parse()
            .map_err(|_| JobError::new(ErrorKind::ProcessFailed, "could not count the pages"))
    }

    /// Writes `pages` of `input`, in that order, to `output`.
    pub fn select(&self, ctx: &JobContext, input: &Path, pages: &[u32], output: &Path) -> Result<(), JobError> {
        let args = args(&[
            "--empty".as_ref(),
            "--pages".as_ref(),
            absolute(input)?.as_os_str(),
            pages::qpdf_range(pages).as_ref(),
            "--".as_ref(),
            output.as_os_str(),
        ]);
        self.run(ctx, &args).map(drop)
    }

    /// Rewrites `input` for editing: decrypted, streams decoded where that
    /// loses nothing, no object streams. Also repairs what can be repaired.
    pub fn expand(&self, ctx: &JobContext, input: &Path, output: &Path) -> Result<(), JobError> {
        let args = args(&[
            "--decrypt".as_ref(),
            "--object-streams=disable".as_ref(),
            "--stream-data=uncompress".as_ref(),
            absolute(input)?.as_os_str(),
            output.as_os_str(),
        ]);
        self.run(ctx, &args).map(drop)
    }

    /// Writes `input` as compactly as qpdf can, without changing any image.
    pub fn finish(&self, ctx: &JobContext, input: &Path, output: &Path) -> Result<(), JobError> {
        let args = args(&[
            "--object-streams=generate".as_ref(),
            "--compress-streams=y".as_ref(),
            "--recompress-flate".as_ref(),
            "--compression-level=9".as_ref(),
            input.as_os_str(),
            output.as_os_str(),
        ]);
        self.run(ctx, &args).map(drop)
    }
}

pub(crate) fn args(items: &[&OsStr]) -> Vec<OsString> {
    items.iter().map(|a| a.to_os_string()).collect()
}

/// Writes a secret to a file only this user can read, inside the job's
/// private temp directory, so it stays out of every command line.
pub(crate) fn secret_file(dir: &Path, name: &str, text: &str) -> Result<PathBuf, JobError> {
    let path = dir.join(name);
    fs::write(&path, text)?;
    Ok(path)
}
