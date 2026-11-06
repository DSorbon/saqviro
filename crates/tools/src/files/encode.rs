//! File to Base64.

use std::{fs, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, Params};

use crate::{
    output::{input_files, name_error},
    params,
};

/// Text this long is more than a text box can usefully hold.
const LARGEST: u64 = 32 * 1024 * 1024;

pub fn to_base64() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let mut parts = Vec::with_capacity(files.len());
        for path in &files {
            ctx.check_cancelled()?;
            let too_big = || {
                JobError::new(
                    ErrorKind::InvalidInput,
                    "this file is too large for text; the limit is 32 MB",
                )
            };
            if fs::metadata(path)?.len() > LARGEST {
                return Err(name_error(path, too_big()));
            }
            let text = STANDARD.encode(fs::read(path)?);
            parts.push(if params::flag(params, "data_uri") {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                format!("data:{mime};base64,{text}")
            } else {
                text
            });
        }
        Ok(JobOutput::Text(parts.join("\n\n")))
    })
}
