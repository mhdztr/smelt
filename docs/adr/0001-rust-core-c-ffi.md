# ADR 0001: Rust core with static C FFI

Status: accepted

## Context

smelt's thesis is speed with zero required runtime: ~5 ms startup, fast
batch conversion, one static binary on Linux/macOS/Windows. Several hot
operations (PDF parse/render, OCR, text shaping, deflate) have mature C
implementations that are dramatically faster and more correct than a from
scratch Rust rewrite would be at this stage.

## Decision

The core is Rust. Where a proven C library is faster, bind to it with
**statically linked** FFI: PDFium (BSD-3) for PDF, Tesseract 5 + Leptonica
for OCR, HarfBuzz/FriBidi for shaping, zlib-ng for compression. Bindings
are hand-written shims confined to the owning crate; no build-time bindgen
(keeps builds reproducible and avoids a clang requirement). Libraries are
feature-gated per platform so a missing toolchain degrades the feature
list, not the build.

## Consequences

- Binaries get larger (PDFium alone is tens of MB before `strip`); acceptable
  against the single-binary promise.
- Build complexity increases; per-platform build scripts arrive when the FFI
  features do (M2/M3), not before.
- License interactions are settled per library in ADR 0005.
- Unsafe code is quarantined: no `unsafe` in `smelt-core` IDM/pipeline code.
