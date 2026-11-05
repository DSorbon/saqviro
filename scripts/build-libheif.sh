#!/usr/bin/env bash
# Builds libheif with only the libde265 decoder, as shared libraries the app
# ships. It must stay free of x265, which is GPL: see
# docs/decisions/0002-image-engine.md. macOS only: Linux packages depend on
# the system libheif, and Windows builds leave HEIC out for now.
#
# Needs cmake and a C++ toolchain.
set -euo pipefail

export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}

DE265_VERSION=1.1.3
DE265_SHA256=554228bd17788c99a7e63b37ab5634722190e6e2bf60c1dcb01cef328e133905
HEIF_VERSION=1.23.7
HEIF_SHA256=f7483e818503ea6224c8863e4a56ac02c7089021a98472c811c12aa19faf2475

root=$(cd "$(dirname "$0")/.." && pwd)
work=${LIBHEIF_BUILD_DIR:-"$root/target/libheif-build"}
prefix="$work/install"
out="$root/apps/desktop/src-tauri/binaries"
jobs=$(getconf _NPROCESSORS_ONLN)

fetch() {
  [ -f "$1" ] || curl -fsSLo "$1" "$2"
  echo "$3  $1" | shasum -a 256 -c -
  rm -rf "${1%.tar.gz}"
  tar xf "$1"
}

mkdir -p "$work" "$out"
cd "$work"
rm -rf "$prefix" build-de265 build-heif
fetch "libde265-$DE265_VERSION.tar.gz" \
  "https://github.com/strukturag/libde265/releases/download/v$DE265_VERSION/libde265-$DE265_VERSION.tar.gz" "$DE265_SHA256"
fetch "libheif-$HEIF_VERSION.tar.gz" \
  "https://github.com/strukturag/libheif/releases/download/v$HEIF_VERSION/libheif-$HEIF_VERSION.tar.gz" "$HEIF_SHA256"

common=(
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_INSTALL_PREFIX="$prefix"
  -DCMAKE_PREFIX_PATH="$prefix"
  -DCMAKE_INSTALL_NAME_DIR=@rpath
  -DCMAKE_INSTALL_LIBDIR=lib
  -DBUILD_SHARED_LIBS=ON
)

cmake -S "libde265-$DE265_VERSION" -B build-de265 "${common[@]}" \
  -DENABLE_DECODER=OFF -DENABLE_ENCODER=OFF -DENABLE_SDL=OFF
cmake --build build-de265 --parallel "$jobs"
cmake --install build-de265

# Without these, cmake picks up whatever codecs this machine has installed.
PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig" cmake -S "libheif-$HEIF_VERSION" -B build-heif "${common[@]}" \
  -DWITH_LIBDE265=ON -DWITH_X265=OFF -DWITH_KVAZAAR=OFF -DWITH_UVG266=OFF -DWITH_VVDEC=OFF -DWITH_VVENC=OFF \
  -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF -DWITH_DAV1D=OFF -DWITH_RAV1E=OFF -DWITH_SvtEnc=OFF \
  -DWITH_JPEG_DECODER=OFF -DWITH_JPEG_ENCODER=OFF -DWITH_OpenJPEG_DECODER=OFF -DWITH_OpenJPEG_ENCODER=OFF \
  -DWITH_OpenH264_DECODER=OFF -DWITH_OpenH264_ENCODER=OFF -DWITH_X264=OFF -DWITH_FFMPEG_DECODER=OFF \
  -DWITH_LIBSHARPYUV=OFF -DWITH_UNCOMPRESSED_CODEC=OFF -DWITH_HEADER_COMPRESSION=OFF \
  -DWITH_EXAMPLES=OFF -DWITH_GDK_PIXBUF=OFF -DBUILD_TESTING=OFF -DENABLE_PLUGIN_LOADING=OFF
cmake --build build-heif --parallel "$jobs"
cmake --install build-heif

heif="$prefix/lib/libheif.1.dylib"
if otool -L "$heif" | grep -Eq 'x265|/opt/homebrew|/usr/local'; then
  echo "libheif links libraries it must not:" >&2
  otool -L "$heif" >&2
  exit 1
fi
for lib in libheif.1.dylib libde265.0.dylib; do
  cp -L "$prefix/lib/$lib" "$out/$lib"
done
echo "Built into $out"
