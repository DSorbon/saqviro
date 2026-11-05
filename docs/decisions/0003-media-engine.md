# 0003: Media engine

Status: accepted, 2026-11-05

## Context

Video and audio tools need decoders and encoders for dozens of formats. Saqviro
is Apache-2.0, so nothing GPL can ship with it, and that rules out x264 and
x265. Media files are also untrusted input: FFmpeg can follow references inside
a file, so a playlist renamed to `.mkv` could read other local files.

## Decision

Run FFmpeg and ffprobe as separate processes, built without `--enable-gpl` and
shipped next to the app binary. Separate processes keep LGPL obligations simple,
keep a decoder crash away from the app, and make cancelling a matter of killing
a process.

- **H.264** comes from the encoder in the operating system: VideoToolbox on
  macOS, Media Foundation on Windows. Each encoder is tried once with a test
  frame, because being compiled in does not mean it works on this machine.
- **VP9 is the fallback.** Without a working H.264 encoder, MP4 and MOV results
  are written as WebM and the result says why. Linux always takes this path for
  now: VA-API needs a device and upload filters we cannot test yet.
- **libx264 is never requested**, even when a development build has it. Encoders
  are always named explicitly.
- **Inputs are confined.** Every input is opened as `file:<path>` with
  `-protocol_whitelist file` and a `-format_whitelist` of real media demuxers.
  That excludes concat, HLS, DASH and other formats that reference further files.
- **Results go to the job's temp directory first** and move next to the original
  only when FFmpeg succeeds, so a failed run leaves nothing behind.

## Finding the binaries

Release builds look only next to the app binary. Debug builds also search
`PATH`, so development and tests can use a system FFmpeg. Release builds must
never search `PATH`: that would run whatever `ffmpeg` the user's environment
provides, and could bring in a GPL build.

## Consequences

- Hardware H.264 gives larger files than x264 at the same quality.
- `scripts/build-ffmpeg.sh` builds the shipped binaries and fails if the result
  reports `--enable-gpl`.
- Tools that re-encode are bound to what the machine offers, so the same tool
  can produce MP4 on one computer and WebM on another.
