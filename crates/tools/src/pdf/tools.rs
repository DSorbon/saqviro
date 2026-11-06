//! PDF tool executors. Work happens in the job's temp directory; results move
//! next to their originals only when the whole job succeeds.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, OutputFile, Params};
use serde_json::Value;

use super::{
    Qpdf, RENDER_NEEDS_PASSWORD, absolute, args, edit, failure,
    pages::{parse_pages, reordered, without},
    secret_file, stamp,
};
use crate::{
    output::{Outputs, file_stem, input_files, name_error, render_name, target_dir},
    process::{find_binary, search_dirs, supervise},
};

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn param_str<'a>(params: &'a Params, id: &str) -> Option<&'a str> {
    params.get(id).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

fn param_u32(params: &Params, id: &str) -> Option<u32> {
    params
        .get(id)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
}

/// One PDF of a batch.
struct Step<'a> {
    ctx: &'a JobContext,
    qpdf: &'static Qpdf,
    path: &'a Path,
    params: &'a Params,
    temp: PathBuf,
    /// The share of the job's progress this file covers.
    span: (f32, f32),
}

impl Step<'_> {
    /// Reports progress within this file, from 0.0 to 1.0.
    fn progress(&self, done: f32) {
        self.ctx.progress(self.span.0 + (self.span.1 - self.span.0) * done);
    }

    fn result(&self) -> PathBuf {
        self.temp.join("result.pdf")
    }

    fn pages_param(&self, id: &str, count: u32) -> Result<Vec<u32>, JobError> {
        let text = param_str(self.params, id).ok_or_else(|| invalid("enter page numbers, like 1, 3-5"))?;
        parse_pages(text, count)
    }

    /// Runs `edit` on an expanded copy of the file and writes it compactly.
    fn lopdf_edit<T>(&self, change: impl FnOnce(&mut lopdf::Document) -> Result<T, JobError>) -> Result<T, JobError> {
        let (expanded, edited) = (self.temp.join("expanded.pdf"), self.temp.join("edited.pdf"));
        self.qpdf.expand(self.ctx, self.path, &expanded)?;
        let mut doc = edit::load(&expanded)?;
        let value = change(&mut doc)?;
        edit::save(&mut doc, &edited)?;
        self.qpdf.finish(self.ctx, &edited, &self.result())?;
        Ok(value)
    }
}

/// A finished result, still in the job's temp directory.
struct Made {
    file: PathBuf,
    /// Tells the parts of a file apart in the result's name.
    part: String,
    ext: &'static str,
    note: Option<String>,
}

impl Made {
    fn pdf(file: PathBuf) -> Vec<Self> {
        vec![Self {
            file,
            part: String::new(),
            ext: "pdf",
            note: None,
        }]
    }
}

fn collect(
    ctx: &JobContext,
    outputs: &mut Outputs,
    results: &mut Vec<OutputFile>,
    template: &str,
    source: &Path,
    stem: &str,
    made: Vec<Made>,
) -> Result<(), JobError> {
    let source_size = (made.len() == 1)
        .then(|| fs::metadata(source).ok().map(|m| m.len()))
        .flatten();
    let dir = target_dir(ctx, source);
    for item in made {
        let name = render_name(template, &[("stem", stem), ("part", &item.part), ("ext", item.ext)]);
        let size = fs::metadata(&item.file)?.len();
        let path = outputs.adopt(&dir, &name, &item.file)?;
        results.push(OutputFile {
            path,
            size,
            source_size,
            note: item.note,
        });
    }
    Ok(())
}

