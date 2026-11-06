//! Tool implementations. Every manifest in `tools/**/tool.yaml` with
//! `engine: builtin` maps to an executor here; the catalog test keeps the two
//! in sync.

use std::sync::Arc;

use saqviro_core::manifest::ToolManifest;
use saqviro_jobs::{ErrorKind, Executor, JobContext, JobError, JobInput, JobOutput, Params};

pub mod developer;
pub mod files;
pub mod images;
pub mod media;
pub mod output;
pub(crate) mod params;
pub mod pdf;
pub mod process;
pub mod qr;
pub mod text;

/// Ids of every tool implemented in this crate.
pub const IMPLEMENTED: &[&str] = &[
    "audio.convert",
    "audio.info",
    "audio.normalize",
    "audio.trim",
    "developer.base64",
    "developer.hash",
    "developer.json.format",
    "developer.jwt.decode",
    "developer.regex",
    "developer.timestamp",
    "developer.uuid",
    "files.checksum.verify",
    "files.rename",
    "files.sha256",
    "files.zip.create",
    "files.zip.extract",
    "image.compress",
    "image.convert",
    "image.crop",
    #[cfg(feature = "heic")]
    "image.heic-to-jpg",
    "image.metadata.remove",
    "image.resize",
    "image.rotate",
    "image.to-avif",
    "image.to-webp",
    "image.webp-to-jpg",
    "pdf.compress",
    "pdf.delete-pages",
    "pdf.extract-pages",
    "pdf.from-images",
    "pdf.merge",
    "pdf.metadata.remove",
    "pdf.password.add",
    "pdf.password.remove",
    "pdf.reorder-pages",
    "pdf.rotate-pages",
    "pdf.split",
    "pdf.to-images",
    "qr.generate",
    "qr.scan",
    "text.case",
    "text.diff",
    "text.invisible",
    "text.lines.dedupe",
    "text.lines.remove-empty",
    "text.lines.reverse",
    "text.lines.sort",
    "text.lines.trim",
    "text.lorem",
    "text.replace",
    "text.slug",
    "text.unicode",
    "text.whitespace",
    "text.word-count",
    "video.compress",
    "video.convert",
    "video.extract-audio",
    "video.info",
    "video.mute",
    "video.resize",
    "video.rotate",
    "video.to-gif",
    "video.trim",
];

/// Tools that exist only when a cargo feature is enabled: (tool id, feature).
/// Without the feature the app hides the tool.
pub const NEEDS_FEATURE: &[(&str, &str)] = &[("image.heic-to-jpg", "heic")];

/// An executor for a tool that turns text into text.
fn text_tool(run: fn(&str, &Params) -> Result<String, String>) -> Arc<dyn Executor> {
    Arc::new(move |_: &JobContext, input: JobInput, params: &Params| {
        let JobInput::Text(text) = input else {
            return Err(JobError::new(
                ErrorKind::InvalidInput,
                "this tool takes text, not files",
            ));
        };
        run(&text, params)
            .map(JobOutput::Text)
            .map_err(|e| JobError::new(ErrorKind::InvalidInput, e))
    })
}

/// The executor for a tool, or `None` if it is not implemented. Executors
/// take their output file name template from the manifest.
pub fn executor(tool: &ToolManifest) -> Option<Arc<dyn Executor>> {
    use images::{Format, tools as img};
    use media::tools as media;
    use pdf::tools as pdf;

    let name = tool
        .outputs
        .first()
        .and_then(|o| o.filename.clone())
        .unwrap_or_else(|| "{stem}.{ext}".into());
    let exec: Arc<dyn Executor> = match tool.id.as_str() {
        "developer.base64" => text_tool(developer::base64::run),
        "developer.hash" => text_tool(developer::hash::run),
        "developer.json.format" => Arc::new(developer::json_format::run),
        "developer.jwt.decode" => text_tool(developer::jwt::run),
        "developer.regex" => text_tool(developer::regex_test::run),
        "developer.timestamp" => text_tool(developer::timestamp::run),
        "developer.uuid" => text_tool(developer::uuid::run),
        "files.checksum.verify" => files::checksum::verify(),
        "files.rename" => files::rename::run(),
        "files.sha256" => files::checksum::sha256(),
        "files.zip.create" => files::archive::create(name),
        "files.zip.extract" => files::archive::extract(name),
        "qr.generate" => Arc::new(qr::generate),
        "qr.scan" => qr::scan(),
        "text.case" => text_tool(text::case::run),
        "text.diff" => text_tool(text::diff::run),
        "text.invisible" => text_tool(text::clean::invisible),
        "text.lines.dedupe" => text_tool(text::lines::dedupe),
        "text.lines.remove-empty" => text_tool(text::lines::remove_empty),
        "text.lines.reverse" => text_tool(text::lines::reverse),
        "text.lines.sort" => text_tool(text::lines::sort),
        "text.lines.trim" => text_tool(text::lines::trim),
        "text.lorem" => text_tool(text::lorem::run),
        "text.replace" => text_tool(text::replace::run),
        "text.slug" => text_tool(text::slug::run),
        "text.unicode" => text_tool(text::unicode::run),
        "text.whitespace" => text_tool(text::clean::whitespace),
        "text.word-count" => text_tool(text::words::run),
        "image.convert" | "image.webp-to-jpg" => img::convert(None, name),
        #[cfg(feature = "heic")]
        "image.heic-to-jpg" => img::convert(None, name),
        "image.to-webp" => img::convert(Some(Format::Webp), name),
        "image.to-avif" => img::convert(Some(Format::Avif), name),
        "image.resize" => img::resize(name),
        "image.compress" => img::compress(name),
        "image.rotate" => img::rotate(name),
        "image.crop" => img::crop(name),
        "image.metadata.remove" => img::remove_metadata(name),
        "pdf.compress" => pdf::compress(name),
        "pdf.delete-pages" => pdf::delete_pages(name),
        "pdf.extract-pages" => pdf::extract_pages(name),
        "pdf.from-images" => pdf::from_images(name),
        "pdf.merge" => pdf::merge(name),
        "pdf.metadata.remove" => pdf::remove_metadata(name),
        "pdf.password.add" => pdf::add_password(name),
        "pdf.password.remove" => pdf::remove_password(name),
        "pdf.reorder-pages" => pdf::reorder_pages(name),
        "pdf.rotate-pages" => pdf::rotate_pages(name),
        "pdf.split" => pdf::split(name),
        "pdf.to-images" => pdf::to_images(name),
        "video.compress" => media::compress(name),
        "video.convert" => media::convert(name),
        "video.extract-audio" | "audio.convert" => media::audio_to(name),
        "video.info" | "audio.info" => media::info(),
        "video.mute" => media::mute(name),
        "video.resize" => media::resize(name),
        "video.rotate" => media::rotate(name),
        "video.to-gif" => media::to_gif(name),
        "video.trim" => media::trim_video(name),
        "audio.trim" => media::trim_audio(name),
        "audio.normalize" => media::normalize(name),
        _ => return None,
    };
    Some(exec)
}
