# 0002 — Image engine

Status: accepted, 2026-11-03

## Context

Image tools (convert, resize, compress, rotate, metadata removal, HEIC) need decoders and encoders on Windows, macOS and Linux. The plan named libvips.

libvips from Homebrew has 28 direct dependencies; shipping it means bundling 40–60 native libraries per platform. Homebrew's build links poppler (GPL). Its decoders are written in C, and Saqviro opens untrusted files.

## Decision

Pure Rust crates: `image` (decode, EXIF orientation, AVIF encode), `fast_image_resize`, `mozjpeg`, `webp`, `oxipng`.

HEIC is the exception: it is decoded by libheif with libde265, linked dynamically to satisfy the LGPL, behind the `heic` cargo feature. Release builds ship libheif built without x265 (GPL).

## Consequences

- `cargo build` needs no system libraries except libheif for the `heic` feature.
- PNG, JPEG and GIF decoding of untrusted files runs in safe Rust.
- Very large images (100+ megapixels) are slower and use more memory than with libvips.
- AVIF can be written but not read until an AVIF decoder is added.