/// Runs `make` for each input file.
fn batch(
    template: String,
    make: impl Fn(&Step) -> Result<Vec<Made>, JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let qpdf = Qpdf::get()?;
        let mut outputs = Outputs::new();
        let mut results = Vec::new();
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let share = 1.0 / files.len() as f32;
            let start = i as f32 * share;
            let temp = ctx.temp_dir()?.join(i.to_string());
            fs::create_dir_all(&temp)?;
            let step = Step {
                ctx,
                qpdf,
                path,
                params,
                temp,
                span: (start, start + share),
            };
            let made = make(&step).map_err(|e| name_error(path, e))?;
            collect(ctx, &mut outputs, &mut results, &template, path, &file_stem(path), made)?;
            ctx.progress(start + share);
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

/// Tools that make one PDF from all their inputs, named after the first.
fn combine(
    template: String,
    make: impl Fn(&JobContext, &[PathBuf], &Params, &Path) -> Result<(), JobError> + Send + Sync + 'static,
) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let result = ctx.temp_dir()?.join("result.pdf");
        make(ctx, &files, params, &result)?;
        ctx.check_cancelled()?;
        let mut outputs = Outputs::new();
        let mut results = Vec::new();
        let total = files.iter().filter_map(|f| fs::metadata(f).ok()).map(|m| m.len()).sum();
        let stem = file_stem(&files[0]);
        let made = Made::pdf(result);
        collect(ctx, &mut outputs, &mut results, &template, &files[0], &stem, made)?;
        results[0].source_size = Some(total);
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}

/// Merge PDF, in the order given.
pub fn merge(template: String) -> Arc<dyn Executor> {
    combine(template, |ctx, files, _, result| {
        if files.len() < 2 {
            return Err(invalid("add at least two PDFs to merge"));
        }
        let qpdf = Qpdf::get()?;
        let mut list: Vec<OsString> = vec!["--empty".into(), "--pages".into()];
        for file in files {
            // Counting first names the file that cannot be opened.
            qpdf.page_count(ctx, file).map_err(|e| name_error(file, e))?;
            list.extend([absolute(file)?.into(), "1-z".into()]);
        }
        list.extend(["--".into(), result.into()]);
        qpdf.run(ctx, &list).map(drop)
    })
}

/// Split PDF into files of `pages` pages each.
pub fn split(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let count = step.qpdf.page_count(step.ctx, step.path)?;
        let size = param_u32(step.params, "pages").unwrap_or(1).max(1);
        let parts = count.div_ceil(size);
        let width = parts.to_string().len();
        let mut made = Vec::with_capacity(parts as usize);
        for part in 0..parts {
            step.ctx.check_cancelled()?;
            let first = part * size + 1;
            let pages: Vec<u32> = (first..=(first + size - 1).min(count)).collect();
            let file = step.temp.join(format!("part-{part}.pdf"));
            step.qpdf.select(step.ctx, step.path, &pages, &file)?;
            made.push(Made {
                file,
                part: format!("{:0width$}", part + 1),
                ext: "pdf",
                note: None,
            });
            step.progress((part + 1) as f32 / parts as f32);
        }
        Ok(made)
    })
}

/// Delete PDF Pages.
pub fn delete_pages(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let count = step.qpdf.page_count(step.ctx, step.path)?;
        let keep = without(count, &step.pages_param("pages", count)?);
        if keep.is_empty() {
            return Err(invalid("a PDF needs at least one page, so not all of them can go"));
        }
        step.qpdf.select(step.ctx, step.path, &keep, &step.result())?;
        Ok(Made::pdf(step.result()))
    })
}

/// Extract PDF Pages, in the order listed.
pub fn extract_pages(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let count = step.qpdf.page_count(step.ctx, step.path)?;
        let pages = step.pages_param("pages", count)?;
        step.qpdf.select(step.ctx, step.path, &pages, &step.result())?;
        Ok(Made::pdf(step.result()))
    })
}

/// Reorder PDF Pages: the listed pages come first, the rest follow.
pub fn reorder_pages(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let count = step.qpdf.page_count(step.ctx, step.path)?;
        let reverse = step.params.get("reverse").and_then(Value::as_bool).unwrap_or(false);
        let pages = if reverse {
            (1..=count).rev().collect()
        } else {
            reordered(count, &step.pages_param("order", count)?)?
        };
        step.qpdf.select(step.ctx, step.path, &pages, &step.result())?;
        Ok(Made::pdf(step.result()))
    })
}

