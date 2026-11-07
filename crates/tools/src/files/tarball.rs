//! Create and Extract TAR. Extraction has the same protections as ZIP: nothing
//! lands outside the new folder, links are skipped, and size is limited.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};
use tar::{Archive, Builder, EntryType};

use super::archive::{Budget, COUNTED_ENTRIES, Contents, LIMITS, Limits, unique};
use crate::output::{Outputs, file_stem, input_files, name_error, render_name, target_dir};

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn pack(ctx: &JobContext, builder: &mut Builder<GzEncoder<File>>, path: &Path, name: &Path) -> Result<(), JobError> {
    ctx.check_cancelled()?;
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        builder.append_dir(name, path)?;
        let mut children: Vec<PathBuf> = fs::read_dir(path)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()?;
        children.sort();
        for child in children {
            let inner = name.join(child.file_name().unwrap_or_default());
            pack(ctx, builder, &child, &inner)?;
        }
    } else if meta.is_file() {
        builder.append_path_with_name(path, name)?;
    }
    Ok(())
}

pub fn create(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let archive = ctx.temp_dir()?.join("result.tar.gz");
        let encoder = GzEncoder::new(File::create(&archive)?, Compression::default());
        let mut builder = Builder::new(encoder);
        builder.follow_symlinks(false);
        let mut taken = HashSet::new();
        for (i, path) in files.iter().enumerate() {
            let leaf = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let name = PathBuf::from(unique(&leaf, &mut taken));
            pack(ctx, &mut builder, path, &name).map_err(|e| name_error(path, e))?;
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        builder.into_inner()?.finish()?;

        let name = render_name(&template, &[("stem", &file_stem(&files[0]))]);
        let size = fs::metadata(&archive)?.len();
        let mut outputs = Outputs::new();
        let path = outputs.adopt(&target_dir(ctx, &files[0]), &name, &archive)?;
        outputs.finish();
        Ok(JobOutput::Files(vec![OutputFile {
            path,
            size,
            source_size: None,
            note: None,
        }]))
    })
}

/// A path inside the target, or `None` if the name tries to leave it.
fn inside(name: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Stops reading after `left` bytes. The tar reader keeps long file names in
/// memory before any entry reaches the budget, so the stream has its own limit.
struct Capped<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("this archive expands to far more than it should"));
        }
        let room = buffer.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let read = self.inner.read(&mut buffer[..room])?;
        self.left -= read as u64;
        Ok(read)
    }
}

/// Unpacks into `target`, which must exist. Returns the bytes written and
/// how many links were skipped.
pub fn unpack(
    ctx: &JobContext,
    reader: impl Read,
    size: u64,
    target: &Path,
    limits: Limits,
) -> Result<(u64, usize), JobError> {
    // Room for headers on top of what the budget allows the files.
    let cap = limits
        .total_bytes
        .min(size.saturating_mul(limits.ratio).max(limits.ratio_floor))
        + 16 * 1024 * 1024;
    let mut archive = Archive::new(Capped {
        inner: reader,
        left: cap,
    });
    let mut budget = Budget::new(limits, size);
    let (mut count, mut skipped) = (0, 0);
    let damaged = |_| JobError::new(ErrorKind::UnsupportedFormat, "this archive is damaged");
    for entry in archive.entries().map_err(damaged)? {
        ctx.check_cancelled()?;
        let mut entry = entry.map_err(damaged)?;
        count += 1;
        if count > limits.entries {
            return Err(invalid("this archive has too many entries"));
        }
        let name = entry.path().map_err(damaged)?.into_owned();
        let Some(relative) = inside(&name) else {
            return Err(invalid(format!(
                "the entry `{}` points outside the archive",
                name.display()
            )));
        };
        let path = target.join(relative);
        match entry.header().entry_type() {
            EntryType::Directory => fs::create_dir_all(&path)?,
            EntryType::Regular | EntryType::Continuous | EntryType::GNUSparse => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| {
                        if e.kind() == io::ErrorKind::AlreadyExists {
                            invalid(format!("the entry `{}` appears twice in this archive", name.display()))
                        } else {
                            e.into()
                        }
                    })?;
                super::archive::copy(ctx, &mut entry, &mut file, |n| budget.spend(n))?;
            }
            EntryType::XGlobalHeader | EntryType::XHeader | EntryType::GNULongName | EntryType::GNULongLink => {}
            _ => skipped += 1,
        }
    }
    Ok((budget.written(), skipped))
}

fn is_gzip(path: &Path) -> Result<bool, JobError> {
    let mut magic = [0; 2];
    let mut file = File::open(path)?;
    Ok(file.read_exact(&mut magic).is_ok() && magic == [0x1F, 0x8B])
}

/// Stops a listing that would have to read too much of the archive.
const LISTING_CAP: u64 = 2 * 1024 * 1024 * 1024;

fn list(reader: impl Read) -> Result<Contents, JobError> {
    let mut archive = Archive::new(Capped {
        inner: reader,
        left: LISTING_CAP,
    });
    let mut found = Contents {
        count: 0,
        total: 0,
        entries: Vec::new(),
        more: false,
    };
    let damaged = |_| JobError::new(ErrorKind::UnsupportedFormat, "this archive is damaged");
    for entry in archive.entries().map_err(damaged)? {
        let Ok(entry) = entry else {
            // Ran into the cap: what was counted so far still stands.
            found.more = found.count > 0;
            if found.more {
                break;
            }
            return Err(JobError::new(ErrorKind::UnsupportedFormat, "this archive is damaged"));
        };
        if found.count >= COUNTED_ENTRIES {
            found.more = true;
            break;
        }
        if entry.header().entry_type().is_file() {
            let name = entry.path().map_err(damaged)?.display().to_string();
            found.add(name, entry.size());
        }
    }
    Ok(found)
}

/// The files in a tar or tar.gz, without writing anything.
pub fn contents(path: &Path) -> Result<Contents, JobError> {
    let file = BufReader::new(File::open(path)?);
    if is_gzip(path)? {
        list(GzDecoder::new(file))
    } else {
        list(file)
    }
}

/// `backup.tar.gz` → `backup`.
fn folder_name(path: &Path) -> String {
    let stem = file_stem(path);
    stem.strip_suffix(".tar").map_or_else(|| stem.clone(), str::to_owned)
}

pub fn extract(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, _: &Params| {
        let archives = input_files(input)?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(archives.len());
        for (i, path) in archives.iter().enumerate() {
            let name = render_name(&template, &[("stem", &folder_name(path))]);
            let size = fs::metadata(path)?.len();
            let target = outputs.folder(&target_dir(ctx, path), &name)?;
            let file = BufReader::new(File::open(path)?);
            let unpacked = if is_gzip(path)? {
                unpack(ctx, GzDecoder::new(file), size, &target, LIMITS)
            } else {
                unpack(ctx, file, size, &target, LIMITS)
            };
            let (written, skipped) = unpacked.map_err(|e| name_error(path, e))?;
            let note = (skipped > 0).then(|| format!("{skipped} links were skipped."));
            results.push(OutputFile {
                path: target,
                size: written,
                source_size: Some(size),
                note,
            });
            ctx.progress((i + 1) as f32 / archives.len() as f32);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}
