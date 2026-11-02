# Architecture

## Overview

```text
Svelte 5 UI ──invoke──▶ Tauri commands ──▶ saqviro-core   (manifests, registry, parameter validation)
(apps/desktop/src)      (src-tauri)    ──▶ saqviro-jobs   (queue, progress, cancel, temp dirs)
     ▲                                 ──▶ saqviro-tools  (tool implementations)
     └──────── `job` events ◀──────────────┘
```

The UI is a thin layer. It never reads or writes user files and never validates parameters; it renders what the Rust side returns.

## Crates

| Crate | Responsibility |
|---|---|
| `crates/core` | `tool.yaml` types, validation, registry, Smart Drop Lite (`for_files`), run-time parameter validation (`resolve_params`) |
| `crates/jobs` | `JobManager`: worker pool, job states, progress, cancellation, per-job temp directories |
| `crates/tools` | One executor per implemented tool (`executor(&manifest)`, `IMPLEMENTED`), the image engine, output files |
| `apps/desktop/src-tauri` | Tauri commands and events; embeds manifests at build time |

## Tool manifests

Every tool is described by `tools/<category>/<name>/tool.yaml`.

- `crates/core/src/manifest.rs` defines the types and the rules serde cannot express (id matches category, presets reference real parameters, file inputs declare MIME types and extensions, 1–4 presets).
- `crates/core/src/registry.rs` loads manifests, rejects duplicate ids and reports every broken file at once.
- `apps/desktop/src-tauri/build.rs` embeds all manifests into the binary. The app never reads manifests from disk.
- `packages/tool-schema/tool.schema.json` is generated from the Rust types for editor validation. A test fails if it is out of date.

A manifest alone is not a tool: the app lists only tools that have an executor in `crates/tools`. A test checks that every `engine: builtin` manifest has an executor and every executor has a manifest.

## Image engine (`crates/tools/src/images`)

Pure Rust except HEIC — see [decision 0002](decisions/0002-image-engine.md).

- Formats are detected from content, never from the extension.
- Decoding applies EXIF orientation, keeps the ICC profile, and rejects images over 32 768 px per side before allocating pixels. Truncated JPEGs are rejected (the decoder would otherwise fill the missing part with grey).
- Encoding: JPEG with mozjpeg (alpha blended onto white, ICC written back), PNG, lossy WebP, AVIF. WebP and AVIF output do not carry the ICC profile yet.
- Resizing never enlarges.
- `metadata::strip` is lossless for JPEG, PNG and WebP: pixel data is copied byte for byte. It keeps the color profile and orientation and removes EXIF, XMP, IPTC, comments, unknown chunks and data after the image (for example Motion Photo video). The tool checks its own result by stripping it again.
- HEIC uses libheif, linked dynamically, through the `heic` feature of `saqviro-tools`. The app enables it on macOS and Linux; Windows builds leave it out (and hide the tool) until libheif is packaged there. AVIF cannot be read yet.

## Result files (`crates/tools/src/output.rs`)

- Written next to the original. A taken name gets ` (2)`, ` (3)`…; nothing is ever overwritten.
- In a batch, the first failing file fails the job and the results already written are removed. Cancelling removes them too.
- Each result carries its size and its source's size, so the UI can show "1.2 MB → 340 KB (−72%)".

## Jobs

Every run is a job: `queued → running → completed | failed | cancelled`.

- Jobs run on `min(4, cores / 2)` worker threads.
- Executors receive a `JobContext`: `progress(0.0..=1.0)`, `check_cancelled()?` between units of work, and `temp_dir()` — a private directory with a random name, created on first use.
- The temp directory is removed when the job ends, however it ends (success, failure, cancellation, panic). Leftovers from a crash are removed when the app starts. Temp root: the app cache directory, `jobs/`.
- A panicking executor fails its job with `unknown`; the worker survives.
- Errors use one set of kinds: `invalid_input`, `unsupported_format`, `engine_missing`, `permission_denied`, `process_failed`, `output_exists`, `disk_full`, `cancelled`, `timeout`, `unknown`.

## Commands and events

| Command | Arguments | Returns |
|---|---|---|
| `list_tools` | — | Manifests of available tools, sorted by id |
| `tools_for_files` | `paths` | Ids of available tools that accept every file (by extension) |
| `run_tool` | `toolId`, `input` (`{kind: "text" \| "files", value}`), `params` | Job id, or a `JobError` if the tool is unknown, unavailable or the parameters are invalid |
| `cancel_job` | `id` | `true` if a queued or running job was cancelled |
| `pick_files` | `toolId` or `null` | Paths chosen in the native picker, filtered by the tool's extensions |
| `open_file`, `reveal_file` | `path` | Opens the file or shows it in its folder. Only files a job created are accepted |

Every change of every job is emitted as a `job` event carrying a full `JobSnapshot`. `ipc_accepts_the_frontend_payloads` in `src-tauri/src/lib.rs` sends commands exactly as `src/lib/api.ts` does, so the two cannot drift apart silently.

## Frontend

- `src/lib/search.ts` — ranks tools by name, keywords, input extensions, category and description; ties broken by usage.
- `src/lib/sidebar.ts` — groups categories into sidebar sections; empty sections are hidden.
- `src/lib/params.ts` — initial parameter values: defaults overridden by the selected preset.
- `src/lib/job.ts` — job types, status lines and error explanations.
- `src/components/ToolView.svelte` — a tool screen generated from the manifest: input, presets, main parameters, "Advanced", Run (⌘/Ctrl+Enter), progress with Cancel, result.

Files dropped on the window go to the open tool if it accepts them; otherwise Home shows the tools that do.

## Not implemented yet

Video, audio and PDF tools; Crop (needs an interactive selection); choosing an output folder; settings, favorites and recents; Tailwind and `packages/ui`; a libheif build for Windows.