/// Rotate PDF Pages, all of them unless `pages` is set.
pub fn rotate_pages(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let count = step.qpdf.page_count(step.ctx, step.path)?;
        let angle = match param_str(step.params, "angle").unwrap_or("90") {
            angle @ ("90" | "180" | "270") => angle,
            other => return Err(invalid(format!("unknown angle `{other}`"))),
        };
        let pages = match param_str(step.params, "pages") {
            Some(_) => step.pages_param("pages", count)?,
            None => (1..=count).collect(),
        };
        let rotate = format!("--rotate=+{angle}:{}", super::pages::qpdf_range(&pages));
        let input = absolute(step.path)?;
        let result = step.result();
        let list = args(&[rotate.as_ref(), input.as_os_str(), result.as_os_str()]);
        step.qpdf.run(step.ctx, &list)?;
        Ok(Made::pdf(result))
    })
}

fn password(params: &Params) -> Result<&str, JobError> {
    let password = params
        .get("password")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| invalid("enter a password"))?;
    if password.contains(['\n', '\r']) {
        return Err(invalid("a password cannot contain line breaks"));
    }
    Ok(password)
}

/// Add PDF Password, with AES-256.
pub fn add_password(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let password = password(step.params)?;
        step.qpdf.page_count(step.ctx, step.path)?;
        let result = step.result();
        let lines = [
            "--encrypt".to_owned(),
            format!("--user-password={password}"),
            format!("--owner-password={password}"),
            "--bits=256".to_owned(),
            "--".to_owned(),
        ];
        let file = secret_file(&step.temp, "arguments", &(lines.join("\n") + "\n"))?;
        let mut at = OsString::from("@");
        at.push(&file);
        let input = absolute(step.path)?;
        let run = step.qpdf.run(
            step.ctx,
            &args(&[at.as_os_str(), input.as_os_str(), result.as_os_str()]),
        );
        fs::remove_file(&file)?;
        run?;
        Ok(Made::pdf(result))
    })
}

/// Remove PDF Password, for someone who knows it.
pub fn remove_password(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let password = password(step.params)?;
        let result = step.result();
        let file = secret_file(&step.temp, "password", password)?;
        let mut option = OsString::from("--password-file=");
        option.push(&file);
        let input = absolute(step.path)?;
        let run = step.qpdf.run(
            step.ctx,
            &args(&[
                option.as_os_str(),
                "--decrypt".as_ref(),
                input.as_os_str(),
                result.as_os_str(),
            ]),
        );
        fs::remove_file(&file)?;
        run.map_err(|e| match e.kind {
            ErrorKind::PermissionDenied => JobError::new(e.kind, "the password is not correct"),
            _ => e,
        })?;
        Ok(Made::pdf(result))
    })
}

/// Remove PDF Metadata.
pub fn remove_metadata(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        step.lopdf_edit(|doc| {
            edit::strip_metadata(doc);
            Ok(())
        })?;
        Ok(Made::pdf(step.result()))
    })
}

/// Compress PDF. See the decision record for the pipeline.
pub fn compress(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let dpi = param_u32(step.params, "dpi").unwrap_or(150).clamp(36, 600);
        let quality = param_u32(step.params, "quality").unwrap_or(70).clamp(1, 100) as u8;
        step.lopdf_edit(|doc| {
            edit::recompress_images(doc, dpi, quality, |done| {
                step.ctx.check_cancelled()?;
                step.progress(done * 0.9);
                Ok(())
            })
        })?;

        let result = step.result();
        let mut made = Made::pdf(result.clone());
        if fs::metadata(&result)?.len() >= fs::metadata(step.path)?.len() {
            fs::copy(step.path, &result)?;
            made[0].note = Some("This PDF is already well compressed, so it was left as it is.".to_owned());
        }
        Ok(made)
    })
}

