//! Create ZIP and Extract ZIP. Archives are untrusted: entries cannot leave
//! the target folder, nothing is overwritten, links are never created, and
//! limits stop archives that expand without end.

use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{Datelike, Local, Timelike};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, result::ZipError, write::SimpleFileOptions};

use crate::{
    output::{Outputs, file_stem, input_files, name_error, render_name, target_dir},
    params,
};

const CHUNK: usize = 1024 * 1024;

/// Limits for what one archive may unpack to.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub entries: usize,
    pub total_bytes: u64,
    /// Output may be this many times the size of the archive…
    pub ratio: u64,
    /// …once it is larger than this.
    pub ratio_floor: u64,
}

pub const LIMITS: Limits = Limits {
    entries: 1_000_000,
    total_bytes: 100 * 1024 * 1024 * 1024,
    ratio: 250,
    ratio_floor: 1024 * 1024 * 1024,
};

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn zip_error(e: ZipError) -> JobError {
    match e {
        ZipError::Io(e) => e.into(),
        ZipError::UnsupportedArchive(what) => JobError::new(ErrorKind::UnsupportedFormat, what.to_string()),
        other => JobError::new(
            ErrorKind::UnsupportedFormat,
            format!("this archive is damaged: {other}"),
        ),
    }
}

/// Copies in pieces so a job can be cancelled in the middle of a large file.
pub(super) fn copy(
    ctx: &JobContext,
    from: &mut impl Read,
    to: &mut impl Write,
    mut on_chunk: impl FnMut(u64) -> Result<(), JobError>,
) -> Result<(), JobError> {
    let mut buffer = vec![0; CHUNK];
    loop {
        ctx.check_cancelled()?;
        let n = from.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        on_chunk(n as u64)?;
        to.write_all(&buffer[..n])?;
    }
}

/// One thing to store.
enum Item {
    File(PathBuf),
    Folder,
}

/// Lists a file or folder for the archive, under `name`. Links are skipped:
/// following one could pull in anything on the disk.
fn walk(path: &Path, name: &str, items: &mut Vec<(String, Item)>) -> Result<(), JobError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_symlink() {
        return Ok(());
    }
    if !meta.is_dir() {
        items.push((name.to_owned(), Item::File(path.to_path_buf())));
        return Ok(());
    }
    items.push((format!("{name}/"), Item::Folder));
    let mut children: Vec<PathBuf> = fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    children.sort();
    for child in children {
        let leaf = child
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        walk(&child, &format!("{name}/{leaf}"), items)?;
    }
    Ok(())
}

/// `name`, or `name (2)` and so on when two inputs share a name.
fn unique(name: &str, taken: &mut HashSet<String>) -> String {
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
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("an unbounded range always yields a free name")
}

fn modified(path: &Path) -> Option<DateTime> {
    let time: chrono::DateTime<Local> = fs::metadata(path).ok()?.modified().ok()?.into();
    DateTime::from_date_and_time(
        u16::try_from(time.year()).ok()?,
        time.month() as u8,
        time.day() as u8,
        time.hour() as u8,
        time.minute() as u8,
        time.second() as u8,
    )
    .ok()
}

fn write_archive(ctx: &JobContext, target: &Path, items: &[(String, Item)], level: u32) -> Result<(), JobError> {
    let sizes = items.iter().map(|(_, item)| match item {
        Item::File(path) => fs::metadata(path).map_or(0, |m| m.len()),
        Item::Folder => 0,
    });
    let total = sizes.sum::<u64>().max(1);
    let mut options = SimpleFileOptions::default().large_file(true).unix_permissions(0o644);
    options = if level == 0 {
        options.compression_method(CompressionMethod::Stored)
    } else {
        options
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(level)))
    };

    let mut zip = ZipWriter::new(File::create(target)?);
    let mut done = 0;
    for (name, item) in items {
        match item {
            Item::Folder => zip.add_directory(name.as_str(), options).map_err(zip_error)?,
            Item::File(path) => {
                let options = modified(path).map_or(options, |t| options.last_modified_time(t));
                zip.start_file(name.as_str(), options).map_err(zip_error)?;
                let mut file = File::open(path).map_err(|e| name_error(path, e.into()))?;
                copy(ctx, &mut file, &mut zip, |n| {
                    done += n;
                    ctx.progress(done as f32 / total as f32 * 0.99);
                    Ok(())
                })?;
            }
        }
    }
    zip.finish().map_err(zip_error)?;
    Ok(())
}

