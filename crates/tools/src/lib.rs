//! Tool implementations. Every manifest in `tools/**/tool.yaml` with
//! `engine: builtin` maps to an executor here; the catalog test keeps the two
//! in sync.

use std::sync::Arc;

use saqviro_core::manifest::ToolManifest;
use saqviro_jobs::Executor;

pub mod developer;
pub mod images;
pub mod output;

/// Ids of every tool implemented in this crate.
pub const IMPLEMENTED: &[&str] = &[
    "developer.json.format",
    "image.compress",
    "image.convert",
    #[cfg(feature = "heic")]
    "image.heic-to-jpg",
    "image.metadata.remove",
    "image.resize",
    "image.rotate",
    "image.to-avif",
    "image.to-webp",
    "image.webp-to-jpg",
];

/// Tools that exist only when a cargo feature is enabled: (tool id, feature).
/// Without the feature the app hides the tool.
pub const NEEDS_FEATURE: &[(&str, &str)] = &[("image.heic-to-jpg", "heic")];

/// The executor for a tool, or `None` if it is not implemented. Executors
/// take their output file name template from the manifest.
pub fn executor(tool: &ToolManifest) -> Option<Arc<dyn Executor>> {
    use images::{Format, tools as img};

    let name = tool
        .outputs
        .first()
        .and_then(|o| o.filename.clone())
        .unwrap_or_else(|| "{stem}.{ext}".into());
    let exec: Arc<dyn Executor> = match tool.id.as_str() {
        "developer.json.format" => Arc::new(developer::json_format::run),
        "image.convert" | "image.webp-to-jpg" => img::convert(None, name),
        #[cfg(feature = "heic")]
        "image.heic-to-jpg" => img::convert(None, name),
        "image.to-webp" => img::convert(Some(Format::Webp), name),
        "image.to-avif" => img::convert(Some(Format::Avif), name),
        "image.resize" => img::resize(name),
        "image.compress" => img::compress(name),
        "image.rotate" => img::rotate(name),
        "image.metadata.remove" => img::remove_metadata(name),
        _ => return None,
    };
    Some(exec)
}