/// PDF to Images, one image per page, drawn by `saqviro-pdf-render`.
pub fn to_images(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let format = param_str(step.params, "format").unwrap_or("png");
        let ext = if format == "jpg" { "jpg" } else { "png" };
        let dpi = param_u32(step.params, "dpi").unwrap_or(150).clamp(36, 600);
        let quality = param_u32(step.params, "quality").unwrap_or(85).clamp(1, 100);
        let mut args = vec![
            absolute(step.path)?.into_os_string(),
            step.temp.clone().into_os_string(),
        ];
        args.extend([dpi.to_string(), ext.to_owned(), quality.to_string()].map(OsString::from));
        let count = run_helper(step, &args)?;

        let width = count.to_string().len();
        (1..=count)
            .map(|page| {
                let file = step.temp.join(format!("{page}.{ext}"));
                if !file.is_file() {
                    return Err(JobError::new(
                        ErrorKind::ProcessFailed,
                        format!("page {page} was not drawn"),
                    ));
                }
                Ok(Made {
                    file,
                    part: format!("{page:0width$}"),
                    ext: if ext == "jpg" { "jpg" } else { "png" },
                    note: None,
                })
            })
            .collect()
    })
}

/// Images to PDF, in the order given.
pub fn from_images(template: String) -> Arc<dyn Executor> {
    combine(template, |ctx, files, params, result| {
        let name = param_str(params, "page_size").unwrap_or("fit");
        let size = edit::PageSize::from_name(name).ok_or_else(|| invalid(format!("unknown page size `{name}`")))?;
        let mut doc = edit::from_images(files, size, |done| {
            ctx.check_cancelled()?;
            ctx.progress(done * 0.9);
            Ok(())
        })?;
        let raw = result.with_file_name("built.pdf");
        edit::save(&mut doc, &raw)?;
        Qpdf::get()?.finish(ctx, &raw, result)
    })
}

/// Spacing from the edge of the page for page numbers, in points.
const NUMBER_MARGIN: f32 = 28.0;
const POINTS_PER_MM: f64 = 72.0 / 25.4;

pub fn page_numbers(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let position = param_str(step.params, "position").unwrap_or("bottom-center").to_owned();
        let style = param_str(step.params, "style").unwrap_or("number").to_owned();
        let start = param_u32(step.params, "start_at").unwrap_or(1);
        let size = param_u32(step.params, "size").unwrap_or(11) as f32;
        step.lopdf_edit(|doc| {
            stamp::stamp_pages(doc, |page, count, view| {
                let shown = start + page - 1;
                let text = match style.as_str() {
                    "page" => format!("Page {shown}"),
                    "of-total" => format!("{shown} / {}", start + count - 1),
                    _ => shown.to_string(),
                };
                let (width, height) = view.size();
                let used = stamp::text_width(&text, size);
                let x = match position.rsplit('-').next() {
                    Some("left") => NUMBER_MARGIN,
                    Some("right") => width - NUMBER_MARGIN - used,
                    _ => (width - used) / 2.0,
                };
                let y = if position.starts_with("top") {
                    height - NUMBER_MARGIN - size
                } else {
                    NUMBER_MARGIN
                };
                Ok(stamp::Stamp {
                    text: stamp::latin_bytes(&text)?,
                    size,
                    matrix: view.matrix(x, y, 0.0),
                    opacity: 1.0,
                    gray: 0.0,
                })
            })
        })?;
        Ok(Made::pdf(step.result()))
    })
}

