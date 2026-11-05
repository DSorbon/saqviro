# Releasing

Pushing a tag like `v0.1.0` runs `.github/workflows/release.yml`, which builds every platform into a draft release. A person tests it and publishes.

## One-time setup

Create the update key and keep the private half safe. Losing it means existing installs can never update again.

```bash
pnpm --filter @saqviro/desktop tauri signer generate -w ~/.tauri/saqviro.key
```

| Name | Kind | Value |
|---|---|---|
| `UPDATER_PUBKEY` | variable | contents of `saqviro.key.pub` |
| `UPDATER_PRIVATE_KEY` | secret | contents of `saqviro.key` |
| `UPDATER_PRIVATE_KEY_PASSWORD` | secret | its password |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | secret | Developer ID certificate as Base64, and its password |
| `APPLE_SIGNING_IDENTITY` | secret | `Developer ID Application: …` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | secret | for notarization |

Without the Apple values the macOS build is ad-hoc signed and Gatekeeper blocks it. Windows signing is not set up yet.

## Building locally

```bash
scripts/prepare-bundle.sh
```

```bash
scripts/build-release.sh dmg
```

```bash
scripts/check-bundle.sh target/release/bundle/dmg/Saqviro_0.1.0_aarch64.dmg
```

## Updating an engine

Change the version and hash at the top of its script, and the table in `licenses/engines.html`. FFmpeg, libheif and libde265 are LGPL: the scripts are how users can rebuild them, so they must stay accurate.
