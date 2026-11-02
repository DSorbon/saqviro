# Architecture

## Overview

```text
Svelte 5 UI  ──invoke / events──▶  Tauri commands  ──▶  saqviro-core
(apps/desktop/src)                 (apps/desktop/src-tauri)   (crates/core)
```

The UI is a thin layer. It never reads or writes user files; it renders what the Rust side returns.

## Tool manifests

Every tool is described by `tools/<category>/<name>/tool.yaml`.

- `crates/core/src/manifest.rs` defines the manifest types and validation rules that serde cannot express (id matches category, presets reference real parameters, file inputs declare MIME types and extensions, 1–4 presets).
- `crates/core/src/registry.rs` loads manifests, rejects duplicate ids and reports every broken file at once.
- `apps/desktop/src-tauri/build.rs` embeds all manifests into the binary at compile time. The app never reads manifests from disk.
- `packages/tool-schema/tool.schema.json` is generated from the Rust types for editor validation. A test fails if it is out of date.

## Commands

| Command | Returns |
|---|---|
| `list_tools` | All tool manifests, sorted by id |
| `tools_for_files(paths)` | Ids of tools that accept every dropped file (Smart Drop Lite, by extension) |

## Frontend

- `src/lib/search.ts` — ranks tools by name, keywords, input extensions, category and description; ties broken by usage.
- `src/lib/sidebar.ts` — groups categories into sidebar sections; empty sections are hidden.
- `src/lib/params.ts` — initial parameter values: defaults overridden by the selected preset.
- `src/components/ToolView.svelte` — renders a tool screen from its manifest: presets as a segmented control, main parameters, and the rest under "Advanced".

## Not implemented yet

Job system (queue, progress, cancel), tool engines, results, settings, favorites and recents.
