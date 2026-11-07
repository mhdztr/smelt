# ARCHITECTURE.md -- smelt internals

## Crate graph

```
                 +---------------+        +----------------+
                 |  smelt-cli    |        |  smelt-wasm    |
                 | binary `smelt`|        | browser/WASM   |
                 | (prints/IO)   |        | bindings only  |
                 +-------+-------+        +-------+--------+
                         |                        |
                         v                        v
                 +-----------------------------------------+
                 |              smelt-core                 |
                 | IDM, Reader/Writer traits, native      |
                 | converters; feature-gated for native    |
                 | (`native-full`) and wasm (`wasm`)       |
                 +-------------------+---------------------+
                                     |
                                     v
                             +---------------+
                             | smelt-detect  |  Format enum, magic-byte detection
                             | (zero deps)   |
                             +---------------+
```

Dependency direction is strictly downward: `smelt-detect <- smelt-core
<- smelt-cli` and `smelt-detect <- smelt-core <- smelt-wasm`. Readers and
writers live in `smelt-core`; `smelt-core` never depends on `smelt-cli` or
`smelt-wasm`.

`smelt-detect` is a zero-dependency leaf so the CLI and WASM bindings can
report format information without unnecessary coupling. `Format` is defined
in `smelt-detect` and re-exported by `smelt-core`.

`smelt-formats` is deferred for heavier engine adapters/FFI; native format
implementations live in `smelt-core/src/formats/` and are feature-gated.

## WASM bindings and Localhost GUI (M5)

`smelt-wasm` is a browser binding layer over `smelt-core`. It must not depend
on `smelt-formats`, the CLI, terminal/UI code, or engine pool orchestration.
It uses `smelt-core` with `default-features = false, features = ["wasm"]` so
the wasm build excludes syntect/onig, rayon, PDF, EPUB, and other non-wasm-safe
or heavyweight dependencies. The v1 wasm surface exposes version, supported
formats, detection, and byte/text conversion with actionable errors and no
panics on user input.

### Localhost GUI architecture

`smelt` is strictly a local tool with **zero remote server or domain dependencies**.
There is no hosted web server, no public cloud conversion service, and no remote
infrastructure.

For users who cannot or prefer not to use the command line, the architecture
provides a local graphical interface:
- Initiated directly from the CLI (e.g. `smelt ui` / `--gui`).
- Binds strictly to `localhost` (`127.0.0.1`) on an ephemeral or designated local port.
- Opens the system's default browser directly to `http://127.0.0.1:<port>`.
- Executes entirely offline and client-side (via `smelt-wasm` or embedded local static assets).
- Closes cleanly when the user terminates the CLI process (`Ctrl+C`).
- Adheres to frontend and interaction design standards in `.agent/skills/`
  (`frontend-design.md`, `design-taste-frontend`, and `design-kowalski`).

## Ponytail architectural guardrails

All Rust modules in `smelt` must adhere to the Ponytail methodology
(`.agent/skills/cleancode-ponytail/ponytail`):
1. **YAGNI (You Aren't Gonna Need It)**: Do not create multi-layered abstractions,
   generic factories, or speculative plugin hooks before a second real caller requires them.
2. **Minimal dependency footprint**: Dependencies directly impact the ~5 ms
   startup budget. Use standard library types and functions whenever possible.
3. **Explicit debt markers**: Any simplified implementation or deferred edge case
   must carry `// shortcut: <limit>, <trigger>` for tracking via `ponytail-debt`.
4. **Resilient error paths**: Never panic on invalid user data. Bubble errors cleanly
   through `SmeltError` to documented exit codes.

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

## FFI and native dependencies

The original design targeted static FFI for hot paths (PDFium, Tesseract,
HarfBuzz/FriBidi, zlib-ng). As of 2.0.0, `smelt-core` implements PDF/EPUB
using Rust crates (`pdf`/`lopdf`, `zip`) behind feature gates rather than
PDFium FFI, and OCR uses the external `tesseract` CLI (basic) rather than
static Tesseract/Leptonica bindings. No hand-written C FFI shims are required
for the shipped 2.0.0 core conversion paths.

The wasm feature set excludes PDF/EPUB and C-heavy dependencies. Linux release
binaries are dynamically linked against glibc (libc, libm, libgcc_s) in
2.0.0; fully static musl artifacts remain future packaging work. Any future
FFI/static-linking work must remain feature-gated per platform and keep the
zero-required-runtime/native build constraints explicit.

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
