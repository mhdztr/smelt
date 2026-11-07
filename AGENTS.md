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

### Core philosophy: CLI-first, zero remote domain overhead

`smelt` is strictly a local command-line tool. There are no hosted web servers,
no public cloud endpoints, and no remote domain names to manage.
For users who prefer a graphical interface over the terminal, `smelt` supports
an optional local GUI that binds strictly to `localhost` (`127.0.0.1`) and
opens in the local browser. It operates 100% offline and client-side (using
the `smelt-wasm` engine or embedded assets) with zero network exposure.

## Mandatory Skill Hierarchy: Read Skills in `.agent/` FIRST

Every AI agent working on this codebase is **strictly obligated to read the relevant skills in `.agent/skills/` before writing or modifying any code on every single session and prompt**. Do not guess or invent patterns when dedicated domain skills exist in `.agent/`.

### 1. Backend, CLI, and Rust Core: Clean Code Ponytail
Before touching any Rust code (`smelt-core`, `smelt-cli`, `smelt-detect`, `smelt-formats`, `smelt-wasm`), read:
- [`.agent/skills/cleancode-ponytail/ponytail/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail/SKILL.md)
- [`.agent/skills/cleancode-ponytail/ponytail-audit/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail-audit/SKILL.md)
- [`.agent/skills/cleancode-ponytail/ponytail-debt/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail-debt/SKILL.md)
- [`.agent/skills/cleancode-ponytail/ponytail-review/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail-review/SKILL.md)
- [`.agent/skills/cleancode-ponytail/ponytail-gain/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail-gain/SKILL.md)
- [`.agent/skills/cleancode-ponytail/ponytail-help/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/cleancode-ponytail/ponytail-help/SKILL.md)

**Mandatory Ponytail Protocol:**
- **Smallest complete change (YAGNI)**: Skip features, options, wrappers, and speculative abstractions nobody asked for. Standard library and existing dependencies first; one line over fifty. Never add a dependency for a few lines of logic.
- **Strict scope and caller checking**: Grep every caller before editing. Fix root causes once in shared code; never break callers, tests, or fixtures.
- **Never cut non-negotiables**: Validation at trust boundaries, error handling preventing data loss, security checks, and accessibility must never be cut. No `panic!` on user input; map errors to documented exit codes (0/1/2/3/4) with one-line actionable messages.
- **Explicit debt tracking with `shortcut:` comments**: Any deliberate shortcut or deferred edge case MUST be tagged with:
  `// shortcut: <the ceiling or limit>, <when to upgrade>`
  Tracked via `ponytail-debt`.
- **Active review**: Run `ponytail-review` on diffs before declaring done.

### 2. Frontend, Localhost GUI, and Web UI: Anti-Slop High Design
Before touching web, WASM demo, HTML/CSS/JS, or the planned `smelt ui` localhost interface, read:
- [`.agent/skills/frontend-design.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/frontend-design.md) -- Core aesthetic guidelines: bold visual direction, no generic AI slop, no cyan/purple neon gradient text, no glassmorphism abuse, modular type scale, OKLCH perceptually uniform palettes.
- [`.agent/skills/design-taste-frontend/SKILL.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/design-taste-frontend/SKILL.md) -- tasteskill 2.5: professional company/product UI bar (figma.com/x.ai), one section per screen, equal left/right content edges, real product UI renders, quiet micro-interactions, layout-v4 system.
- [`.agent/skills/design-kowalski/README.md`](file:///home/pinefeast/Projects/smelt/.agent/skills/design-kowalski/README.md) -- Emil Kowalski interaction & animation engineering suite:
  - `animate` & `apple-design`: Natural exponential/ease-out curves, fluid physical feel, reduced motion support.
  - `choreograph`: Coordinated transitions (what leads, what follows, what stays static).
  - `animation-performance`: Only animate `transform` and `opacity`; zero layout thrashing; 60fps interaction budgets.
  - `break-ui`: Stress-test UI with worst-case data (empty states, oversized text, long filenames).
- [`.agent/workflows/`](file:///home/pinefeast/Projects/smelt/.agent/workflows/) -- Execute dedicated design workflows when auditing or refining UI (`audit.md`, `polish.md`, `simplify.md`, `harden.md`, `delight.md`).

### Mandatory end-of-turn report
Every response delivering code or documentation changes MUST conclude with:
1. Exactly one single-line ready-to-use **Conventional Commit Message** (`feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`, etc.; strictly one line, NO emojis, scoped when appropriate, e.g., `feat(cli): ...`, `docs(core): ...`).
2. 1-2 lines detailing:
   - What was skipped or not checked.
   - Any risks, limitations, or ceilings the user must know.

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
  dispatch (`src/lib.rs`), native converters (`src/formats/`). Feature-gated:
  `native-full` (default, CLI/native builds) and `wasm` (pure-Rust subset).
- `crates/smelt-cli` -- binary `smelt`: `main.rs` (parse -> dispatch ->
  exit code), `cli.rs` (clap derive), `ui.rs` (Theme, glyphs, color gate,
  human units), `engine.rs` (PATH scan for optional engines),
  `commands/`, `tests/cli.rs`.
- `crates/smelt-wasm` -- browser/WASM bindings over `smelt-core` (no CLI/UI,
  no engine pools, no `smelt-formats`). Ships a static demo under `demo/`.
- Root docs are specs of record: `DESIGN.md` (UX), `FORMATS.md` (matrix),
  `PLAN.md` (milestones), `docs/cli.md` (command reference),
  `docs/idm-spec.md` (IDM v0).

## Crate dependency rules (violating these = review blocker)

1. `smelt-detect` is a leaf: zero dependencies, no crate-internal deps.
2. `smelt-core` depends only on `smelt-detect` and parsing/serialization
   libraries. It **never** depends on `smelt-cli` or `smelt-wasm`.
3. `smelt-cli` is the only crate that prints to stdout/stderr, reads
   environment variables for terminal behavior, or exits the process.
4. `smelt-wasm` is browser bindings only: it depends on `smelt-core`'s
   wasm-safe feature set and **never** depends on `smelt-formats`, `smelt-cli`,
   terminal/UI code, or engine pool orchestration.
5. Readers/writers live in `smelt-core`; `smelt-formats` is reserved for
   heavier engine adapters/FFI if/when added. Format availability is
   feature-gated (`native-full` vs `wasm`) and must fail with actionable
   errors, never panics on user input.

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

## Agent preference

- Default: **Ponytail mode is mandatory** across all prompts and tasks for Rust/CLI code (`.agent/skills/cleancode-ponytail/`). Fast path for well-patterned extensions following existing codebase patterns.
- **Anti-slop frontend standards are mandatory** for any web, WASM, or localhost GUI tasks (`.agent/skills/frontend-design.md`, `.agent/skills/design-taste-frontend/`, `.agent/skills/design-kowalski/`).
- Always read the relevant skills in `.agent/skills/` first.
- Keep changes minimal and focused. Never introduce bloat, speculative layers, or unnecessary dependencies.
- Verify everything against `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
