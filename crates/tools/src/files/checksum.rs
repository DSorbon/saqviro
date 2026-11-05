//! SHA-256 Calculator and Verify File Checksum. Files are read in pieces, so
//! size is no problem, and a running job can be cancelled between pieces.

use std::{fs::File, io, path::Path, sync::Arc};

use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, Params};

use crate::{
    developer::hash::{Algorithm, hash},
    output::{input_files, name_error},
    params,
};

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Hashes each file, reporting progress by bytes across all of them.
fn hash_files(ctx: &JobContext, files: &[std::path::PathBuf], algorithm: Algorithm) -> Result<Vec<String>, JobError> {
    let sizes: Vec<u64> = files
        .iter()
        .map(|f| {
            let meta = std::fs::metadata(f).map_err(|e| name_error(f, e.into()))?;
            if meta.is_dir() {
                return Err(name_error(
                    f,
                    JobError::new(ErrorKind::InvalidInput, "folders have no checksum"),
                ));
            }
            Ok(meta.len())
        })
        .collect::<Result<_, _>>()?;
    let total = sizes.iter().sum::<u64>().max(1);
    let mut done = 0;
    files
        .iter()
        .map(|path| {
            let mut file = File::open(path).map_err(|e| name_error(path, e.into()))?;
            let result = hash(algorithm, &mut file, |n| {
                if ctx.is_cancelled() {
                    return Err(io::Error::other("cancelled"));
                }
                done += n;
                ctx.progress(done as f32 / total as f32);
                Ok(())
            });
            ctx.check_cancelled()?;
            result.map_err(|e| name_error(path, e.into()))
        })
        .collect()
}

/// Lines in the format `shasum -c` reads.
pub fn sha256() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let hashes = hash_files(ctx, &files, Algorithm::Sha256)?;
        let lines: Vec<String> = hashes
            .iter()
            .zip(&files)
            .map(|(hash, path)| format!("{hash}  {}", name(path)))
            .collect();
        Ok(JobOutput::Text(lines.join("\n")))
    })
}

/// Reads a checksum as people paste it: any case, with a `sha256:` prefix
/// or a file name after it. The length tells the algorithm.
pub fn parse_expected(text: &str) -> Result<(Algorithm, String), String> {
    let first = text.split_whitespace().next().unwrap_or_default();
    let hex = first.rsplit(':').next().unwrap_or_default().to_ascii_lowercase();
    if hex.is_empty() {
        return Err("paste the expected checksum".into());
    }
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("a checksum has only the digits 0-9 and letters a-f".into());
    }
    let algorithm = match hex.len() {
        32 => Algorithm::Md5,
        40 => Algorithm::Sha1,
        64 => Algorithm::Sha256,
        128 => Algorithm::Sha512,
        n => {
            return Err(format!(
                "a checksum of {n} characters is not MD5, SHA-1, SHA-256 or SHA-512"
            ));
        }
    };
    Ok((algorithm, hex))
}

/// A mismatch is an answer, not a failure, so the job still completes.
pub fn verify() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let (algorithm, expected) =
            parse_expected(params::text(params, "expected")).map_err(|e| JobError::new(ErrorKind::InvalidInput, e))?;
        let hashes = hash_files(ctx, &files, algorithm)?;
        let lines: Vec<String> = hashes
            .iter()
            .zip(&files)
            .map(|(actual, path)| {
                if *actual == expected {
                    format!("✓ {} matches ({})", name(path), algorithm.name())
                } else {
                    format!(
                        "✗ {} does NOT match\nExpected: {expected}\nActual:   {actual}",
                        name(path)
                    )
                }
            })
            .collect();
        Ok(JobOutput::Text(lines.join("\n\n")))
    })
}
