# DESIGN.md -- CLI UX and document style spec

This is the spec of record for how `smelt` looks and behaves on a terminal
and how this repository's documents are written. UI code lives in
`crates/smelt-cli/src/ui.rs` (a `Theme` struct); call sites never apply
color unconditionally.

## Doc style rules (this repository)

1. **No emojis in any file**: code, docs, commit messages, PKGBUILDs,
   completions, help text. Not one. The rationale: output must be safe in
   every terminal, pipe, and AUR page, and emoji are noise in a tool whose
   job is precise text.
2. The only sanctioned non-ASCII glyphs are `✓`, `✗`, `▪`, `»`, `─` and
   box-drawing characters (`│ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`). Non-unicode terminals get
   the ASCII fallbacks below, auto-detected.
3. Everything else is plain ASCII.

## Glyph table

| Meaning | Unicode | ASCII fallback |
| --- | --- | --- |
| success / found / ok | `✓` | `[ok]` |
| failure / missing / fail | `✗` | `[fail]` |
| bullet | `▪` | `*` |
| separator (list-ish) | `»` | `>` |
| horizontal rule / line | `─` | `-` |
| box corners/edges | `┌ ┐ └ ┘ │` | `+`, `-`, `\|` |

Unicode mode (`ui::unicode_ok()`) requires ALL of:

- stdout or stderr is a terminal (`std::io::IsTerminal`),
- `TERM` is not `linux` (the Linux console has poor Unicode coverage),
- the first defined of `LC_ALL`, `LC_CTYPE`, `LANG` contains `utf-8`/`utf8`
  (case-insensitive),
- `SMELT_ASCII` is unset (setting `SMELT_ASCII=1` forces ASCII).

`--color=always` does **not** force unicode; `--color` controls color only.

## Color

- Library: owo-colors, applied only through `Theme { on: bool, .. }`.
- Precedence, highest first: `--color=never` > `NO_COLOR` (any value,
  including empty) > `--color=always` > TTY autodetect.
- `CLICOLOR_FORCE` is **not** supported (documented in docs/cli.md).
- Styling: **filenames bold**, secondary info dim (sizes, timings, evidence,
  paths in the doctor table). Success green `✓`, failure red `✗`.

## Human units

- Bytes: `B`, `KB`, `MB`, `GB` -- decimal (1 KB = 1000 B), integers below
  MB, one decimal place for MB and up. Examples: `512 B`, `3.1 KB`, `2.1 MB`.
- Durations: `120 ms` below one second, then `1.4 s`, `2 m 05 s`.

## Output samples (normative)

Single-file success, printed to stderr unless `-q` (dim trailing detail):

```
a.md -> a.html (3.1 KB, 4 ms)
```

Errors: one line, actionable, prefixed `✗` / `[fail]`, never a Debug dump:

```
✗ failed: a.docx -> pdf (no native reader for docx in v0; run 'smelt doctor' for available engines)
✗ failed: soffice pool: no workers. run 'smelt doctor'
✗ no such file: /nonexistent.md
```

Batch summary box (end of every directory run):

```
┌───────────────────────────────┐
│ converted   12 files          │
│ ok         11                 │
│ failed      1                 │
│ elapsed     1.4 s             │
└───────────────────────────────┘
```

Alignment: label column left-aligned and padded to the widest label;
values left-aligned in the value column; box padded to content width.
ASCII fallback uses `-`, `|`, `+` in the same layout.

`smelt doctor` table (aligned columns, dim paths, optional-engine misses as
`✗ ... (optional)` so they never read as errors; the version banner is
whatever the engine reports):

```
✓ soffice       /usr/bin/soffice (<version banner>)
✓ tesseract     /usr/bin/tesseract (<version banner>)
✗ pandoc        not found (optional)

smelt M0 uses no external engines; these are reported for future milestones.
```

`smelt info` human output:

```
path      README.md
format    Markdown (guessed: extension)
size      3.4 KB
lines     120
words     640
bytes     3481
```

`smelt info --json` prints one JSON object:
`{"path","format","confidence","evidence","size_bytes","lines","words","bytes"}`
with `null` for fields that do not apply.

## Progress policy

- TTY stderr: `indicatif::MultiProgress` with one bar per file, `[n/N] name`
  style.
- Non-TTY stderr (pipes, logs): plain lines,
  `converting a.md -> a.html ... ok` / `... failed (reason)`. Plain lines
  are also used under `--color=never`.
- `-q` suppresses progress and success lines; errors always print.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | partial failure (batch: at least one ok, at least one failed) |
| 2 | usage error (bad flags, missing `-t` where required, output path equals input path) |
| 3 | unsupported conversion (no reader/writer for a format) |
| 4 | missing input path |

Implementation: `main` returns `ExitCode` derived from a `SmeltError`
carrying the code; user input never triggers a panic.

## Pipes

A closed reader (for example `smelt doctor | head -1`) must not produce a
panic dump or a broken-pipe message. The process restores the default
`SIGPIPE` disposition at startup (`main::restore_sigpipe_default`), so it
dies quietly on the signal like `cat` or `grep`. Output is written with
`println!`/`eprintln!` and is never routed through a custom error path on
`EPIPE`.
