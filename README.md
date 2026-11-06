# Saqviro

**Everyday tools. Zero uploads.**

A free, open-source desktop toolbox for Windows, macOS and Linux. Convert, compress and inspect files locally — your files never leave your device. No account, no ads, no tracking.

> Status: early development. Nothing to download yet.

## What it does

[139 tools](docs/tools.md) for PDF, images, video, audio, files, text, QR codes, developers, privacy and more. Drop a file and Saqviro shows what it can do with it. [Workflows](docs/workflows.md) run several tools in a row, and every tool works offline. [PRIVACY.md](PRIVACY.md) says exactly what the app does and does not do with your data.

## Development

Requirements: [Rust](https://rustup.rs) (stable), Node.js 24, pnpm, the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS, and on macOS and Linux libheif 1.17+ with pkg-config for HEIC (`brew install libheif pkgconf`, `apt install libheif-dev libheif-plugin-libde265`). Windows builds do not include HEIC yet. Video, audio and PDF tools need FFmpeg and qpdf on your `PATH` during development (`brew install ffmpeg qpdf`, `apt install ffmpeg qpdf`), and `scripts/fetch-pdfium.sh` once for PDF rendering; releases ship their own builds. On x86_64, [nasm](https://nasm.us) speeds up the JPEG and AVIF encoders.

```bash
pnpm install
pnpm dev          # run the desktop app
pnpm test         # frontend tests
cargo test --workspace
```

## Command line

The tools also run from a terminal, with the same checks as in the app:

```bash
cargo run -p saqviro-cli -- list text
```

```bash
cargo run -p saqviro-cli -- run image.resize --width 800 --out results photo.jpg
```

```bash
echo hello | cargo run -p saqviro-cli -- run text.case --case upper
```

## Repository layout

| Path | What |
|---|---|
| `apps/desktop` | Tauri 2 + Svelte 5 desktop app |
| `apps/cli` | The same tools on the command line |
| `crates/core` | Tool manifests, registry and parameter validation |
| `crates/jobs` | Job queue with progress, cancellation and temp directories |
| `crates/tools` | Tool implementations |
| `crates/workflow` | Runs tools one after another |
| `tools/` | One folder per tool with its `tool.yaml` manifest |
| `packages/tool-schema` | JSON Schema for `tool.yaml`, generated from `crates/core` |
| `packages/workflow-schema` | JSON Schema for workflows, and the starter workflows |
| `docs/` | Developer documentation |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache-2.0](LICENSE)
