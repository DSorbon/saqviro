//! Tools that look through folders. They only read: nothing is changed, and
//! links are not followed, so a link cannot lead them outside what was given.

use std::{
    collections::HashMap,
    fmt::Write,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, Params};
use sha2::{Digest, Sha256};

use crate::{output::input_files, params};

const CHUNK: usize = 1024 * 1024;
/// Files of this size or less are not worth comparing as duplicates.
const SMALLEST_DUPLICATE: u64 = 1;

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// A file found in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
}

/// What a walk found. What could not be read is counted, not fatal.
#[derive(Debug, Default)]
pub struct Walk {
    pub files: Vec<Found>,
    pub empty_folders: Vec<PathBuf>,
    pub unreadable: usize,
}

fn visit(ctx: &JobContext, path: &Path, walk: &mut Walk) -> Result<(), JobError> {
    ctx.check_cancelled()?;
    let Ok(meta) = fs::symlink_metadata(path) else {
        walk.unreadable += 1;
        return Ok(());
    };
    if meta.is_file() {
        walk.files.push(Found {
            path: path.to_path_buf(),
            size: meta.len(),
        });
    } else if meta.is_dir() {
        let Ok(entries) = fs::read_dir(path) else {
            walk.unreadable += 1;
            return Ok(());
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        children.sort();
        if children.is_empty() {
            walk.empty_folders.push(path.to_path_buf());
        }
        for child in children {
            visit(ctx, &child, walk)?;
        }
    }
    Ok(())
}

pub fn walk(ctx: &JobContext, roots: &[PathBuf]) -> Result<Walk, JobError> {
    let mut found = Walk::default();
    for root in roots {
        visit(ctx, root, &mut found)?;
    }
    Ok(found)
}

fn unreadable_note(out: &mut String, walk: &Walk) {
    if walk.unreadable > 0 {
        let _ = write!(out, "\n{} items could not be read.", walk.unreadable);
    }
}

fn text_tool(run: fn(&JobContext, &[PathBuf], &Params) -> Result<String, JobError>) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let roots = input_files(input)?;
        run(ctx, &roots, params).map(JobOutput::Text)
    })
}

fn tree_of(
    ctx: &JobContext,
    dir: &Path,
    prefix: &str,
    depth: u32,
    hidden: bool,
    out: &mut String,
) -> Result<(), JobError> {
    ctx.check_cancelled()?;
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut children: Vec<_> = entries
        .flatten()
        .filter(|e| hidden || !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    children.sort_by_key(|p| name_of(p).to_lowercase());
    let count = children.len();
    for (i, child) in children.iter().enumerate() {
        let last = i + 1 == count;
        let is_dir = fs::symlink_metadata(child).is_ok_and(|m| m.is_dir());
        let _ = writeln!(
            out,
            "{prefix}{}{}{}",
            if last { "└── " } else { "├── " },
            name_of(child),
            if is_dir { "/" } else { "" }
        );
        if is_dir && depth > 1 {
            let deeper = format!("{prefix}{}", if last { "    " } else { "│   " });
            tree_of(ctx, child, &deeper, depth - 1, hidden, out)?;
        }
    }
    Ok(())
}

pub fn tree() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, params| {
        let depth = params::number(params, "max_depth", 5).max(1);
        let hidden = params::flag(params, "show_hidden");
        let mut out = String::new();
        for root in roots {
            let _ = writeln!(out, "{}/", name_of(root));
            tree_of(ctx, root, "", depth, hidden, &mut out)?;
        }
        Ok(out)
    })
}

fn top_lines(mut sized: Vec<(u64, String)>, top: usize) -> String {
    sized.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let shown = sized.iter().take(top);
    shown
        .map(|(size, name)| format!("{:>10}  {name}", format_size(*size)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn folder_size() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, params| {
        let top = params::number(params, "top", 30) as usize;
        let mut out = String::new();
        for root in roots {
            let found = walk(ctx, std::slice::from_ref(root))?;
            let total: u64 = found.files.iter().map(|f| f.size).sum();
            let _ = writeln!(
                out,
                "{}: {} in {} files\n",
                name_of(root),
                format_size(total),
                found.files.len()
            );
            let mut groups: HashMap<String, u64> = HashMap::new();
            for file in &found.files {
                let inside = file.path.strip_prefix(root).unwrap_or(&file.path);
                let part = inside
                    .components()
                    .next()
                    .map_or_else(|| name_of(&file.path), |c| c.as_os_str().to_string_lossy().into_owned());
                *groups.entry(part).or_default() += file.size;
            }
            out.push_str(&top_lines(groups.into_iter().map(|(n, s)| (s, n)).collect(), top));
            unreadable_note(&mut out, &found);
            out.push_str("\n\n");
        }
        Ok(out.trim_end().to_owned())
    })
}

pub fn large() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, params| {
        let found = walk(ctx, roots)?;
        let least = u64::from(params::number(params, "min_mb", 10)) * 1024 * 1024;
        let big: Vec<(u64, String)> = found
            .files
            .iter()
            .filter(|f| f.size >= least)
            .map(|f| (f.size, f.path.display().to_string()))
            .collect();
        if big.is_empty() {
            return Ok("No files that large".into());
        }
        let mut out = top_lines(big, params::number(params, "top", 50) as usize);
        unreadable_note(&mut out, &found);
        Ok(out)
    })
}