/// Create ZIP, from files and folders, named after the first of them.
pub fn create(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let inputs = input_files(input)?;
        let mut items = Vec::new();
        let mut taken = HashSet::new();
        for path in &inputs {
            let leaf = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let leaf = leaf.ok_or_else(|| invalid("this file has no name"))?;
            walk(path, &unique(&leaf, &mut taken), &mut items).map_err(|e| name_error(path, e))?;
        }
        if !items.iter().any(|(_, item)| matches!(item, Item::File(_))) {
            return Err(invalid("there are no files to put in the archive"));
        }

        let archive = ctx.temp_dir()?.join("result.zip");
        write_archive(ctx, &archive, &items, params::number(params, "level", 6).min(9))?;
        ctx.check_cancelled()?;

        let name = render_name(&template, &[("stem", &file_stem(&inputs[0]))]);
        let dir = target_dir(ctx, &inputs[0]);
        let size = fs::metadata(&archive)?.len();
        let mut outputs = Outputs::new();
        let path = outputs.adopt(&dir, &name, &archive)?;
        let source_size = items.iter().filter_map(|(_, item)| match item {
            Item::File(path) => fs::metadata(path).ok().map(|m| m.len()),
            Item::Folder => None,
        });
        let result = OutputFile {
            path,
            size,
            source_size: Some(source_size.sum()),
            note: None,
        };
        outputs.finish();
        Ok(JobOutput::Files(vec![result]))
    })
}

/// Stops extraction once it has written too much.
pub(super) struct Budget {
    limits: Limits,
    archive_size: u64,
    written: u64,
}

impl Budget {
    pub(super) fn new(limits: Limits, archive_size: u64) -> Self {
        Self {
            limits,
            archive_size,
            written: 0,
        }
    }

    pub(super) fn written(&self) -> u64 {
        self.written
    }

    pub(super) fn spend(&mut self, bytes: u64) -> Result<(), JobError> {
        self.written += bytes;
        let ratio_limit = self
            .archive_size
            .saturating_mul(self.limits.ratio)
            .max(self.limits.ratio_floor);
        if self.written > self.limits.total_bytes || self.written > ratio_limit {
            return Err(invalid(
                "this archive expands to far more than it should, so it was not extracted",
            ));
        }
        Ok(())
    }
}

/// Unpacks into `target`, which must exist. Returns the bytes written.
pub fn unpack(
    ctx: &JobContext,
    archive: impl Read + Seek,
    archive_size: u64,
    target: &Path,
    limits: Limits,
    mut on_progress: impl FnMut(f32),
) -> Result<(u64, usize), JobError> {
    let mut zip = ZipArchive::new(archive).map_err(zip_error)?;
    if zip.len() > limits.entries {
        return Err(invalid("this archive has too many entries"));
    }
    let mut budget = Budget::new(limits, archive_size);
    let mut skipped = 0;
    let count = zip.len();
    for i in 0..count {
        ctx.check_cancelled()?;
        let mut entry = zip.by_index(i).map_err(|e| match e {
            ZipError::UnsupportedArchive(what) if what.to_lowercase().contains("password") => JobError::new(
                ErrorKind::UnsupportedFormat,
                "password-protected archives are not supported yet",
            ),
            other => zip_error(other),
        })?;
        if entry.encrypted() {
            return Err(JobError::new(
                ErrorKind::UnsupportedFormat,
                "password-protected archives are not supported yet",
            ));
        }
        let shown = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        // `None` is a name that would leave the target folder.
        let Some(relative) = entry.enclosed_name() else {
            return Err(invalid(format!("the entry `{}` points outside the archive", shown)));
        };
        if entry.is_symlink() {
            skipped += 1;
            continue;
        }
        let path = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    invalid(format!("the entry `{}` appears twice in this archive", shown))
                } else {
                    e.into()
                }
            })?;
        copy(ctx, &mut entry, &mut file, |n| budget.spend(n)).map_err(|e| match e.kind {
            ErrorKind::Unknown | ErrorKind::InvalidInput if e.message.contains("checksum") => {
                JobError::new(ErrorKind::UnsupportedFormat, "this archive is damaged")
            }
            _ => e,
        })?;
        on_progress((i + 1) as f32 / count as f32);
    }
    Ok((budget.written, skipped))
}

/// Extract ZIP, into a new folder next to each archive.
pub fn extract(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, _: &Params| {
        let archives = input_files(input)?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(archives.len());
        for (i, path) in archives.iter().enumerate() {
            let share = 1.0 / archives.len() as f32;
            let name = render_name(&template, &[("stem", &file_stem(path))]);
            let dir = target_dir(ctx, path);
            let file = File::open(path).map_err(|e| name_error(path, e.into()))?;
            let source_size = file.metadata()?.len();
            let target = outputs.folder(&dir, &name)?;
            let (size, skipped) = unpack(ctx, file, source_size, &target, LIMITS, |done| {
                ctx.progress((i as f32 + done) * share);
            })
            .map_err(|e| name_error(path, e))?;
            results.push(OutputFile {
                path: target,
                size,
                source_size: Some(source_size),
                note: (skipped > 0).then(|| format!("{skipped} links in the archive were skipped.")),
            });
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}
