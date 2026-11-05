#!/usr/bin/env bash
# Downloads the PDFium build that matches the `pdfium_8125` bindings into
# apps/desktop/src-tauri/binaries, checking its hash.
set -euo pipefail

RELEASE=8125
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) asset=pdfium-mac-arm64 sha=4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d lib=lib/libpdfium.dylib ;;
  Darwin-x86_64) asset=pdfium-mac-x64 sha=8a5193535b80c7215d8932f6647ae5097465b6dba2811602c21e120c2af112b0 lib=lib/libpdfium.dylib ;;
  Linux-x86_64) asset=pdfium-linux-x64 sha=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659 lib=lib/libpdfium.so ;;
  *) echo "no PDFium build configured for this platform" >&2; exit 1 ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/apps/desktop/src-tauri/binaries"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSLo "$work/pdfium.tgz" \
  "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$RELEASE/$asset.tgz"
if command -v sha256sum >/dev/null; then sum=sha256sum; else sum="shasum -a 256"; fi
echo "$sha  $work/pdfium.tgz" | $sum -c -
tar xzf "$work/pdfium.tgz" -C "$work" "$lib" LICENSE
mkdir -p "$out"
cp "$work/$lib" "$out/"
cp "$work/LICENSE" "$out/PDFIUM-LICENSE"
echo "PDFium $RELEASE is in $out"
