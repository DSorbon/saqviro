//! Writing result files. Results never overwrite anything: a taken name gets
//! a ` (2)`, ` (3)`… suffix. Files written by a job that does not finish are
//! removed, so a failed or cancelled job leaves nothing half-done behind.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use saqviro_jobs::{ErrorKind, JobError, JobInput};

/// Replaces `{name}` placeholders in one pass, so values that contain braces
/// are never substituted again. Unknown placeholders are left as they are.
pub fn render_name(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let value = rest.find('}').and_then(|close| {
            let key = &rest[1..close];
            vars.iter().find(|(k, _)| *k == key).map(|(_, v)| (close, *v))
        });
        match value {
            Some((close, v)) => {
                out.push_str(v);
                rest = &rest[close + 1..];
            }
            None => {
                out.push('{');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The files a job was given.
pub fn input_files(input: JobInput) -> Result<Vec<PathBuf>, JobError> {
    match input {
        JobInput::Files(files) if !files.is_empty() => Ok(files),
        JobInput::Files(_) => Err(JobError::new(ErrorKind::InvalidInput, "add at least one file")),
        JobInput::Text(_) => Err(JobError::new(ErrorKind::InvalidInput, "this tool works on files")),
    }
}

/// A file's name without its extension.
pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "file".into(), |s| s.to_string_lossy().into_owned())
}

/// Puts the file name in front of an error, unless the job was cancelled.
pub fn name_error(path: &Path, e: JobError) -> JobError {
    if e.kind == ErrorKind::Cancelled {
        return e;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    JobError::new(e.kind, format!("{name}: {}", e.message))
}

/// Files created by one job.
#[derive(Debug, Default)]
pub struct Outputs {
    created: Vec<PathBuf>,
}

impl Outputs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes `bytes` to `dir/name`, or to the first free `name (n)`.
    pub fn write(&mut self, dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, JobError> {
        let (mut file, path) = self.reserve(dir, name)?;
        file.write_all(bytes).and_then(|()| file.sync_all())?;
        Ok(path)
    }

    /// Moves the finished file `source` to `dir/name`, or to the first free
    /// `name (n)`. Copies when `source` is on another volume.
    pub fn adopt(&mut self, dir: &Path, name: &str, source: &Path) -> Result<PathBuf, JobError> {
        let (file, path) = self.reserve(dir, name)?;
        drop(file);
        if fs::rename(source, &path).is_err() {
            fs::copy(source, &path)?;
            fs::remove_file(source)?;
        }
        Ok(path)
    }

    /// Creates an empty file under the first free name. It is removed with
    /// the other results if the job does not finish.
    fn reserve(&mut self, dir: &Path, name: &str) -> Result<(fs::File, PathBuf), JobError> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                format!("invalid output file name `{name}`"),
            ));
        }
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
            _ => (name, None),
        };

        for n in 1u32.. {
            let candidate = match (n, ext) {
                (1, _) => name.to_owned(),
                (_, Some(ext)) => format!("{stem} ({n}).{ext}"),
                (_, None) => format!("{stem} ({n})"),
            };
            let path = dir.join(candidate);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    self.created.push(path.clone());
                    return Ok((file, path));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!("an unbounded range always yields a free name")
    }

    /// Keeps the files and returns their paths.
    pub fn finish(mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.created)
    }
}

impl Drop for Outputs {
    fn drop(&mut self) {
        for path in &self.created {
            let _ = fs::remove_file(path);
        }
    }
}
