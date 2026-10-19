# docs/cli.md -- smelt command reference

Spec of record for the CLI surface. Commands marked *planned* are designed
here but not yet implemented (see PLAN.md for their milestone). Anything not
marked is implemented in M0.

## Global shape

```
smelt [OPTIONS] <COMMAND>
```

| Option | Values | Default | Notes |
| --- | --- | --- | --- |
| `--color` | `auto`, `always`, `never` | `auto` | global; accepted on every subcommand. Precedence over `NO_COLOR` is documented in DESIGN.md |
| `-h`, `--help` | | | standard clap help; every subcommand shows real examples |
| `-V`, `--version` | | | |

Environment variables:

| Variable | Effect |
| --- | --- |
| `NO_COLOR` | any value disables color (https://no-color.org convention) |
| `SMELT_ASCII` | any value forces ASCII glyphs (`[ok]`, `[fail]`, ...) |
| `LC_ALL` / `LC_CTYPE` / `LANG` | first defined one must contain `utf-8` for Unicode glyphs on a TTY |
| `TERM` | `linux` disables Unicode glyphs (poor console coverage) |
| `CLICOLOR_FORCE` | **not supported** -- use `--color=always` |

Exit codes (documented, stable):

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | partial failure (batch: at least one succeeded, at least one failed) |
| 2 | usage error (bad flags, missing `-t` where required) |
| 3 | unsupported conversion (no reader or writer for a format) |
| 4 | missing input path |

## `smelt convert` (alias `c`)

```
smelt convert [OPTIONS] [INPUT]
smelt c [OPTIONS] [INPUT]
```

Convert files, directories, or stdin. One input per invocation; for many
inputs use a directory (batch mode).

| Flag | Meaning |
| --- | --- |
| `[INPUT]` | file, directory, or `-` for stdin. Default: `-` when stdin is not a terminal, else usage error |
| `-t`, `--to <FMT>` | target format name (`md`, `html`, `txt`, ...). Required for stdin and directory input |
| `-o`, `--output <PATH>` | output file, directory, or `-` for stdout |
| `-q`, `--quiet` | suppress progress, per-file success lines, and the summary box; errors still print |
| `-v`, `--verbose` | extra detail (detection evidence per file in batch mode) |
| `--color <WHEN>` | see global options |

### Output-format resolution

| INPUT | `-t` | `-o` | Result |
| --- | --- | --- | --- |
| file | given | any | convert to `-t` |
| file | absent | file path with known extension | infer format from `-o` extension |
| file | absent | absent | **error, exit 2**: "pass -t/--to or -o/--output to choose the output format" |
| `-` (stdin) | given | absent or `-` | convert stdin to stdout |
| `-` (stdin) | given | file path | convert stdin to file |
| `-` (stdin) | absent | any | **error, exit 2**: stdin requires `-t` |
| directory | given | absent or `-` | **error, exit 2**: stdout forbids directory input |
| directory | given | directory path | batch convert; the directory is created if missing |
| directory | absent | any | **error, exit 2**: directory input requires `-t` |

An inferred same-format pair (md -> md) is allowed and acts as a
normalization pass.

### Batch mode

Directory input converts every regular file directly inside it
(**non-recursive in M0**; `-r/--recursive` is planned). Hidden files
(dot-prefixed) and subdirectories are skipped. Each file is converted in
parallel (rayon). Output files keep their stem and get the target
extension: `notes.2026.md` -> `notes.2026.html`.

Progress: on a TTY, one progress bar per file inside a multi-progress
group; on a non-TTY (and under `--color=never`), plain lines:

```
converting a.md -> a.html ... ok
converting b.md -> b.html ... failed (no native reader for docx in v0; run 'smelt doctor' for available engines)
```

End of run, a summary box (unless `-q`):

```
┌───────────────────────────────┐
│ converted   12 files          │
│ ok         11                 │
│ failed      1                 │
│ elapsed     1.4 s             │
└───────────────────────────────┘
```

Batch exit codes: 0 all ok; 1 partial failure; 3 every file failed and every
failure was an unsupported conversion; 4 input directory missing.

### Errors

One line, actionable, exit code matched to cause:

```
✗ failed: a.docx -> pdf (no native reader for docx in v0; run 'smelt doctor' for available engines)
✗ no such file: /nonexistent.md        (exit 4)
```

## `smelt info`

```
smelt info [OPTIONS] [FILE]
```

| Flag | Meaning |
| --- | --- |
| `[FILE]` | file path or `-` for stdin; defaults to stdin when piped |
| `--json` | machine-readable output |
| `--color <WHEN>` | see global options |

Human output:

```
path      README.md
format    Markdown (guessed: extension)
size      3.4 KB
lines     120
words     640
bytes     3481
```

`--json` object: `{"path": ..., "format": ..., "confidence": ...,
"evidence": ..., "size_bytes": ..., "lines": ..., "words": ...,
"bytes": ...}` -- `lines`/`words`/`bytes` are `null` for non-text formats,
`size_bytes` is `null` for stdin (unknown until read). Exit codes: 0 ok,
4 missing file.

## `smelt doctor`

```
smelt doctor
```

Reports which optional external engines are on PATH (`soffice`,
`tesseract`, `pandoc` in M0), each with a `--version` probe (5 s timeout,
degrades to path-only when the probe fails or times out):

```
✓ soffice       /usr/bin/soffice (LibreOffice 26.8.1.1)
✓ tesseract     /usr/bin/tesseract (tesseract 5.5.0)
✗ pandoc        not found (optional)

smelt M0 uses no external engines; these are reported for future milestones.
```

Always exits 0. Never runs on the convert path (PERFORMANCE.md guardrail 1).

## `smelt completions`

```
smelt completions <SHELL>
```

`SHELL` is one of `bash`, `zsh`, `fish`, `powershell`; prints a completion
script to stdout (pipe into the shell's completion dir, e.g.
`smelt completions bash > /etc/bash_completion.d/smelt`).

## Planned commands (M1+, no code yet)

| Command | Milestone | Purpose |
| --- | --- | --- |
| `smelt watch` | M3 | watch a directory and convert on change (`-r` recursive flag arrives with batch mode improvements in M1) |
| `smelt diff` | M3 | semantic diff of two documents via the IDM |
| `smelt ocr` | M3 | OCR images and scanned PDFs via Tesseract |
| `smelt man` | M4 | emit the man page (clap_mangen) |
| `smelt convert -r` | M1 | recursive batch traversal |
