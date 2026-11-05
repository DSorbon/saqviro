#!/usr/bin/env python3
"""Writes latest.json, the manifest the updater reads, from release artifacts.

Usage: release-manifest.py <version> <artifacts folder> <output file>
"""

import json
import sys
from datetime import datetime, timezone
from pathlib import Path

BASE = "https://github.com/DSorbon/saqviro/releases/latest/download"

# Which update file serves which platform, by the artifact folder it is in.
PLATFORMS = {
    "macos-arm64": ("darwin-aarch64", ".app.tar.gz"),
    "macos-x64": ("darwin-x86_64", ".app.tar.gz"),
    "linux-x64": ("linux-x86_64", ".AppImage"),
    "windows-x64": ("windows-x86_64", ".exe"),
}


def manifest(version: str, artifacts: Path) -> dict:
    platforms = {}
    for folder, (platform, suffix) in PLATFORMS.items():
        for signature in sorted((artifacts / folder).rglob(f"*{suffix}.sig")):
            update = signature.with_suffix("")
            platforms[platform] = {
                "signature": signature.read_text().strip(),
                "url": f"{BASE}/{update.name}",
            }
    if not platforms:
        sys.exit("no signed update files found: is the updater key set?")
    return {
        "version": version,
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }


if __name__ == "__main__":
    version, artifacts, output = sys.argv[1:]
    Path(output).write_text(json.dumps(manifest(version, Path(artifacts)), indent=2) + "\n")
