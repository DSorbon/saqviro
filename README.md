# Saqviro

**Everyday tools. Zero uploads.**

A free, open-source desktop toolbox for Windows, macOS and Linux. Convert, compress and inspect files locally — your files never leave your device. No account, no ads, no tracking.

> Status: early development. Nothing to download yet.

## Development

Requirements: [Rust](https://rustup.rs) (stable), Node.js 24, pnpm, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

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
| `crates/core` | Tool manifests and registry |
| `tools/` | One folder per tool with its `tool.yaml` manifest |
| `packages/tool-schema` | JSON Schema for `tool.yaml`, generated from `crates/core` |
| `docs/` | Developer documentation |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache-2.0](LICENSE)
