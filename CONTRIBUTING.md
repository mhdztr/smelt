# Contributing to smelt

`smelt` is a local-first document converter CLI. Keep changes small, observable, and honest.

## Read first

1. [`AGENTS.md`](AGENTS.md) — session entry point, crate dependency rules, style rules.
2. [`PLAN.md`](PLAN.md) — milestones and current ship state.
3. [`ARCHITECTURE.md`](ARCHITECTURE.md) — IDM pipeline and crate graph.
4. [`DESIGN.md`](DESIGN.md) — CLI UX spec.
5. The relevant skills under [`.agent/skills/`](.agent/skills/) before writing code.

## Ponytail methodology

- Smallest complete change. Strict YAGNI. Standard library and existing dependencies first.
- One line over fifty. Never add a dependency for a few lines of logic.
- Grep every caller before editing. Fix root causes once in shared code.
- Never cut validation at trust boundaries, data-loss prevention, security checks, or accessibility.
- No `panic!` on user input. Map errors to documented exit codes 0/1/2/3/4 with one-line actionable messages.
- Mark any deliberate deferred edge case with `// shortcut: <the ceiling or limit>, <when to upgrade>`.

## Crate boundaries

1. `smelt-detect` is a leaf: zero dependencies, no crate-internal deps.
2. `smelt-core` depends only on `smelt-detect` and parsing/serialization libraries — never on `smelt-cli` or `smelt-wasm`.
3. `smelt-cli` is the only crate that prints to stdout/stderr, reads terminal env vars, or exits the process.
4. `smelt-wasm` is browser bindings only — never `smelt-formats`, CLI, terminal code, or engine pools.
5. Readers and writers live in `smelt-core`; `smelt-formats` is reserved for heavier engine adapters.

## Adding a reader or writer

1. Implement `Reader` or `Writer` in `crates/smelt-core/src/formats/` (one module per format).
2. Register it in `reader_for()` / `writer_for()` in `crates/smelt-core/src/lib.rs`.
3. Add roundtrip tests comparing `Document` values, not raw text strings. Include a unicode and RTL fixture when the format plausibly carries such content.
4. In the same commit, update `FORMATS.md` (matrix row and lossy cases), and `docs/idm-spec.md` if the IDM changed.
5. CLI behavior changes must update `docs/cli.md` and `DESIGN.md` samples together.

## Verification

Run all of these before proposing a change:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Smoke checks:

```sh
cargo run -q -p smelt-cli -- --help
printf '# Hi\n\n**bold**\n' | cargo run -q -p smelt-cli -- convert - -t html
```

## Style

- No emoji in any file. Sanctioned glyphs: `✓ ✗ ▪ » ─` and box drawing (`│ ┌ ┐ └ ┘`).
- Color only through the `Theme` struct; never unconditional `.green()` at call sites.
- Human byte and duration units come from `ui.rs` helpers.
- Errors are one line and actionable, never a Debug dump.
- For UI, WASM, or localhost GUI work, follow the anti-slop design skills in `.agent/skills/`. Animate only `transform` and `opacity`; support reduced motion.

## Commits

Conventional commits, one logical change each: `feat(cli): ...`, `fix(core): ...`, `docs: ...`, `chore(aur): ...`, `test: ...`. No emoji in messages. Keep docs sync in the same commit as the behavior change.
