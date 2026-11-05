#!/usr/bin/env bash
# Builds the qpdf that ships with Saqviro, statically, with its own crypto
# provider and libjpeg, and puts it where Tauri picks it up. Windows uses the
# official binaries instead: see scripts/fetch-qpdf-windows.sh.
#
# Needs cmake and a C++ toolchain; nasm on x86_64.
set -euo pipefail

VERSION=12.4.2
SHA256=8a58af5b6141319287c1883bec8bd1bd545b7567b7fc5e6ce5d25a1c85f36397
JPEG_VERSION=3.2.1
JPEG_SHA256=c27ad72a68da151177929aa969ae773a2da5ac2af9ce946079314fdac8cece7f

export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}

root=$(cd "$(dirname "$0")/.." && pwd)
work=${QPDF_BUILD_DIR:-"$root/target/qpdf-build"}
out="$root/apps/desktop/src-tauri/binaries"
triple=$(rustc -vV | sed -n 's/^host: //p')

mkdir -p "$work" "$out"
cd "$work"

if command -v sha256sum >/dev/null; then sum=sha256sum; else sum="shasum -a 256"; fi

# fetch <file> <url> <sha256> <folder it unpacks to>
fetch() {
  [ -f "$1" ] || curl -fsSLo "$1" "$2"
  echo "$3  $1" | $sum -c -
  rm -rf "$4"
  tar xf "$1"
}

fetch "libjpeg-turbo-$JPEG_VERSION.tar.gz" \
  "https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/$JPEG_VERSION/libjpeg-turbo-$JPEG_VERSION.tar.gz" \
  "$JPEG_SHA256" "libjpeg-turbo-$JPEG_VERSION"
jpeg="$work/jpeg"
rm -rf "$jpeg" build-jpeg build
cmake -S "libjpeg-turbo-$JPEG_VERSION" -B build-jpeg -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$jpeg" -DCMAKE_INSTALL_LIBDIR=lib -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF
cmake --build build-jpeg --parallel "$(getconf _NPROCESSORS_ONLN)"
cmake --install build-jpeg

fetch "qpdf-$VERSION.tar.gz" "https://github.com/qpdf/qpdf/releases/download/v$VERSION/qpdf-$VERSION.tar.gz" \
  "$SHA256" "qpdf-$VERSION"

# Without an empty pkg-config path, cmake finds this machine's libjpeg.
PKG_CONFIG_LIBDIR="$jpeg/lib/pkgconfig" cmake -S "qpdf-$VERSION" -B build \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF -DBUILD_STATIC_LIBS=ON \
  -DUSE_IMPLICIT_CRYPTO=OFF -DREQUIRE_CRYPTO_NATIVE=ON \
  -DBUILD_DOC=OFF -DINSTALL_EXAMPLES=OFF -DSTATIC_JPEG=ON \
  -DLIBJPEG_STATIC_PATH="$jpeg/lib/libjpeg.a" -DLIBJPEG_LIB_PATH="$jpeg/lib/libjpeg.a" -DLIBJPEG_H_PATH="$jpeg/include"
cmake --build build --target qpdf --parallel "$(getconf _NPROCESSORS_ONLN)"

if [ "$(uname -s)" = Darwin ] && otool -L build/qpdf/qpdf | grep -q /opt/homebrew; then
  echo "qpdf links Homebrew libraries, so it would not run elsewhere" >&2
  exit 1
fi
cp build/qpdf/qpdf "$out/qpdf-$triple"
"$out/qpdf-$triple" --version | head -1
echo "Built into $out"