fn content_hash(ctx: &JobContext, path: &Path) -> Result<blake3::Hash, JobError> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; CHUNK];
    loop {
        ctx.check_cancelled()?;
        match file.read(&mut buffer)? {
            0 => return Ok(hasher.finalize()),
            n => hasher.update(&buffer[..n]),
        };
    }
}

pub fn duplicates() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, _| {
        let found = walk(ctx, roots)?;
        // Only files of the same size can be the same, which spares reading most.
        let mut by_size: HashMap<u64, Vec<&Found>> = HashMap::new();
        for file in found.files.iter().filter(|f| f.size >= SMALLEST_DUPLICATE) {
            by_size.entry(file.size).or_default().push(file);
        }
        let candidates: Vec<&Found> = by_size.values().filter(|g| g.len() > 1).flatten().copied().collect();
        let mut groups: HashMap<blake3::Hash, Vec<&Found>> = HashMap::new();
        for (i, file) in candidates.iter().enumerate() {
            ctx.progress(i as f32 / candidates.len() as f32);
            if let Ok(hash) = content_hash(ctx, &file.path) {
                groups.entry(hash).or_default().push(file);
            } else {
                ctx.check_cancelled()?;
            }
        }
        let mut same: Vec<Vec<&Found>> = groups.into_values().filter(|g| g.len() > 1).collect();
        if same.is_empty() {
            return Ok("No duplicates found".into());
        }
        for group in &mut same {
            group.sort_by(|a, b| a.path.cmp(&b.path));
        }
        same.sort_by(|a, b| {
            (b[0].size * b.len() as u64)
                .cmp(&(a[0].size * a.len() as u64))
                .then(a[0].path.cmp(&b[0].path))
        });
        let wasted: u64 = same.iter().map(|g| g[0].size * (g.len() as u64 - 1)).sum();
        let mut out = format!(
            "{} sets of duplicates, {} could be freed\n",
            same.len(),
            format_size(wasted)
        );
        for group in &same {
            let _ = writeln!(out, "\n{} × {}", group.len(), format_size(group[0].size));
            for file in group {
                let _ = writeln!(out, "{}", file.path.display());
            }
        }
        Ok(out)
    })
}

pub fn empty() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, _| {
        let found = walk(ctx, roots)?;
        let files: Vec<_> = found.files.iter().filter(|f| f.size == 0).collect();
        if files.is_empty() && found.empty_folders.is_empty() {
            return Ok("Nothing empty found".into());
        }
        let mut out = String::new();
        if !found.empty_folders.is_empty() {
            let _ = writeln!(out, "Empty folders: {}", found.empty_folders.len());
            for folder in &found.empty_folders {
                let _ = writeln!(out, "{}", folder.display());
            }
        }
        if !files.is_empty() {
            let gap = if out.is_empty() { "" } else { "\n" };
            let _ = writeln!(out, "{gap}Empty files: {}", files.len());
            for file in files {
                let _ = writeln!(out, "{}", file.path.display());
            }
        }
        Ok(out)
    })
}

/// One hash of the names and contents under `root`. Two folders with the
/// same files in the same places have the same hash, wherever they are.
pub fn folder_hash(ctx: &JobContext, root: &Path) -> Result<(String, usize), JobError> {
    let found = walk(ctx, std::slice::from_ref(&root.to_path_buf()))?;
    if found.unreadable > 0 {
        return Err(JobError::new(
            ErrorKind::PermissionDenied,
            format!("{} items could not be read", found.unreadable),
        ));
    }
    let mut entries: Vec<(String, &Found)> = found
        .files
        .iter()
        .map(|f| {
            let inside = f.path.strip_prefix(root).unwrap_or(&f.path);
            let parts: Vec<_> = inside.components().map(|c| c.as_os_str().to_string_lossy()).collect();
            (parts.join("/"), f)
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut all = Sha256::new();
    for (name, file) in &entries {
        all.update(name.as_bytes());
        all.update([0]);
        all.update(content_hash(ctx, &file.path)?.as_bytes());
    }
    let hex: String = all.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok((hex, entries.len()))
}

pub fn folder_checksum() -> Arc<dyn Executor> {
    text_tool(|ctx, roots, _| {
        let lines = roots
            .iter()
            .map(|root| {
                let (hash, count) = folder_hash(ctx, root)?;
                Ok(format!("{hash}  {} ({count} files)", name_of(root)))
            })
            .collect::<Result<Vec<_>, JobError>>()?;
        Ok(lines.join("\n"))
    })
}
