//! Tool manifests and the tool registry.
//!
//! Every tool is described by a `tool.yaml` manifest. The UI, search and
//! Smart Drop are built from manifests, so validation here is strict: a
//! manifest that loads is safe to render.

pub mod manifest;
pub mod registry;
