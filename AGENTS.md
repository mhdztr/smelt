# AGENTS.md -- session entry point for `smelt`

Read this first in any new session (human or agent). Then `PLAN.md`, then
`ARCHITECTURE.md` for internals and `DESIGN.md` for UX/style rules. Spec
details live in `docs/`.

## What this is

`smelt` -- an advanced document converter CLI ("Melt any document into any
other"). Rust workspace, one self-contained binary (dynamically linked
against glibc today; a fully static musl artifact is planned for M4 release
packaging), N+M converters via the IDM:
Reader -> Intermediate Document Model -> transform -> Writer. External tools
(LibreOffice, pandoc, calibre, groff) are optional accelerators driven by
warm pools, never required. No Python/Node, ever.

## Verify commands (run ALL before claiming done)

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Smoke: `cargo run -q -p smelt-cli -- --help` and
`printf '# Hi\n\n**bold**\n' | cargo run -q -p smelt-cli -- convert - -t html`.

## Repo layout

- `crates/smelt-detect` -- `Format` enum + magic-byte detection. **Zero
  dependencies.** Inline `#[cfg(test)]` tests; no binary fixtures.
- `crates/smelt-core` -- IDM (`src/idm.rs`, spec of record:
  `docs/idm-spec.md`), errors (`src/error.rs`), Reader/Writer traits and
  dispatch (`src/lib.rs`), native converters (`src/formats/`).
- `crates/smelt-cli` -- binary `smelt`: `main.rs` (parse -> dispatch ->
  exit code), `cli.rs` (clap derive), `ui.rs` (Theme, glyphs, color gate,
  human units), `engine.rs` (PATH scan for optional engines),
  `commands/`, `tests/cli.rs`.
- Root docs are specs of record: `DESIGN.md` (UX), `FORMATS.md` (matrix),
  `PLAN.md` (milestones), `docs/cli.md` (command reference),
  `docs/idm-spec.md` (IDM v0).

## Crate dependency rules (violating these = review blocker)

1. `smelt-detect` is a leaf: zero dependencies, no crate-internal deps.
2. `smelt-core` depends only on `smelt-detect` and parsing/serialization
   libraries. It **never** depends on `smelt-cli`.
3. `smelt-cli` is the only crate that prints to stdout/stderr, reads
   environment variables, or exits.
4. Readers/writers live in `smelt-core` (M0); the `smelt-formats` crate
   arrives only in M1 for engine adapters and FFI.

## How to add a reader or writer

1. Implement `Reader` or `Writer` in `crates/smelt-core/src/formats/` (one
   module per format).
2. Register it in `reader_for()` / `writer_for()` in
   `crates/smelt-core/src/lib.rs`.
3. Add roundtrip tests that compare `Document` **values**, not raw text
   strings (see existing tests in `formats/markdown.rs`). Include a unicode
   + RTL fixture when the format plausibly carries such content.
4. In the same commit: update `FORMATS.md` (matrix row + lossy cases) and
   `docs/idm-spec.md` if the IDM changed. Docs-sync rule: CLI behavior
   changes must update `docs/cli.md` and `DESIGN.md` samples together.

## Style rules

- **No emojis in any file**: code, docs, commit messages, PKGBUILDs, help
  text. Sanctioned glyphs only: `✓ ✗ ▪ » ─` and box-drawing (`│ ┌ ┐ └ ┘`).
  The CLI auto-falls back to `[ok] [fail] * > -` on non-unicode terminals
  (`ui.rs`), but the source stays glyph-clean.
- UI: color only through the `Theme` struct; never unconditional `.green()`
  at call sites. Filenames bold, secondary info dim. Human byte/duration
  units from `ui.rs` helpers.
- Errors: one line, actionable, never a Debug dump. No `panic!` on user
  input; `main` maps `SmeltError` to documented exit codes (0/1/2/3/4).

## Commit style

Conventional commits (`feat(cli): ...`, `fix(core): ...`, `docs: ...`,
`chore(aur): ...`, `test: ...`). No emojis in messages. Keep the commit
granularity of the initial sequence: one logical change per commit, docs
sync included.

## Performance guardrails

See `PERFORMANCE.md` for the numbered rules. Headlines: never spawn a
subprocess per file; never block the UI thread on engine I/O; justify every
new dependency against the ~5 ms startup budget.
