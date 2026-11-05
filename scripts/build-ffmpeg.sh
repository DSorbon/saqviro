#!/usr/bin/env bash
# Builds the FFmpeg and ffprobe that ship with Saqviro, and puts them where
# Tauri picks them up. The build must stay LGPL: see
# docs/decisions/0003-media-engine.md.
#
# Codec libraries are built here too, statically: that keeps the result
# independent of the build machine, including its macOS version.
#
# Needs a C toolchain, make and pkg-config; nasm on x86_64.
# On Windows, run it in an MSYS2 UCRT64 shell.
set -euo pipefail

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
VPX_VERSION=1.17.0
VPX_SHA256=1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a
OPUS_VERSION=1.6.1
OPUS_SHA256=6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1
LAME_VERSION=4.0
LAME_SHA256=3df5124d5ad3a98312ffd7ba6a9b36230e4f8a3e66d3ce0f425e336c32d216eb

export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}

root=$(cd "$(dirname "$0")/.." && pwd)
work=${FFMPEG_BUILD_DIR:-"$root/target/ffmpeg-build"}
deps="$work/deps"
out="$root/apps/desktop/src-tauri/binaries"
triple=$(rustc -vV | sed -n 's/^host: //p')
jobs=$(getconf _NPROCESSORS_ONLN)
if command -v sha256sum >/dev/null; then sum=sha256sum; else sum="shasum -a 256"; fi

# fetch <file> <url> <sha256> <folder it unpacks to>
fetch() {
  [ -f "$1" ] || curl -fsSLo "$1" "$2"
  echo "$3  $1" | $sum -c -
  rm -rf "$4"
  tar xf "$1"
}

mkdir -p "$work" "$out"
cd "$work"
rm -rf "$deps"

fetch "libvpx-$VPX_VERSION.tar.gz" "https://github.com/webmproject/libvpx/archive/refs/tags/v$VPX_VERSION.tar.gz" \
  "$VPX_SHA256" "libvpx-$VPX_VERSION"
vpx_target=()
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) vpx_target=(--target=arm64-darwin23-gcc) ;;
  Darwin-x86_64) vpx_target=(--target=x86_64-darwin23-gcc) ;;
esac
(
  cd "libvpx-$VPX_VERSION"
  ./configure --prefix="$deps" --enable-static --disable-shared --enable-pic \
    --disable-examples --disable-tools --disable-docs --disable-unit-tests "${vpx_target[@]}"
  make -j"$jobs"
  make install
)

fetch "opus-$OPUS_VERSION.tar.gz" "https://ftp.osuosl.org/pub/xiph/releases/opus/opus-$OPUS_VERSION.tar.gz" \
  "$OPUS_SHA256" "opus-$OPUS_VERSION"
(
  cd "opus-$OPUS_VERSION"
  ./configure --prefix="$deps" --enable-static --disable-shared --with-pic --disable-doc --disable-extra-programs
  make -j"$jobs"
  make install
)

fetch "lame-$LAME_VERSION.tar.gz" "https://downloads.sourceforge.net/project/lame/lame/$LAME_VERSION/lame-$LAME_VERSION.tar.gz" \
  "$LAME_SHA256" "lame-$LAME_VERSION"
(
  cd "lame-$LAME_VERSION"
  ./configure --prefix="$deps" --enable-static --disable-shared --with-pic --disable-frontend --disable-decoder
  make -j"$jobs"
  make install
)

fetch "ffmpeg-$VERSION.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz" "$SHA256" "ffmpeg-$VERSION"

platform=()
exe=""
case "$(uname -s)" in
  Darwin) platform=(--enable-videotoolbox --enable-audiotoolbox) ;;
  MINGW* | MSYS*)
    platform=(--enable-mediafoundation)
    exe=.exe
    ;;
esac

cd "ffmpeg-$VERSION"
# Never add --enable-gpl, --enable-nonfree or --enable-version3 here.
PKG_CONFIG_PATH="$deps/lib/pkgconfig" ./configure \
  --prefix="$work/install" \
  --pkg-config-flags=--static \
  --disable-autodetect \
  --disable-doc --disable-debug --disable-ffplay \
  --disable-network \
  --disable-protocols --enable-protocol=file,pipe \
  --disable-devices --enable-indev=lavfi \
  --enable-libvpx --enable-libopus --enable-libmp3lame \
  --enable-zlib \
  --extra-cflags="-I$deps/include" \
  --extra-ldflags="-L$deps/lib" \
  "${platform[@]}"
make -j"$jobs"
make install

for tool in ffmpeg ffprobe; do
  built="$work/install/bin/$tool$exe"
  if "$built" -version | grep -qE -- '--enable-(gpl|nonfree)'; then
    echo "$tool is not LGPL" >&2
    exit 1
  fi
  "$built" -L | grep -q 'Lesser General Public' || {
    echo "$tool does not report the LGPL" >&2
    exit 1
  }
  if [ "$(uname -s)" = Darwin ] && otool -L "$built" | grep -Eq '/opt/homebrew|/usr/local'; then
    echo "$tool links libraries that exist only on this machine" >&2
    exit 1
  fi
  cp "$built" "$out/$tool-$triple$exe"
done
echo "Built into $out"
