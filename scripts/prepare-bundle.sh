#!/usr/bin/env bash
# Puts every engine into apps/desktop/src-tauri/binaries, ready for
# `tauri build --config src-tauri/tauri.bundle.conf.json`.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
triple=$(rustc -vV | sed -n 's/^host: //p')

"$root/scripts/build-ffmpeg.sh"
"$root/scripts/build-qpdf.sh"
"$root/scripts/fetch-pdfium.sh"

cargo build --release --manifest-path "$root/Cargo.toml" -p saqviro-tools --bin saqviro-pdf-render
cp "$root/target/release/saqviro-pdf-render" "$root/apps/desktop/src-tauri/binaries/saqviro-pdf-render-$triple"
