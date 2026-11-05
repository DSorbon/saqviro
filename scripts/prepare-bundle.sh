#!/usr/bin/env bash
# Puts every engine and the license notices into
# apps/desktop/src-tauri/binaries, ready for scripts/build-release.sh.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
triple=$(rustc -vV | sed -n 's/^host: //p')
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}

case "$(uname -s)" in
  MINGW* | MSYS*)
    exe=.exe
    "$root/scripts/fetch-qpdf-windows.sh"
    ;;
  Darwin)
    exe=""
    "$root/scripts/build-qpdf.sh"
    "$root/scripts/build-libheif.sh"
    ;;
  *)
    exe=""
    "$root/scripts/build-qpdf.sh"
    ;;
esac
"$root/scripts/build-ffmpeg.sh"
"$root/scripts/fetch-pdfium.sh"

cargo build --release --manifest-path "$root/Cargo.toml" -p saqviro-tools --bin saqviro-pdf-render
cp "$root/target/release/saqviro-pdf-render$exe" \
  "$root/apps/desktop/src-tauri/binaries/saqviro-pdf-render-$triple$exe"

"$root/scripts/third-party-notices.sh"