/// Add Watermark to PDF. The text runs corner to corner, centered.
pub fn watermark(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let text = param_str(step.params, "text")
            .ok_or_else(|| invalid("enter the text to print"))?
            .to_owned();
        let bytes = stamp::latin_bytes(&text)?;
        let opacity = param_u32(step.params, "opacity").unwrap_or(20).clamp(1, 100) as f32 / 100.0;
        let wanted = param_u32(step.params, "size").unwrap_or(60) as f32;
        step.lopdf_edit(|doc| {
            stamp::stamp_pages(doc, |_, _, view| {
                let (width, height) = view.size();
                let angle = height.atan2(width);
                // A long text is made smaller rather than cut off by the page.
                let diagonal = width.hypot(height) * 0.9;
                let size = wanted.min(wanted * diagonal / stamp::text_width(&text, wanted).max(1.0));
                let used = stamp::text_width(&text, size);
                let (sin, cos) = angle.sin_cos();
                let x = width / 2.0 - (used * cos - size * 0.7 * sin) / 2.0;
                let y = height / 2.0 - (used * sin + size * 0.7 * cos) / 2.0;
                Ok(stamp::Stamp {
                    text: bytes.clone(),
                    size,
                    matrix: view.matrix(x, y, angle.to_degrees()),
                    opacity,
                    gray: 0.5,
                })
            })
        })?;
        Ok(Made::pdf(step.result()))
    })
}

/// Crop PDF. Margins are in millimetres. Nothing is deleted: the page only
/// shows less of what it holds.
pub fn crop(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let margin = |id: &str| {
            let mm = step.params.get(id).and_then(Value::as_f64).unwrap_or(0.0);
            (mm * POINTS_PER_MM) as f32
        };
        let (top, right, bottom, left) = (margin("top"), margin("right"), margin("bottom"), margin("left"));
        step.lopdf_edit(|doc| stamp::crop_pages(doc, top, right, bottom, left))?;
        Ok(Made::pdf(step.result()))
    })
}

pub fn edit_metadata(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let keys = [
            ("title", "Title"),
            ("author", "Author"),
            ("subject", "Subject"),
            ("keywords", "Keywords"),
        ];
        let entries: Vec<(&str, &str)> = keys
            .iter()
            .filter_map(|(id, key)| Some((*key, param_str(step.params, id)?)))
            .collect();
        if entries.is_empty() {
            return Err(invalid("fill in at least one property"));
        }
        step.lopdf_edit(|doc| stamp::set_info(doc, &entries))?;
        Ok(Made::pdf(step.result()))
    })
}

/// Repair PDF. qpdf rebuilds what it can while reading the file.
pub fn repair(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        step.qpdf.finish(step.ctx, &absolute(step.path)?, &step.result())?;
        // qpdf can write a file from what it found and still have no document.
        step.qpdf
            .page_count(step.ctx, &step.result())
            .map_err(|_| JobError::new(ErrorKind::UnsupportedFormat, "this PDF is too damaged to repair"))?;
        Ok(Made::pdf(step.result()))
    })
}

pub fn info() -> Arc<dyn Executor> {
    Arc::new(|ctx: &JobContext, input: JobInput, _: &Params| {
        let files = input_files(input)?;
        let qpdf = Qpdf::get()?;
        let mut sections = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let temp = ctx.temp_dir()?.join(format!("info-{i}.pdf"));
            qpdf.expand(ctx, path, &temp).map_err(|e| name_error(path, e))?;
            let doc = edit::load(&temp).map_err(|e| name_error(path, e))?;
            let pages = doc.get_pages();
            let mut lines = vec![
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                format!("Pages: {}", pages.len()),
                format!("PDF version: {}", doc.version),
                format!("File size: {} KB", fs::metadata(path)?.len().div_ceil(1024)),
            ];
            if let Some(view) = pages.values().next().and_then(|p| stamp::PageView::of(&doc, *p)) {
                let (w, h) = view.size();
                lines.push(format!(
                    "Page size: {:.0} × {:.0} mm",
                    f64::from(w) / POINTS_PER_MM,
                    f64::from(h) / POINTS_PER_MM
                ));
            }
            lines.extend(stamp::info_entries(&doc).into_iter().map(|(k, v)| format!("{k}: {v}")));
            sections.push(lines.join("\n"));
            ctx.progress((i + 1) as f32 / files.len() as f32);
        }
        Ok(JobOutput::Text(sections.join("\n\n")))
    })
}

