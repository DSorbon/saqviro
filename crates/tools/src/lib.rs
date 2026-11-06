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
    "audio.detect-silence",
    "audio.merge",
    "audio.remove-silence",
    "audio.sample-rate",
    "audio.spectrogram",
    "audio.to-mono",
    "audio.volume",
    "audio.waveform",
    "video.crop",
    "video.faststart",
    "video.fps",
    "video.frames",
    "video.from-gif",
    "video.merge",
    "video.replace-audio",
    "video.speed",
    "video.thumbnail",
    "audio.info",
    "audio.normalize",
    "audio.trim",
    "developer.base64",
    "developer.cron",
    "developer.csv-to-json",
    "developer.hmac",
    "developer.html-entities",
    "developer.http-status",
    "developer.json-diff",
    "developer.json-to-csv",
    "developer.json-to-ts",
    "developer.json-to-yaml",
    "developer.mime",
    "developer.number-base",
    "developer.password",
    "developer.regex-escape",
    "developer.ulid",
    "developer.url.encode",
    "developer.url.parse",
    "developer.xml.format",
    "developer.yaml-to-json",
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
    "image.border",
    "image.compare",
    "image.contact-sheet",
    "image.favicon",
    "image.flip",
    "image.frames-to-gif",
    "image.gif-frames",
    "image.grayscale",
    "image.info",
    "image.remove-alpha",
    "image.round-corners",
    "image.svg-to-png",
    "image.to-ico",
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
    "pdf.crop",
    "pdf.extract-images",
    "pdf.info",
    "pdf.metadata.edit",
    "pdf.page-numbers",
    "pdf.repair",
    "pdf.to-text",
    "pdf.watermark",
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
        "developer.cron" => text_tool(developer::cron::run),
        "developer.csv-to-json" => text_tool(developer::convert::csv_to_json),
        "developer.hmac" => text_tool(developer::generate::hmac),
        "developer.html-entities" => text_tool(developer::web::html_entities),
        "developer.http-status" => text_tool(developer::web::http_status),
        "developer.json-diff" => text_tool(developer::convert::json_diff),
        "developer.json-to-csv" => text_tool(developer::convert::json_to_csv),
        "developer.json-to-ts" => text_tool(developer::convert::json_to_ts),
        "developer.json-to-yaml" => text_tool(developer::convert::json_to_yaml),
        "developer.mime" => text_tool(developer::web::mime),
        "developer.number-base" => text_tool(developer::generate::number_base),
        "developer.password" => text_tool(developer::generate::passwords),
        "developer.regex-escape" => text_tool(developer::generate::regex_escape),
        "developer.ulid" => text_tool(developer::generate::ulids),
        "developer.url.encode" => text_tool(developer::web::url_encode),
        "developer.url.parse" => text_tool(developer::web::url_parse),
        "developer.xml.format" => text_tool(developer::xml::format),
        "developer.yaml-to-json" => text_tool(developer::convert::yaml_to_json),
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
        "image.border" => img::border(name),
        "image.compare" => img::compare(name),
        "image.contact-sheet" => img::contact_sheet(name),
        "image.favicon" => img::favicon(name),
        "image.flip" => img::flip(name),
        "image.frames-to-gif" => img::frames_to_gif(name),
        "image.gif-frames" => img::gif_frames(name),
        "image.grayscale" => img::grayscale(name),
        "image.info" => img::info(),
        "image.remove-alpha" => img::remove_alpha(name),
        "image.round-corners" => img::round_corners(name),
        "image.svg-to-png" => img::svg_to_png(name),
        "image.to-ico" => img::to_ico(name),
        "image.resize" => img::resize(name),
        "image.compress" => img::compress(name),
        "image.rotate" => img::rotate(name),
        "image.crop" => img::crop(name),
        "image.metadata.remove" => img::remove_metadata(name),
        "pdf.crop" => pdf::crop(name),
        "pdf.extract-images" => pdf::extract_images(name),
        "pdf.info" => pdf::info(),
        "pdf.metadata.edit" => pdf::edit_metadata(name),
        "pdf.page-numbers" => pdf::page_numbers(name),
        "pdf.repair" => pdf::repair(name),
        "pdf.to-text" => pdf::to_text(name),
        "pdf.watermark" => pdf::watermark(name),
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
        "audio.detect-silence" => media::detect_silence(),
        "audio.merge" => media::merge_audio(name),
        "audio.remove-silence" => media::remove_silence(name),
        "audio.sample-rate" => media::sample_rate(name),
        "audio.spectrogram" => media::spectrogram(name),
        "audio.to-mono" => media::to_mono(name),
        "audio.volume" => media::volume(name),
        "audio.waveform" => media::waveform(name),
        "video.crop" => media::crop(name),
        "video.faststart" => media::faststart(name),
        "video.fps" => media::fps(name),
        "video.frames" => media::frames(name),
        "video.from-gif" => media::from_gif(name),
        "video.merge" => media::merge_video(name),
        "video.replace-audio" => media::replace_audio(name),
        "video.speed" => media::speed(name),
        "video.thumbnail" => media::thumbnail(name),
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
