//! Encrypt and Decrypt Files, in the age format: an open, reviewed design
//! that other programs can read, rather than a cipher of our own.

use std::{
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    iter,
    path::Path,
    sync::Arc,
};

use age::secrecy::SecretString;
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};

use crate::{
    output::{Outputs, file_stem, input_files, name_error, render_name, target_dir},
    params,
};

const CHUNK: usize = 1024 * 1024;

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn passphrase(params: &Params) -> Result<SecretString, JobError> {
    let text = params::text(params, "passphrase");
    if text.is_empty() {
        return Err(invalid("enter a passphrase"));
    }
    Ok(SecretString::from(text.to_owned()))
}

fn copy(ctx: &JobContext, from: &mut impl Read, to: &mut impl Write) -> Result<(), JobError> {
    let mut buffer = vec![0; CHUNK];
    loop {
        ctx.check_cancelled()?;
        match from.read(&mut buffer)? {
            0 => return Ok(()),
            n => to.write_all(&buffer[..n])?,
        }
    }
}

fn each_file(
    template: String,
    work: impl Fn(&JobContext, &Path, &Path, &SecretString) -> Result<(), JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let secret = passphrase(params)?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            let temp = ctx.temp_dir()?.join(format!("{i}.part"));
            work(ctx, path, &temp, &secret).map_err(|e| name_error(path, e))?;
            let full = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = render_name(&template, &[("name", &full), ("stem", &file_stem(path))]);
            let size = fs::metadata(&temp)?.len();
            let written = outputs.adopt(&target_dir(ctx, path), &name, &temp)?;
            let source_size = fs::metadata(path).ok().map(|m| m.len());
            results.push(OutputFile {
                path: written,
                size,
                source_size,
                note: None,
            });
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

pub fn encrypt(template: String) -> Arc<dyn Executor> {
    each_file(template, |ctx, path, target, secret| {
        if !fs::metadata(path)?.is_file() {
            return Err(invalid("folders cannot be encrypted; put them in a ZIP first"));
        }
        let encryptor = age::Encryptor::with_user_passphrase(secret.clone());
        let mut writer = encryptor
            .wrap_output(File::create(target)?)
            .map_err(|e| JobError::new(ErrorKind::Unknown, e.to_string()))?;
        copy(ctx, &mut File::open(path)?, &mut writer)?;
        writer.finish()?;
        Ok(())
    })
}

pub fn decrypt(template: String) -> Arc<dyn Executor> {
    each_file(template, |ctx, path, target, secret| {
        let not_age = |_| JobError::new(ErrorKind::UnsupportedFormat, "this is not an age file");
        let decryptor = age::Decryptor::new(BufReader::new(File::open(path)?)).map_err(not_age)?;
        if !decryptor.is_scrypt() {
            return Err(JobError::new(
                ErrorKind::UnsupportedFormat,
                "this file was locked with a key, not a passphrase",
            ));
        }
        let identity = age::scrypt::Identity::new(secret.clone());
        let mut reader = decryptor
            .decrypt(iter::once(&identity as &dyn age::Identity))
            .map_err(|e| match e {
                age::DecryptError::DecryptionFailed | age::DecryptError::NoMatchingKeys => invalid("wrong passphrase"),
                other => JobError::new(ErrorKind::UnsupportedFormat, other.to_string()),
            })?;
        let mut file = File::create(target)?;
        copy(ctx, &mut reader, &mut file).map_err(|e| match e.kind {
            ErrorKind::Cancelled => e,
            _ => JobError::new(ErrorKind::UnsupportedFormat, "this file is damaged or was changed"),
        })?;
        let _ = io::Write::flush(&mut file);
        Ok(())
    })
}
