#!/usr/bin/env bash
# Writes THIRD-PARTY-NOTICES.html, which ships with the app. Needs cargo-about.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/apps/desktop/src-tauri/binaries/THIRD-PARTY-NOTICES.html"
web="$root/apps/desktop/node_modules"
mkdir -p "$(dirname "$out")"

cd "$root"
crates=$(mktemp)
trap 'rm -f "$crates"' EXIT
cargo about generate --workspace --all-features -c about.toml licenses/about.hbs -o "$crates"

{
  sed '/<!-- ENGINES -->/,$d' "$crates"
  cat licenses/engines.html
  echo '<h2>Interface</h2>'
  for package in svelte @tauri-apps/api @lucide/svelte; do
    echo "<h3>$package</h3><pre>"
    sed 's/&/\&amp;/g; s/</\&lt;/g' "$web/$package"/LICENSE* | head -220
    echo '</pre>'
  done
  sed -n '/<!-- ENGINES -->/,$p' "$crates" | tail -n +2
} > "$out"
echo "Wrote $out"
