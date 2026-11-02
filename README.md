# Saqviro

**Everyday tools. Zero uploads.**

A free, open-source desktop toolbox for Windows, macOS and Linux. Convert, compress and inspect files locally — your files never leave your device. No account, no ads, no tracking.

> Status: early development. Nothing to download yet.

## Development

Requirements: [Rust](https://rustup.rs) (stable), Node.js 24, pnpm, the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS, and on macOS and Linux libheif 1.17+ with pkg-config for HEIC (`brew install libheif pkgconf`, `apt install libheif-dev`). Windows builds do not include HEIC yet. On x86_64, [nasm](https://nasm.us) speeds up the JPEG and AVIF encoders.

```bash
pnpm install
pnpm dev          # run the desktop app
pnpm test         # frontend tests
cargo test --workspace
```

## Repository layout

| Path | What |
|---|---|
| `apps/desktop` | Tauri 2 + Svelte 5 desktop app |
| `crates/core` | Tool manifests, registry and parameter validation |
| `crates/jobs` | Job queue with progress, cancellation and temp directories |
| `crates/tools` | Tool implementations |
| `tools/` | One folder per tool with its `tool.yaml` manifest |
| `packages/tool-schema` | JSON Schema for `tool.yaml`, generated from `crates/core` |
| `docs/` | Developer documentation |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache-2.0](LICENSE)
