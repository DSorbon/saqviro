#!/usr/bin/env bash
# Downloads the official qpdf for Windows, checking its hash.
set -euo pipefail

VERSION=12.4.2
SHA256=db87077e683630c1217e0e8f9a20a9749d952ab676e881c3689187763a5de25d
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/apps/desktop/src-tauri/binaries"
triple=$(rustc -vV | sed -n 's/^host: //p')
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSLo "$work/qpdf.zip" \
  "https://github.com/qpdf/qpdf/releases/download/v$VERSION/qpdf-$VERSION-msvc64.zip"
echo "$SHA256  $work/qpdf.zip" | sha256sum -c -
unzip -q "$work/qpdf.zip" -d "$work"
mkdir -p "$out"
cp "$work"/qpdf-*/bin/qpdf.exe "$out/qpdf-$triple.exe"
cp "$work"/qpdf-*/bin/*.dll "$out/" 2>/dev/null || true
