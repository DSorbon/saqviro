//! Tool implementations. Each tool id in `tools/**/tool.yaml` with
//! `engine: builtin` maps to an executor here.

use std::sync::Arc;

use saqviro_jobs::Executor;

pub mod developer;

/// Ids of every tool implemented in this crate.
pub const IMPLEMENTED: &[&str] = &["developer.json.format"];

pub fn executor(tool_id: &str) -> Option<Arc<dyn Executor>> {
    match tool_id {
        "developer.json.format" => Some(Arc::new(developer::json_format::run)),
        _ => None,
    }
}
