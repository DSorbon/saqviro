# 0004: PDF engine

Status: accepted, 2026-11-05

## Context

PDF tools need structural edits, passwords, rendering and real compression.
MuPDF and Ghostscript do all of it but are AGPL, which Apache-2.0 cannot ship.
PDFs are hostile input with a long history of parser bugs.

## Decision

Three engines, each doing what it is best at:

- **qpdf** (Apache-2.0), run as a process, for everything structural: merge,
  split, page edits, rotation, passwords, repair, final writing.
- **lopdf** (MIT, pure Rust) for edits qpdf has no option for: removing
  metadata, replacing images, and building a PDF from images.
- **PDFium** (BSD-3) for rendering only, inside `saqviro-pdf-render`, a small
  helper process.

C and C++ parsers never run inside the app. A crash on a damaged file fails one
job, and cancelling kills a process. This is also why PDFium gets a helper
rather than being loaded into the app, even though it is a library.

## Compress PDF

Neither qpdf nor PDFium re-encodes images, and images are what make PDFs big.

1. qpdf rewrites the file plainly, repairing it and expanding object streams.
2. lopdf finds JPEG and 8-bit RGB or gray images, scales down those with more
   pixels than the largest page needs at the target resolution, and encodes
   them with mozjpeg. An image is replaced only if it gets smaller.
3. qpdf writes the result with object streams and maximum Flate compression.

Masks, CMYK, indexed, JBIG2 and CCITT images stay as they are. Page placement is
not analysed: an image is measured against the whole page, so a small picture
on a page is never over-reduced, at the cost of missing some savings. If the
result is not smaller, the original is returned with a note.

## Passwords

Passwords never appear in a command line, where any process could read them.
They go through a file in the job's private temp directory, as a qpdf `@file`
or `--password-file`. Manifest parameters marked `secret` render as password
fields.

## Consequences

- `scripts/build-qpdf.sh` and `scripts/fetch-pdfium.sh` provide shipped binaries.
- Reordering pages takes typed page numbers until the UI has thumbnails.
- Text and vector content is only compressed losslessly.
