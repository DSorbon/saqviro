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
| `crates/workflow` | Validates and runs tools one after another as one job |
| `apps/desktop/src-tauri` | Tauri commands and events; embeds manifests at build time |

## Workflows (`crates/workflow`)

Tools run one after another as a single job. See [workflows.md](workflows.md).

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

## Media engine (`crates/tools/src/media`)

See [decision 0003](decisions/0003-media-engine.md).

- `Engine` runs FFmpeg and ffprobe as separate processes, from argument arrays. Release builds use only the binaries next to the app; debug builds also search `PATH`.
- `-progress pipe:1` drives job progress. Cancelling kills the process.
- Every input is opened as `file:<path>` with a protocol and demuxer whitelist, so a file cannot pull in other files or the network.
- Results are written to the job's temp directory and moved next to the original on success.
- `plan_video` picks the encoder: system H.264 where it works, otherwise VP9. MP4 and MOV then become WebM, and the result's `note` tells the user.
- Streams are copied when nothing needs to change: Mute, fast Trim, and Convert or Extract Audio when the codec already fits.
- Trim Audio copies lossy audio and encodes lossless audio with its own codec, which is exact and keeps FLAC headers correct.
- Normalize Audio runs loudnorm twice: once to measure, once to apply one constant gain.

## PDF engine (`crates/tools/src/pdf`)

See [decision 0004](decisions/0004-pdf-engine.md).

- `Qpdf` runs qpdf as a process for merge, split, page edits, rotation and passwords. Paths are made absolute so a file name can never be read as an option.
- `edit` uses lopdf for what qpdf cannot do: removing metadata, recompressing images, building a PDF from images.
- `saqviro-pdf-render` is a helper binary that draws pages with PDFium, so a crash in PDFium ends the helper rather than the app.
- Passwords reach qpdf through a file in the job's temp directory, never through a command line. A test records qpdf's arguments to keep it that way.
- Manifest parameters with `secret: true` render as password fields.

## File tools (`crates/tools/src/files`)

- **Extract ZIP** treats archives as hostile. Names that leave the target folder fail the job, links are skipped, an entry can never replace another, and limits on real bytes written stop zip bombs. Results go into a new folder that is removed if the job fails.
- **Create ZIP** takes files and folders and never follows links.
- **Batch Rename** is the only tool that changes the user's files in place, so it is marked `destructive`. It previews by default, checks every name before anything moves, renames through temporary names so swaps work, and puts every name back if a step fails.
- **Checksums** read files in pieces and can be cancelled between them. A mismatch completes the job: it is an answer, not a failure.

## Inputs and outputs

- `inputs[].type: none` is a tool without input; `optional: true` lets text stay empty; `extensions: ["*"]` takes any file or folder. Such tools appear for every drop, and they add no filter to the file picker.
- `JobOutput::Image` holds a picture made from text, such as a QR code. The app shows it, and `save_image` writes it where the user chooses. The bytes come from the job, so the UI cannot write anything else.

## Shipping engines

See [decision 0005](decisions/0005-distribution.md) and [releasing](releasing.md). `scripts/prepare-bundle.sh` puts every engine and the license notices into `apps/desktop/src-tauri/binaries`, `scripts/build-release.sh` builds installers from them, and `scripts/check-bundle.sh` runs the result in a clean environment. Bundling is opt-in, so `pnpm dev` needs no such files.

## Result files (`crates/tools/src/output.rs`)

- Written next to the original. A taken name gets ` (2)`, ` (3)`…; nothing is ever overwritten.
- In a batch, the first failing file fails the job and the results already written are removed. Cancelling removes them too.
- Each result carries its size and its source's size, so the UI can show "1.2 MB → 340 KB (−72%)", and an optional `note` for anything the user should know.

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

Preferences live in `lib/prefs.ts`, in `localStorage`: favorites, theme, reduced motion, and how often each tool was used. Never file names. Damaged or unavailable storage falls back to defaults. Usage decays by half every two weeks, and it ranks search results and the tools offered for dropped files. `contrast.test.ts` checks that both themes reach 4.5:1.

## Not implemented yet

Tailwind and `packages/ui`; HEIC on Windows; Windows code signing; hardware H.264 on Linux (VA-API).
