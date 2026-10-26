# ARCHITECTURE.md -- smelt internals

## Crate graph

```
                 +---------------+
                 |  smelt-cli    |  binary `smelt`: clap CLI, UI, pools
                 | (only crate   |  (dispatch, commands, ui.rs, engine.rs)
                 |  that prints) |
                 +-------+-------+
                         |
                         v
                 +---------------+
                 |  smelt-core   |  IDM, Reader/Writer traits, native
                  |               |  converters (md, html, txt in M0;
                  |               |  csv, tsv in M1)
                 +-------+-------+
                         |
                         v
                 +---------------+
                 | smelt-detect  |  Format enum, magic-byte detection
                 | (zero deps)   |
                 +---------------+
```

Dependency direction is strictly downward: `smelt-detect <- smelt-core
<- smelt-cli`. Readers and writers depend only on `smelt-core`;
`smelt-core` never depends on `smelt-cli`.

`smelt-detect` is a zero-dependency leaf so the CLI can report format
information without linking the conversion stack. `Format` is defined in
`smelt-detect` and re-exported by `smelt-core`.

A fourth crate, `smelt-formats`, is deferred to M1: it appears at the first
point where LibreOffice/pandoc/calibre adapters and C FFI bindings make an
extra layer worth its rent. In M0 the pure-Rust md/html/txt converters live
in `smelt-core/src/formats/` because they share the IDM directly.

## Data flow

```
input bytes --detect--> Format
     |
     v
 Reader (per Format) --parse--> IDM Document
     |
     v
 transform pipeline (identity in M0; Lua filters, fidelity scoring, cache
                     lookups arrive M1-M4)
     |
     v
 Writer (per Format) --serialize--> output bytes
```

- `Reader::read(&self, input: &[u8], fmt: Format) -> Result<Document, ReadError>`
- `Writer::write(&self, doc: &Document, fmt: Format) -> Result<Vec<u8>, WriteError>`
- Dispatch: `reader_for(Format) -> Option<&'static dyn Reader>`,
  `writer_for(Format) -> Option<&'static dyn Writer>`. `None` means the
  format has no native implementation in this version -- the CLI maps that
  to exit code 3 with an actionable message (and, later, tries an engine
  pool before giving up).

## The IDM's role

The Intermediate Document Model ([docs/idm-spec.md](docs/idm-spec.md)) is
the project's central asset:

- **N+M converters** as described in PLAN.md.
- **Semantic diff** (M3): diff two IDM values structurally instead of
  diffing rendered bytes.
- **Fidelity reporting** (M1+): count constructs lost or degraded by a
  roundtrip by comparing the source IDM with the re-parsed output IDM.
- **Cache** (M1): the serde representation of an IDM document is the cache
  payload; the cache key is (content hash, direction, engine version).

## Warm external engine pools (M1, designed now)

Never spawn an external tool per file (PERFORMANCE.md guardrail 1).

- A pool owns N persistent headless worker processes (e.g. LibreOffice
  `soffice --headless --nofirststartwizard` instances holding a listening
  socket).
- Conversion requests go through a bounded queue; a worker claims a request,
  converts, returns bytes over the control channel.
- Health check: on pool start and after K consecutive failures a worker is
  recycled. `smelt doctor` runs the same health check as a precondition
  report and is the only place version probes run (5 s timeout, degrade to
  path-only on failure).
- Idle workers shut down after a grace period so `smelt` never leaves
  strays behind.
- Failure surfaces as an actionable error, e.g.
  `failed: a.docx -> pdf (soffice pool: no workers. run 'smelt doctor')`.

## FFI strategy

Hot-path C libraries are statically linked, feature-gated per platform:

- **PDFium** (BSD-3) -- PDF read/write, M2.
- **Tesseract 5 + Leptonica** (Apache-2.0) -- OCR, M3.
- **HarfBuzz / FriBidi** -- text shaping/RTL, post-M0 (M0 preserves RTL
  content but does not shape).
- **zlib-ng** -- fast deflate for OOXML/zip internals when needed.

None of these are linked yet, so the M0 binary contains no FFI at all. The
M0 Linux binary is dynamically linked against glibc (libc, libm, libgcc_s);
a fully static musl artifact is M4 release packaging work.

Bindings are hand-written `unsafe` shims confined to the crate that owns the
library; no bindgen at build time (reproducible builds, no clang
requirement). Static linking keeps the zero-required-runtime promise
(ADR 0004); licensing is settled in ADR 0005.

## Threading model

- **rayon** for batch conversion (one task per file) and, in M2, for
  page-parallel PDF work inside a single file.
- **The UI/progress thread never blocks on engine I/O** (guardrail 2):
  workers send progress events to the UI through channels; indicatif's
  MultiProgress consumes them.
- Engine pools have their own I/O threads; request/response is async over
  channels, but no async runtime is imposed on the rest of the codebase.
- IDM and Format are plain owned data (`Send + Sync`) so any work item can
  move across threads freely.
