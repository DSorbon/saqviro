//! Draws PDF pages with PDFium, in a process of its own so that a crash on a
//! hostile file fails one job and nothing else.
//!
//! Usage: saqviro-pdf-render <pdf> <output dir> <dpi> <png|jpg> <quality>
//!        saqviro-pdf-render --first <pdf> <output dir>
//!        saqviro-pdf-render --text <pdf> <output file>
//!        saqviro-pdf-render --images <pdf> <output dir>
//! Writes `<page>.<ext>` and prints `page <done> <count>` after each page.

use std::{
    env,
    fmt::Display,
    fs,
    io::{Write, stdout},
    path::{Path, PathBuf},
    process::ExitCode,
};

use pdfium_render::prelude::*;
use saqviro_tools::{
    images::{Format, encode},
    pdf::RENDER_NEEDS_PASSWORD,
};

/// Longest side of a rendered page, in pixels.
const MAX_SIDE: i32 = 16_384;

fn library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("../Frameworks"));
        dirs.push(exe_dir.join("binaries"));
        dirs.push(exe_dir.join("../lib/Saqviro/binaries"));
        dirs.push(exe_dir);
    }
    if cfg!(debug_assertions) {
        dirs.push(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src-tauri/binaries").into());
    }
    dirs
}

fn bind() -> Result<Pdfium, String> {
    let library = library_dirs()
        .into_iter()
        .map(|dir| Pdfium::pdfium_platform_library_name_at_path(&dir))
        .find(|path| path.is_file())
        .ok_or("PDFium is missing from this installation")?;
    Ok(Pdfium::new(Pdfium::bind_to_library(library).map_err(text)?))
}

fn text(e: impl Display) -> String {
    e.to_string()
}

fn render(
    path: &str,
    out: &Path,
    dpi: f32,
    ext: &str,
    quality: u8,
    limit: usize,
    pdfium: &Pdfium,
) -> Result<(), RenderError> {
    let document = open(path, pdfium)?;
    let format = if ext == "jpg" { Format::Jpeg } else { Format::Png };
    let pages = document.pages();
    let count = pages.len();
    for (i, page) in pages.iter().enumerate().take(limit) {
        let scale = dpi / 72.0;
        let width = ((page.width().value * scale) as i32).clamp(1, MAX_SIDE);
        let height = ((page.height().value * scale) as i32).clamp(1, MAX_SIDE);
        let config = PdfRenderConfig::new()
            .set_target_width(width)
            .set_maximum_height(height);
        let failed = |e: String| RenderError::Failed(format!("page {} could not be drawn: {e}", i + 1));
        let bitmap = page.render_with_config(&config).map_err(|e| failed(text(e)))?;
        let image = bitmap.as_image().map_err(|e| failed(text(e)))?;
        let bytes = encode(&image, None, format, quality).map_err(|e| failed(e.message))?;
        fs::write(out.join(format!("{}.{ext}", i + 1)), bytes).map_err(|e| failed(text(e)))?;
        report(i + 1, count);
    }
    Ok(())
}

fn open<'a>(path: &str, pdfium: &'a Pdfium) -> Result<PdfDocument<'a>, RenderError> {
    pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => RenderError::NeedsPassword,
        other => RenderError::Failed(format!("this PDF could not be opened: {other}")),
    })
}

fn report(done: usize, count: impl Display) {
    println!("page {done} {count}");
    let _ = stdout().flush();
}

/// The text of every page, with a blank line between pages.
fn extract_text(path: &str, out: &Path, pdfium: &Pdfium) -> Result<(), RenderError> {
    let document = open(path, pdfium)?;
    let pages = document.pages();
    let count = pages.len();
    let mut all = String::new();
    for (i, page) in pages.iter().enumerate() {
        let failed = |e: String| RenderError::Failed(format!("page {} could not be read: {e}", i + 1));
        let page_text = page.text().map_err(|e| failed(text(e)))?.all();
        all.push_str(page_text.trim_end());
        all.push_str("\n\n");
        report(i + 1, count);
    }
    fs::write(out, all.trim_end().as_bytes()).map_err(|e| RenderError::Failed(text(e)))
}

/// Every picture on every page, as `page-<n>-<m>.png`.
fn extract_images(path: &str, out: &Path, pdfium: &Pdfium) -> Result<(), RenderError> {
    let document = open(path, pdfium)?;
    let pages = document.pages();
    let count = pages.len();
    for (i, page) in pages.iter().enumerate() {
        let failed = |e: String| RenderError::Failed(format!("page {} could not be read: {e}", i + 1));
        let mut found = 0;
        for object in page.objects().iter() {
            let Some(picture) = object.as_image_object() else {
                continue;
            };
            let Ok(image) = picture.get_raw_image() else { continue };
            found += 1;
            let bytes = encode(&image, None, Format::Png, 100).map_err(|e| failed(e.message))?;
            let name = format!("page-{}-{found}.png", i + 1);
            fs::write(out.join(name), bytes).map_err(|e| failed(text(e)))?;
        }
        report(i + 1, count);
    }
    Ok(())
}

enum RenderError {
    NeedsPassword,
    Failed(String),
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let run = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--first", path, out] => bind()
            .map_err(RenderError::Failed)
            .and_then(|pdfium| render(path, Path::new(out), 110.0, "jpg", 80, 1, &pdfium)),
        ["--text", path, out] => bind()
            .map_err(RenderError::Failed)
            .and_then(|pdfium| extract_text(path, Path::new(out), &pdfium)),
        ["--images", path, out] => bind()
            .map_err(RenderError::Failed)
            .and_then(|pdfium| extract_images(path, Path::new(out), &pdfium)),
        [path, out, dpi, ext, quality] => {
            let (Ok(dpi), Ok(quality)) = (dpi.parse(), quality.parse()) else {
                eprintln!("dpi and quality must be numbers");
                return ExitCode::from(2);
            };
            bind()
                .map_err(RenderError::Failed)
                .and_then(|pdfium| render(path, Path::new(out), dpi, ext, quality, usize::MAX, &pdfium))
        }
        _ => {
            eprintln!("usage: saqviro-pdf-render <pdf> <output dir> <dpi> <png|jpg> <quality>");
            return ExitCode::from(2);
        }
    };
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(RenderError::NeedsPassword) => ExitCode::from(RENDER_NEEDS_PASSWORD as u8),
        Err(RenderError::Failed(message)) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
