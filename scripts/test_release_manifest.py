#!/usr/bin/env python3
"""Checks release-manifest.py. Run it directly: it needs nothing installed."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

script = Path(__file__).with_name("release-manifest.py")
spec = importlib.util.spec_from_file_location("release_manifest", script)
release_manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_manifest)

FILES = {
    "macos-arm64/macos/Saqviro.app.tar.gz": "arm",
    "macos-x64/macos/Saqviro.app.tar.gz": "intel",
    "linux-x64/appimage/Saqviro_1.2.3_amd64.AppImage": "linux",
    "windows-x64/nsis/Saqviro_1.2.3_x64-setup.exe": "windows",
}


class Manifest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.artifacts = Path(folder.name)
        for name, signature in FILES.items():
            path = self.artifacts / name
            path.parent.mkdir(parents=True)
            path.write_text("update")
            path.with_name(path.name + ".sig").write_text(signature)
        release_manifest.name_by_platform("1.2.3", self.artifacts)
        self.platforms = release_manifest.manifest("1.2.3", self.artifacts)["platforms"]

    def test_every_file_has_a_name_of_its_own(self):
        names = [p.name for p in self.artifacts.rglob("*") if p.is_file()]
        self.assertEqual(len(names), len(set(names)))

    def test_every_platform_gets_its_own_update(self):
        self.assertEqual(len({p["url"] for p in self.platforms.values()}), 4)
        self.assertEqual(self.platforms["darwin-aarch64"]["signature"], "arm")
        self.assertEqual(self.platforms["darwin-x86_64"]["signature"], "intel")

    def test_urls_name_files_that_will_be_uploaded(self):
        names = {p.name for p in self.artifacts.rglob("*")}
        for platform in self.platforms.values():
            self.assertIn(platform["url"].rsplit("/", 1)[1], names)


if __name__ == "__main__":
    unittest.main()
