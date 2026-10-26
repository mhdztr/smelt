# Native CSV and TSV Conversion Implementation Plan

> **For agentic workers:** Execute this plan task-by-task while preserving the repository rules in `AGENTS.md`.

**Goal:** Add native CSV and TSV readers and writers as the first M1 slice, using the IDM table shape without introducing a new runtime dependency.

**Architecture:** CSV and TSV share one strict RFC-4180-style delimited parser/serializer. Thin `csv` and `tsv` modules provide the format-specific reader/writer registrations. A delimited document is represented as one IDM `Table`, with the first record as the header and later records as rows.

**Tech Stack:** Rust 2024, existing `smelt-core` IDM and reader/writer traits, existing CLI dispatch and integration-test harness.

---

### Task 1: Add delimited format implementation

**Files:**
- Create: `crates/smelt-core/src/formats/delimited.rs`
- Create: `crates/smelt-core/src/formats/csv.rs`
- Create: `crates/smelt-core/src/formats/tsv.rs`
- Modify: `crates/smelt-core/src/formats/mod.rs`
- Modify: `crates/smelt-core/src/lib.rs`
- Modify: `crates/smelt-detect/src/lib.rs`

- [x] Add strict quoted-field parsing for comma and tab delimiters, preserving UTF-8 field contents and rejecting unterminated quotes or ragged records.
- [x] Map the first record to `Block::Table.header`, later records to `rows`, and use `Align::None` for every column.
- [x] Serialize exactly one IDM table, quoting fields containing delimiters, quotes, newlines, or edge whitespace; return an actionable `WriteError::Serialize` for non-table documents.
- [x] Register `Csv` and `Tsv` native reader/writer dispatch and capability flags.

### Task 2: Add focused roundtrip and CLI coverage

**Files:**
- Modify: `crates/smelt-core/src/formats/delimited.rs`
- Modify: `crates/smelt-cli/tests/cli.rs`

- [x] Cover quoted delimiters, escaped quotes, CRLF input, Unicode and RTL text, empty fields, and malformed/ragged input.
- [x] Cover CSV and TSV document-value roundtrips and conversion of a native delimited file to HTML.

### Task 3: Synchronize specifications

**Files:**
- Modify: `FORMATS.md`
- Modify: `docs/cli.md`
- Modify: `README.md`

- [x] Mark CSV/TSV as implemented native M1 table formats and document the header/strict-shape and lossy rules.
- [x] Document the supported target names and the CSV/TSV writer shape error.
- [x] Update the status line to say M1 is in progress with native CSV/TSV support.

### Task 4: Verify the repository

- [x] Run the narrow core and CLI tests first.
- [x] Run `cargo fmt --check`.
- [x] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Run `cargo test --workspace`.
- [x] Run the two smoke commands from `AGENTS.md`.
