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

# Relative paths: unzip may be a Windows program that cannot read MSYS paths.
cd "$work"
curl -fsSL --retry 5 --retry-all-errors -o qpdf.zip \
  "https://github.com/qpdf/qpdf/releases/download/v$VERSION/qpdf-$VERSION-msvc64.zip"
echo "$SHA256  qpdf.zip" | sha256sum -c -
unzip -q qpdf.zip
mkdir -p "$out"
cp qpdf-*/bin/qpdf.exe "$out/saqviro-qpdf-$triple.exe"
cp qpdf-*/bin/*.dll "$out/" 2>/dev/null || true
