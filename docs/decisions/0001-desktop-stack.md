# 0001 — Desktop stack

Status: accepted, 2026-11-03

## Context

The app has 50–200 small, form-like tool screens and must be accessible (keyboard, screen readers, IME, HiDPI) on Windows, macOS and Linux. We compared Tauri 2, egui, Slint, iced, Dioxus and GPUI.

## Decision

Tauri 2 with Svelte 5 and Vite (no SvelteKit). All logic lives in Rust. Tool screens are generated from manifests.

egui was rejected for its non-native look, limited accessibility and text shaping. Slint is the fallback if Linux WebKitGTK proves unworkable; keeping the UI thin and manifest-driven keeps that switch possible.

## Consequences

- Webview memory use (40–100 MB) is accepted.
- Linux builds depend on WebKitGTK; we test on NVIDIA and Wayland and prefer Flatpak.
- License: Apache-2.0. GPL and AGPL dependencies are rejected by `cargo-deny`.
