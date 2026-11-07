# PLAN.md -- smelt master plan

`smelt` -- melt any document into any other. This is the full project plan.
**M5 (ship) is complete locally**: workspace version `2.0.0`, usable
`smelt-wasm` browser bindings, a static client-side demo, and CI/release
artifacts for Linux x86_64/aarch64 + WASM. See the M5 section for the ship
record. Historical milestone records (M0-M4 targets) are preserved below.

## Design thesis

1. **N+M, not N x M.** Every converter suite pays N x M: each of N readers is
   wired to each of M writers. smelt routes everything through an
   Intermediate Document Model (IDM, [docs/idm-spec.md](docs/idm-spec.md)):
   a normalized AST of blocks, runs, styles, tables, images, links, and
   metadata. A reader costs one IDM producer; a writer costs one IDM
   consumer. The IDM also unlocks features flat converters cannot offer:
   semantic diff, fidelity reporting, and content-hash caching.
2. **Warm pools, never per-file subprocesses.** External engines (LibreOffice,
   pandoc, calibre, groff) are driven by persistent worker pools that start
   once and serve a request queue. Spawning `soffice` per file costs seconds;
   a warm pool costs milliseconds. See
   [ARCHITECTURE.md](ARCHITECTURE.md) and ADR 0003.
3. **Static FFI where it is faster.** Hot paths (PDF read, OCR, text shaping,
   compression) use C libraries statically linked via FFI: PDFium (BSD-3),
   Tesseract 5 + Leptonica, HarfBuzz/FriBidi, zlib-ng. See ADR 0001.
4. **Zero required runtime.** One self-contained binary works on Linux,
   macOS, and Windows. External tools are optional, detected at runtime,
   reported by `smelt doctor`. No Python, no Node, ever. See ADR 0004.
   Note: the M0 Linux build is dynamically linked against glibc (libc,
   libm, libgcc_s); a fully static musl artifact is M4 release packaging
   work, not a claim about today's binary.
5. **CLI-first, zero remote domain footprint.** `smelt` is strictly a local
   tool. There are no hosted web servers, no public cloud endpoints, and no
   remote domain configuration to manage. For users who prefer a graphical
   interface over terminal commands, the CLI provides an optional command
   (`smelt ui`) to serve a lightweight local UI strictly on `localhost`
   (`127.0.0.1`) and launch the default browser. Everything runs offline
   and client-side (powered by `smelt-wasm` or embedded assets) with zero
   network exposure.
6. **Ponytail lazy senior dev engineering.** The project strictly adheres
   to the Ponytail methodology (`.agent/skills/cleancode-ponytail/ponytail`):
   build the smallest change that completely works, enforce strict YAGNI
   (skip speculative features, wrappers, and unused abstractions), prioritize
   stdlib and existing dependencies, never break callers or tests, preserve
   the ~5 ms startup budget, and track intentional simplifications with
   `shortcut: <ceiling>, <trigger>` comments.
7. **Anti-slop frontend and interaction design.** For any graphical UI
   (`smelt ui`, `demo/`, `smelt-wasm`), the project enforces high design
   standards from `.agent/skills/`:
   - Anti-AI slop aesthetics (`frontend-design.md`).
   - Professional product layout and clean copy (`design-taste-frontend`).
   - Fluid physical motion and 60fps interaction engineering (`design-kowalski`).

## Milestones

### M0 -- foundation (complete)

Native Rust conversion for text formats, detection, CLI core.

- Cargo workspace: `smelt-detect` (magic-byte detection), `smelt-core` (IDM,
  reader/writer traits, native converters), `smelt-cli` (binary).
- Format detection by magic bytes with extension tiebreak.
- IDM v0: headings, paragraphs, lists, tables, code blocks, images, rules,
  blockquotes; inline text/code/strong/emph/strikethrough/links; title and
  author metadata.
- Native conversions: **md <-> html <-> txt** round-trip, no external tools.
- CLI: `convert` (alias `c`; single file, directory batch with rayon
  parallelism, stdin/stdout streaming), `info` (detection + stats, `--json`),
  `doctor` (engine report), `completions`.
- Acceptance: `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace` all green; README quickstart works
  verbatim on a fresh clone; exit codes 0/1/2/3/4 as documented in
  [docs/cli.md](docs/cli.md).

### M1 -- office formats via warm LibreOffice pool

Progress: native CSV/TSV table conversion is implemented; the remaining M1
work is the warm LibreOffice pool, recursive batch traversal, EPUB, and the
content-hash cache.

- Persistent headless LibreOffice pool (request queue, health checks, idle
  shutdown); `smelt doctor` validates pool preconditions.
- docx/odt/pptx/xlsx <-> IDM via soffice; CSV/TSV readers/writers natively;
  native EPUB reader/writer.
- Content-hash conversion cache keyed by (input hash, direction, engine
  version).
- Acceptance: batch of 100 docx -> pdf at >= 5x LibreOffice-per-file
  ([PERFORMANCE.md](PERFORMANCE.md)); cache hits skip engine time entirely;
  warm-pool failures produce the actionable error format from DESIGN.md.

### M2 -- PDF

- PDFium (static FFI) PDF reader -> IDM; PDF writer; searchable-PDF output
  (text layer injection).
