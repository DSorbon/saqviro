#!/usr/bin/env bash
# Builds the FFmpeg and ffprobe that ship with Saqviro, and puts them where
# Tauri picks them up. The build must stay LGPL: see
# docs/decisions/0003-media-engine.md. macOS and Linux only for now.
#
# Needs a C toolchain, pkg-config, and static libvpx, opus and lame
# (macOS: brew install libvpx opus lame nasm pkgconf;
#  Linux: apt install nasm libvpx-dev libopus-dev libmp3lame-dev).
set -euo pipefail

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e

root=$(cd "$(dirname "$0")/.." && pwd)
work=${FFMPEG_BUILD_DIR:-"$root/target/ffmpeg-build"}
out="$root/apps/desktop/src-tauri/binaries"
triple=$(rustc -vV | sed -n 's/^host: //p')
jobs=$(getconf _NPROCESSORS_ONLN)

mkdir -p "$work" "$out"
cd "$work"

tarball="ffmpeg-$VERSION.tar.xz"
[ -f "$tarball" ] || curl -fsSLo "$tarball" "https://ffmpeg.org/releases/$tarball"
if command -v sha256sum >/dev/null; then sum=sha256sum; else sum="shasum -a 256"; fi
echo "$SHA256  $tarball" | $sum -c -
rm -rf "ffmpeg-$VERSION"
tar xf "$tarball"

# Homebrew's lame needs mpg123 (LGPL) for its decoder.
libs=(vpx opus mp3lame)
extra_libs=""
if [ "$(uname -s)" = Darwin ]; then
  libs+=(mpg123)
  extra_libs="-lmpg123"
fi

# Linkers prefer shared libraries, so offer only the static ones. The result
# then runs on machines without these libraries.
static="$work/static"
rm -rf "$static" && mkdir -p "$static"
for lib in "${libs[@]}"; do
  found=$(find /opt/homebrew/lib /opt/homebrew/opt /usr/local/lib /usr/lib -name "lib$lib.a" 2>/dev/null | head -1)
  [ -n "$found" ] || { echo "static lib$lib not found" >&2; exit 1; }
  ln -s "$found" "$static/"
done

# lame has no pkg-config file, so its headers need a path.
platform=()
includes=""
case "$(uname -s)" in
  Darwin)
    platform=(--enable-videotoolbox --enable-audiotoolbox)
    includes="-I$(brew --prefix)/include"
    ;;
esac

cd "ffmpeg-$VERSION"
# Never add --enable-gpl, --enable-nonfree or --enable-version3 here.
./configure \
  --prefix="$work/install" \
  --disable-autodetect \
  --disable-doc --disable-debug --disable-ffplay \
  --disable-network \
  --disable-protocols --enable-protocol=file,pipe \
  --disable-devices --enable-indev=lavfi \
  --enable-libvpx --enable-libopus --enable-libmp3lame \
  --enable-zlib \
  --extra-cflags="$includes" \
  --extra-ldflags="-L$static" \
  --extra-libs="$extra_libs" \
  "${platform[@]}"
make -j"$jobs"
make install

for tool in ffmpeg ffprobe; do
  built="$work/install/bin/$tool"
  if "$built" -version | grep -qE -- '--enable-(gpl|nonfree)'; then
    echo "$tool is not LGPL" >&2
    exit 1
  fi
  "$built" -version | grep -q 'LGPL' 2>/dev/null || "$built" -L | grep -q 'Lesser General Public' || {
    echo "$tool does not report the LGPL" >&2
    exit 1
  }
  cp "$built" "$out/$tool-$triple"
done
echo "Built into $out"
