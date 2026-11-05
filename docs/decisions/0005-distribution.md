# 0005: Distribution

Status: accepted, 2026-11-05

## Context

A build that works on the machine that made it proves little. The first release
bundle linked Homebrew's libheif, which pulled in GPL x265; its engines required
the build machine's macOS version; and it carried the Tauri logo. No test noticed.

## Decision

- **Everything ships inside the app.** Codec libraries are built from pinned
  sources with their hashes checked, so nothing depends on the build machine.
- **macOS 13 is the minimum**, set by the PDFium builds we use.
- **libheif ships as a dynamic library**, decoder only, as the LGPL expects.
  Linux packages depend on the system libheif. Windows leaves HEIC out for now.
- **Library files go in `Contents/Frameworks`** on macOS, where signing and
  notarization cover them.
- **Hardened runtime only with a real signing identity.** It refuses libraries
  under an ad-hoc signature, and the app then fails to start.
- **`scripts/check-bundle.sh` gates macOS releases.** It runs the built app in a
  clean environment instead of trusting the build.

## Updates

The app contacts the update server only when the user presses Check for
updates. Updates are verified against a public key compiled in through
`SAQVIRO_UPDATER_PUBKEY`. A build without the key offers no updates. Tauri's
updater plugin refuses to start without its config section, so
`scripts/build-release.sh` always sets the key and that section together.

## Consequences

- Unsigned builds are blocked by Gatekeeper until the project has an Apple
  Developer account.
- Windows and Linux installers exist only as a CI workflow until it has run.