- Page-parallel processing with rayon.
- Acceptance: pdf -> md quality gate on a fixture corpus; parallel speedup
  measured and >= 3x on 8+ pages; PDFium stays BSD-3 compatible (ADR 0005).

### M3 -- long tail

- pandoc pool for latex/man-troff and md edge cases; calibre pool for
  mobi/azw; Tesseract 5 + Leptonica static OCR for images and scanned PDFs;
  groff for man/troff.
- `watch` (directory watching + incremental convert), `diff` (semantic diff
  on the IDM), `ocr` subcommand.
- Acceptance: each engine optional -- smelt with none of them installed still
  passes the full test suite; OCR fixture pass; `watch` reacts to file
  changes within 1 s.

### M4 -- polish and GA

- Embedded Lua filters (mlua, vendored) for user-defined IDM transforms.
- Shell completion polish, `man` subcommand (clap_mangen), config file.
- Packaging GA: AUR packages published, release tarballs with static
  binaries for Linux x86_64/aarch64.
- Acceptance: Lua filter example runs; `smelt man` emits roff; AUR packages
  build on a clean chroot.

Status (honest): `smelt man` emits roff via `clap_mangen` and is shipped.
Lua filters, config file, and full AUR publication are not completed in M5;
AUR PKGBUILDs are versioned for 2.0.0 with placeholder checksums pending
release asset publication.

### M5 -- ship (complete locally)

Goal: ship a usable 2.0 release with browser bindings, a static demo, and
release artifacts -- no backend conversion server.

- **`smelt-core` feature gates**: `default = ["native-full"]` preserves the
  native CLI/core format set (markdown/html/text/csv/tsv/epub/pdf + rayon).
  `wasm = ["markdown","html","text","csv","tsv"]` is a pure-Rust subset with
  no syntect/onig/rayon/pdf/epub C or heavyweight deps. `comrak` uses
  `default-features = false`. `smelt-cli` depends on `smelt-core` with
  `native-full`; `smelt-wasm` uses `default-features = false, features = ["wasm"]`.
- **`smelt-wasm`**: browser bindings over `smelt-core` (not `smelt-formats`,
  not the CLI, not pools). Exposes `version()`, `supported_formats()`,
  `detect_format(...)`, `convert(...)`, and `convert_text(...)`. Applies CLI
  plain-text parity (detected Text read via Markdown reader). Unsupported
  formats and bad inputs return actionable JS errors; no panics on user input.
  v1 wasm formats: md/html/txt/csv/tsv.
- **Static demo and local UI**: `demo/index.html` + `demo/main.js` + `demo/README.md`.
  Client-side only; no backend conversion server. ES modules require static
  HTTP serving locally:
  `wasm-pack build crates/smelt-wasm --target web --out-dir ../../demo/pkg --release`
  then `python3 -m http.server --directory demo 8000` and open
  `http://localhost:8000`. Serves as the foundation for the local `smelt ui`
  localhost-only GUI command for non-terminal users (no remote domains).
- **CI**: fmt, clippy, workspace tests, `cargo check -p smelt-wasm --target wasm32-unknown-unknown`,
  and a wasm-pack smoke build (web target) on push/PR.
- **Release (`v*` tags)**: publishes `smelt-x86_64-unknown-linux-gnu.tar.gz`,
  `smelt-aarch64-unknown-linux-gnu.tar.gz` (built via `cross`), and
  `smelt-wasm-web-pkg.tar.gz` to the GitHub release.
- **Packaging/docs**: AUR PKGBUILDs and AUR runbook bumped to 2.0.0; checksums
  remain placeholders until release assets are published. `smelt-bin` remains
  x86_64-only in packaging while aarch64 release artifacts are published.

Acceptance (local): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and `cargo check -p smelt-wasm --target wasm32-unknown-unknown`
green. Local tag `v2.0.0` created after verification; not pushed.

## Format matrix

See [FORMATS.md](FORMATS.md) for the authoritative table (format, read,
write, engine, milestone, notes) and the lossy-case list.

## Risk register

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Engine version drift (soffice/pandoc output changes) | Conversion regressions | Pin expectations by version probe in doctor; cache keyed by engine version; fixture-based roundtrip tests on Document values, not raw text |
| Windows console (Unicode, colors, no PTY) | Broken UI on Windows | ui.rs logic is std-only (`IsTerminal`); ASCII fallback; NO_COLOR; best-effort documented, tested on Linux/macOS only until CI exists |
| FFI licensing (static linking) | Distribution risk | PDFium is BSD-3 (OK); Tesseract is Apache-2.0, statically compatible with MIT distribution with attribution -- see ADR 0005 |
| comrak GFM edge cases (nested table cells, setext headings) | Roundtrip loss | Document-equality roundtrip tests instead of string snapshots; known gaps listed in FORMATS.md |
| html5ever silently repairs malformed HTML | Perceived data loss | Documented as lossy; fidelity reporting in a later milestone |
| Startup budget creep from dependencies | Loses the ~5 ms thesis | Perf guardrail 3 in PERFORMANCE.md: every new dep justified against the budget |

## M0 acceptance record

Verified green on the M0 completion commit: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `cargo build --release`, every README quickstart
command verbatim, and exit codes 0/1/2/3/4 as documented in
[docs/cli.md](docs/cli.md).

Not in M0: watch, diff, ocr, man subcommand, Lua filters, smelt-formats
crate, any external-engine invocation, CI config, publishing to a remote.
