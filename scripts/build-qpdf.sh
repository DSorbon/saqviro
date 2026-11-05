#!/usr/bin/env bash
# Builds the qpdf that ships with Saqviro, statically, with its own crypto
# provider, and puts it where Tauri picks it up. macOS and Linux only for now.
#
# Needs cmake, a C++ toolchain and a static libjpeg
# (macOS: brew install cmake jpeg-turbo; Linux: apt install cmake libjpeg-dev).
set -euo pipefail

VERSION=12.4.2
SHA256=8a58af5b6141319287c1883bec8bd1bd545b7567b7fc5e6ce5d25a1c85f36397

root=$(cd "$(dirname "$0")/.." && pwd)
work=${QPDF_BUILD_DIR:-"$root/target/qpdf-build"}
out="$root/apps/desktop/src-tauri/binaries"
triple=$(rustc -vV | sed -n 's/^host: //p')

mkdir -p "$work" "$out"
cd "$work"

tarball="qpdf-$VERSION.tar.gz"
[ -f "$tarball" ] || curl -fsSLo "$tarball" "https://github.com/qpdf/qpdf/releases/download/v$VERSION/$tarball"
if command -v sha256sum >/dev/null; then sum=sha256sum; else sum="shasum -a 256"; fi
echo "$SHA256  $tarball" | $sum -c -
rm -rf "qpdf-$VERSION" build
tar xf "$tarball"

jpeg=$(find /opt/homebrew/opt/jpeg-turbo/lib /opt/homebrew/lib /usr/local/lib /usr/lib -name libjpeg.a 2>/dev/null | head -1)
[ -n "$jpeg" ] || { echo "static libjpeg not found" >&2; exit 1; }
include="$(dirname "$(dirname "$jpeg")")/include"

cmake -S "qpdf-$VERSION" -B build \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF -DBUILD_STATIC_LIBS=ON \
  -DUSE_IMPLICIT_CRYPTO=OFF -DREQUIRE_CRYPTO_NATIVE=ON \
  -DBUILD_DOC=OFF -DINSTALL_EXAMPLES=OFF \
  -DSTATIC_JPEG=ON -DLIBJPEG_STATIC_PATH="$jpeg" -DLIBJPEG_LIB_PATH="$jpeg" -DLIBJPEG_H_PATH="$include"
cmake --build build --target qpdf --parallel "$(getconf _NPROCESSORS_ONLN)"

if [ "$(uname -s)" = Darwin ] && otool -L build/qpdf/qpdf | grep -q /opt/homebrew; then
  echo "qpdf links Homebrew libraries, so it would not run elsewhere" >&2
  exit 1
fi
cp build/qpdf/qpdf "$out/qpdf-$triple"
"$out/qpdf-$triple" --version | head -1
echo "Built into $out"
