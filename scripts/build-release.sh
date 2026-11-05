#!/usr/bin/env bash
# Builds installers from what scripts/prepare-bundle.sh prepared.
# Usage: scripts/build-release.sh [bundles], for example `dmg` or `deb,appimage`.
#
# Release builds set SAQVIRO_UPDATER_PUBKEY, TAURI_SIGNING_PRIVATE_KEY and, on
# macOS, the APPLE_* variables. Without them the result is unsigned or
# ad-hoc signed and offers no updates: fine to try, not to publish.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}

config=(--config src-tauri/tauri.bundle.conf.json)
case "$(uname -s)" in
  Darwin)
    # Link the libheif that ships, not one installed on this machine.
    export PKG_CONFIG_PATH="$root/target/libheif-build/install/lib/pkgconfig"
    export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"
    config+=(--config src-tauri/tauri.bundle.macos.conf.json)
    # Hardened runtime refuses libraries from an ad-hoc signature, so the app
    # would not start. Notarization needs it, and a real identity satisfies it.
    if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
      config+=(--config '{"bundle":{"macOS":{"hardenedRuntime":true}}}')
    fi
    ;;
  MINGW* | MSYS*) config+=(--config src-tauri/tauri.bundle.windows.conf.json) ;;
  *) config+=(--config src-tauri/tauri.bundle.linux.conf.json) ;;
esac
# The two go together: an app built with the public key needs this section to
# start at all, and signing needs the private key.
if [ -n "${SAQVIRO_UPDATER_PUBKEY:-}" ] || [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  : "${SAQVIRO_UPDATER_PUBKEY:?set it together with TAURI_SIGNING_PRIVATE_KEY}"
  : "${TAURI_SIGNING_PRIVATE_KEY:?set it together with SAQVIRO_UPDATER_PUBKEY}"
  manifest=https://github.com/DSorbon/saqviro/releases/latest/download/latest.json
  config+=(--config "{\"bundle\":{\"createUpdaterArtifacts\":true},\"plugins\":{\"updater\":{\"pubkey\":\"$SAQVIRO_UPDATER_PUBKEY\",\"endpoints\":[\"$manifest\"]}}}")
fi

cd "$root/apps/desktop"
args=()
[ $# -gt 0 ] && args=(--bundles "$1")
pnpm tauri build "${args[@]}" "${config[@]}"
