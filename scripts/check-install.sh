#!/usr/bin/env bash
# Installs the built package on Linux or Windows and runs it, the way
# scripts/check-bundle.sh does for macOS.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bundle="$root/target/release/bundle"
work=$(mktemp -d)
fail() {
  echo "FAILED: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Linux)
    # A system FFmpeg must not get in the way of installing ours.
    sudo apt-get install -y ffmpeg qpdf >/dev/null
    sudo apt-get install -y "$bundle"/deb/*.deb >/dev/null
    dir=/usr/bin
    exe=""
    ;;
  *)
    "$bundle"/nsis/*-setup.exe //S
    for _ in $(seq 1 60); do
      dir=$(ls -d "$LOCALAPPDATA/Saqviro" "/c/Program Files/Saqviro" 2>/dev/null | head -1 || true)
      [ -n "$dir" ] && [ -f "$dir/saqviro-qpdf.exe" ] && break
      sleep 2
    done
    [ -n "$dir" ] || fail "the installer did not install anything"
    exe=.exe
    ;;
esac
echo "Installed in $dir"

printf '%%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 100]>>endobj\ntrailer<</Root 1 0 R>>\n' >"$work/page.pdf"
"$dir/saqviro-qpdf$exe" --warning-exit-0 --no-warn --show-npages "$work/page.pdf" || fail "qpdf does not work"
"$dir/saqviro-ffmpeg$exe" -version | grep -q 'enable-gpl' && fail "FFmpeg is not LGPL"
"$dir/saqviro-ffmpeg$exe" -y -v error -f lavfi -i sine=d=1 "$work/tone.mp3" || fail "FFmpeg does not work"
if [ "$exe" = .exe ]; then
  "$dir/saqviro-ffmpeg$exe" -y -v error -f lavfi -i testsrc2=s=320x240:d=1 -c:v h264_mf "$work/clip.mp4" \
    && echo "h264_mf works" || echo "h264_mf is not available here: the app falls back to VP9"
fi
mkdir "$work/out"
"$dir/saqviro-pdf-render$exe" "$work/page.pdf" "$work/out" 72 png 85 || fail "PDF rendering does not work"
[ -f "$work/out/1.png" ] || fail "PDF rendering wrote nothing"

if [ "$exe" = .exe ]; then
  "$dir/saqviro-desktop.exe" &
else
  xvfb-run -a "$dir/saqviro-desktop" &
fi
pid=$!
sleep 15
if [ "$exe" = .exe ]; then
  kill -0 "$pid" 2>/dev/null || fail "the app does not start"
else
  # $pid is xvfb-run, which says nothing about the app.
  pgrep -x saqviro-desktop >/dev/null || fail "the app does not start"
fi
kill "$pid" || true
echo "OK"
