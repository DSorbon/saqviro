#!/usr/bin/env bash
# Checks a built macOS app the way another computer would meet it. Every check
# here stands for a release that once looked fine and would not have run.
# Usage: scripts/check-bundle.sh <Saqviro.app or .dmg>
set -euo pipefail

MIN_MACOS=13.0
input=$1
work=$(mktemp -d)
trap 'hdiutil detach "$work/dmg" -quiet 2>/dev/null || true; rm -rf "$work"' EXIT

case "$input" in
  *.dmg)
    hdiutil attach -nobrowse -readonly -mountpoint "$work/dmg" "$input" >/dev/null
    cp -R "$work/dmg/Saqviro.app" "$work/"
    hdiutil detach "$work/dmg" -quiet
    ;;
  *) cp -R "$input" "$work/" ;;
esac
app="$work/Saqviro.app"
contents="$app/Contents"
fail() {
  echo "FAILED: $*" >&2
  exit 1
}

codesign --verify --deep --strict "$app" || fail "the signature is not valid"

for binary in "$contents"/MacOS/* "$contents"/Frameworks/*; do
  name=$(basename "$binary")
  if otool -L "$binary" | tail -n +2 | grep -Eq '/opt/homebrew|/usr/local'; then
    fail "$name links a library from this machine"
  fi
  if otool -L "$binary" | grep -q x265; then
    fail "$name links x265, which is GPL"
  fi
  minimum=$(vtool -show-build "$binary" | awk '/minos/ { print $2; exit }')
  if [ "$(printf '%s\n%s\n' "$MIN_MACOS" "$minimum" | sort -V | tail -1)" != "$MIN_MACOS" ]; then
    fail "$name needs macOS $minimum, above $MIN_MACOS"
  fi
done

[ -f "$contents/Resources/binaries/THIRD-PARTY-NOTICES.html" ] || fail "the license notices are missing"

# A clean environment, so nothing from this machine helps.
printf '%%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 100]>>endobj\ntrailer<</Root 1 0 R>>\n' >"$work/page.pdf"
mkdir "$work/out"
env -i "$contents/MacOS/saqviro-pdf-render" "$work/page.pdf" "$work/out" 72 png 85 >/dev/null || fail "PDF rendering does not work"
[ -f "$work/out/1.png" ] || fail "PDF rendering wrote nothing"
env -i "$contents/MacOS/saqviro-ffmpeg" -v error -f lavfi -i sine=d=1 "$work/out/tone.mp3" || fail "FFmpeg does not work"
env -i "$contents/MacOS/saqviro-qpdf" --warning-exit-0 --no-warn --show-npages "$work/page.pdf" >/dev/null || fail "qpdf does not work"

env -i "$contents/MacOS/saqviro-desktop" >"$work/start.log" 2>&1 &
pid=$!
sleep 5
if ! kill -0 "$pid" 2>/dev/null; then
  cat "$work/start.log" >&2
  fail "the app does not start"
fi
kill "$pid"
echo "OK: $input"
