//! Batch Rename, the one tool that changes the user's files in place. It
//! previews by default, checks every name before touching anything, and puts
//! everything back if a rename fails on the way.

use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    process,
    sync::Arc,
};

use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};

use crate::{output::input_files, params};

/// Characters that are not allowed in a name on at least one system.
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub find: String,
    pub replace: String,
    pub prefix: String,
    pub suffix: String,
    /// First number, if files get numbered.
    pub number_from: Option<u32>,
    pub case: Case,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Case {
    #[default]
    Keep,
    Lower,
    Upper,
}

impl Rule {
    fn from_params(params: &Params) -> Self {
        Self {
            find: params::text(params, "find").to_owned(),
            replace: params::text(params, "replace").to_owned(),
            prefix: params::text(params, "prefix").to_owned(),
            suffix: params::text(params, "suffix").to_owned(),
            number_from: params::flag(params, "numbers").then(|| params::number(params, "start", 1)),
            case: match params::text(params, "case") {
                "lower" => Case::Lower,
                "upper" => Case::Upper,
                _ => Case::Keep,
            },
        }
    }

    fn does_nothing(&self) -> bool {
        self.find.is_empty()
            && self.prefix.is_empty()
            && self.suffix.is_empty()
            && self.number_from.is_none()
            && self.case == Case::Keep
    }

    /// The new name. The extension never changes, except for its case.
    pub fn apply(&self, name: &str, index: usize, count: usize) -> Result<String, String> {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
            _ => (name, None),
        };
        let mut new = if self.find.is_empty() {
            stem.to_owned()
        } else {
            stem.replace(&self.find, &self.replace)
        };
        new = format!("{}{new}{}", self.prefix, self.suffix);
        if let Some(from) = self.number_from {
            let last = u64::from(from) + count as u64 - 1;
            let width = last.to_string().len();
            new = format!("{new}-{:0width$}", u64::from(from) + index as u64);
        }
        let mut full = match ext {
            Some(ext) => format!("{new}.{ext}"),
            None => new.clone(),
        };
        full = match self.case {
            Case::Keep => full,
            Case::Lower => full.to_lowercase(),
            Case::Upper => full.to_uppercase(),
        };
        if new.trim().is_empty() || full.trim_matches('.').is_empty() {
            return Err(format!("`{name}` would be left without a name"));
        }
        if let Some(c) = full.chars().find(|c| FORBIDDEN.contains(c) || c.is_control()) {
            return Err(format!("a file name cannot contain `{}`", c.escape_default()));
        }
        Ok(full)
    }
}

fn leaf(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Works out every rename and refuses the whole batch if any of them could
/// lose a file. Pairs of unchanged names are left out.
pub fn plan(paths: &[PathBuf], rule: &Rule) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    if rule.does_nothing() {
        return Err("choose what to change: find, prefix, suffix, numbers or case".into());
    }
    let sources: HashSet<PathBuf> = paths.iter().filter_map(|p| fs::canonicalize(p).ok()).collect();
    if sources.len() != paths.len() {
        return Err("some files are missing or listed twice".into());
    }
    let mut seen = HashSet::new();
    let mut moves = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let target = path.with_file_name(rule.apply(&leaf(path), i, paths.len())?);
        // Lowercased, because two names differing in case are one file on
        // many disks.
        if !seen.insert(target.to_string_lossy().to_lowercase()) {
            return Err(format!("two files would be named `{}`", leaf(&target)));
        }
        if target == *path {
            continue;
        }
        // A target that is itself being renamed will be free by then. This
        // also covers changing only the case on a case-insensitive disk.
        let occupied = fs::symlink_metadata(&target).is_ok();
        if occupied && !fs::canonicalize(&target).is_ok_and(|t| sources.contains(&t)) {
            return Err(format!("`{}` already exists", leaf(&target)));
        }
        moves.push((path.clone(), target));
    }
    Ok(moves)
}

fn step_aside(path: &Path, index: usize) -> PathBuf {
    path.with_file_name(format!(".saqviro-rename-{}-{index}", process::id()))
}

/// Applies the plan in two steps, through temporary names, so swaps and
/// chains work. On failure every file gets its old name back.
pub fn apply(moves: &[(PathBuf, PathBuf)], rename: impl Fn(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    let mut undo: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(moves.len() * 2);
    let mut run = || -> io::Result<()> {
        for (i, (from, _)) in moves.iter().enumerate() {
            let aside = step_aside(from, i);
            rename(from, &aside)?;
            undo.push((aside, from.clone()));
        }
        for (i, (from, to)) in moves.iter().enumerate() {
            if fs::symlink_metadata(to).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} appeared", leaf(to)),
                ));
            }
            let aside = step_aside(from, i);
            rename(&aside, to)?;
            undo.push((to.clone(), aside));
        }
        Ok(())
    };
    let result = run();
    if result.is_err() {
        for (now, before) in undo.iter().rev() {
            let _ = fs::rename(now, before);
        }
    }
    result
}

pub fn run() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let moves = plan(&files, &Rule::from_params(params)).map_err(|e| JobError::new(ErrorKind::InvalidInput, e))?;

        if params::flag(params, "preview") {
            let mut lines: Vec<String> = moves
                .iter()
                .map(|(from, to)| format!("{} → {}", leaf(from), leaf(to)))
                .collect();
            if lines.is_empty() {
                lines.push("Nothing would change.".to_owned());
            }
            return Ok(JobOutput::Text(lines.join("\n")));
        }

        ctx.check_cancelled()?;
        apply(&moves, |from, to| fs::rename(from, to))?;
        let results = moves
            .iter()
            .map(|(from, to)| OutputFile {
                path: to.clone(),
                size: fs::metadata(to).map_or(0, |m| m.len()),
                source_size: None,
                note: Some(format!("Was {}", leaf(from))),
            })
            .collect();
        Ok(JobOutput::Files(results))
    })
}