/// Runs the PDFium helper with `args`, reporting its `page n count` lines.
fn run_helper(step: &Step<'_>, args: &[OsString]) -> Result<usize, JobError> {
    let helper = find_binary(search_dirs(), "saqviro-pdf-render").ok_or_else(|| {
        JobError::new(
            ErrorKind::EngineMissing,
            "the PDF renderer is missing from this installation",
        )
    })?;
    let total = Mutex::new(0usize);
    let mut command = Command::new(helper);
    command.args(args);
    let (status, stderr) = supervise(step.ctx, &mut command, |line| {
        let mut parts = line.split_whitespace().skip(1).filter_map(|n| n.parse::<usize>().ok());
        if let (Some(done), Some(count)) = (parts.next(), parts.next()) {
            *total.lock().unwrap() = count;
            step.progress(done as f32 / count.max(1) as f32);
        }
    })?;
    if status.code() == Some(RENDER_NEEDS_PASSWORD) {
        return Err(failure("invalid password"));
    }
    if !status.success() {
        let reason = stderr.trim();
        let reason = if reason.is_empty() {
            "the renderer stopped on this file"
        } else {
            reason
        };
        return Err(JobError::new(ErrorKind::UnsupportedFormat, reason));
    }
    Ok(total.into_inner().unwrap())
}

pub fn to_text(template: String) -> Arc<dyn Executor> {
    batch(template, |step| {
        let file = step.temp.join("text.txt");
        let args = [
            "--text".into(),
            absolute(step.path)?.into_os_string(),
            file.clone().into_os_string(),
        ];
        run_helper(step, &args)?;
        let note = (fs::metadata(&file)?.len() < 8)
            .then(|| "This PDF has almost no text. It may be scanned pictures of pages.".to_owned());
        Ok(vec![Made {
            file,
            part: String::new(),
            ext: "txt",
            note,
        }])
    })
}

/// Extract Images from PDF. Each PDF gets a folder of its own.
pub fn extract_images(template: String) -> Arc<dyn Executor> {
    Arc::new(move |ctx: &JobContext, input: JobInput, params: &Params| {
        let files = input_files(input)?;
        let qpdf = Qpdf::get()?;
        let mut outputs = Outputs::new();
        let mut results = Vec::with_capacity(files.len());
        for (i, path) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let share = 1.0 / files.len() as f32;
            let temp = ctx.temp_dir()?.join(i.to_string());
            fs::create_dir_all(&temp)?;
            let step = Step {
                ctx,
                qpdf,
                path,
                params,
                temp: temp.clone(),
                span: (i as f32 * share, (i + 1) as f32 * share),
            };
            let args = [
                "--images".into(),
                absolute(path)?.into_os_string(),
                temp.clone().into_os_string(),
            ];
            run_helper(&step, &args).map_err(|e| name_error(path, e))?;

            let found: Vec<_> = fs::read_dir(&temp)?.collect::<Result<_, _>>()?;
            if found.is_empty() {
                return Err(name_error(path, invalid("this PDF has no images")));
            }
            let name = render_name(&template, &[("stem", &file_stem(path))]);
            let folder = outputs.folder(&target_dir(ctx, path), &name)?;
            let mut size = 0;
            for image in found {
                size += image.metadata()?.len();
                let target = folder.join(image.file_name());
                if fs::rename(image.path(), &target).is_err() {
                    fs::copy(image.path(), &target)?;
                }
            }
            results.push(OutputFile {
                path: folder,
                size,
                source_size: None,
                note: None,
            });
        }
        ctx.check_cancelled()?;
        outputs.finish();
        Ok(JobOutput::Files(results))
    })
}
