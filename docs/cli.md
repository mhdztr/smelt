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
| 1 | partial failure (batch: at least one succeeded, at least one failed); also input read errors other than "not found" (e.g. permission denied) |
| 2 | usage error (bad flags, missing `-t` where required, batch output-name collision, output path equals the input path -- including a hardlink alias on Unix) |
| 3 | unsupported conversion (no reader or writer for a format) |
| 4 | missing input path (the path does not exist) |

Only a genuinely absent input is exit 4. A path that exists but cannot be
opened reports the OS reason at exit 1, because claiming "no such file" for
an unreadable file is actively misleading:

```
$ smelt convert secret.md -t html
✗ cannot read secret.md: permission denied     (exit 1)

$ smelt convert /nonexistent.md -t html
✗ no such file: /nonexistent.md                (exit 4)
```

### Pipes

`smelt` writes to stdout/stderr and exits quietly when the reader end of a
pipe closes early. `smelt doctor | head -1` terminates with no error output;
it never prints a panic dump or a broken-pipe message. This is the standard
Unix filter behavior (the process restores the default `SIGPIPE`
disposition at startup), so `smelt` composes safely with `head`, `grep`,
`less`, and shell pipelines.

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
| `-t`, `--to <FMT>` | target format name (`md`, `html`, `txt`, `csv`, `tsv`, ...). Required for stdin and directory input |
| `-o`, `--output <PATH>` | output file, or directory in batch mode; `-` for stdout |
| `-q`, `--quiet` | suppress progress, per-file success lines, and the summary box; errors still print |
| `-v`, `--verbose` | extra detail (detection evidence per file in batch mode) |
| `--color <WHEN>` | see global options |

### Output-format resolution

| INPUT | `-t` | `-o` | Result |
| --- | --- | --- | --- |
| file | given | file path or `-` | convert to `-t`; a directory path is batch-only and fails at write with exit 1 |
| file | absent | file path with known extension | infer format from `-o` extension |
| file | absent | file path with unknown or no extension | **error, exit 2**: "pass -t/--to or -o/--output to choose the output format: cannot infer a format from `<path>`" |
| file | absent | absent | **error, exit 2**: "pass -t/--to or -o/--output to choose the output format" |
| `-` (stdin) | given | absent or `-` | convert stdin to stdout |
| `-` (stdin) | given | file path | convert stdin to file |
| `-` (stdin) | absent | any | **error, exit 2**: stdin requires `-t` |
| directory | given | absent or `-` | **error, exit 2**: stdout forbids directory input |
| directory | given | directory path | batch convert; the directory is created if missing |
| directory | absent | any | **error, exit 2**: directory input requires `-t` |

An inferred same-format pair (md -> md) is allowed and acts as a
normalization pass only when the output path differs from the input. A
resolved output path that equals the input path is refused with **exit 2**
before anything is written, because rewriting the source through the IDM
drops what the model does not carry (classes, inline styles, original
structure). On Unix a hardlink alias of the input counts as the input
(compared by device and inode), so an output that shares the source inode is
refused too:

```
$ smelt convert a.html -t html
✗ refusing to overwrite the input file in place: a.html; pass -o with a distinct output path or a different -t

$ smelt convert a.html -t html -o normalized.html    # allowed
```

There is no `--force` in M0; `-o` with a distinct path (or a different `-t`)
is the explicit opt-in.

### Plain-text source-format rule (stdin and files)

When detection lands on plain `Text` -- which happens for any valid-UTF-8
input with no markup and no revealing extension -- the **Markdown reader**
is used, on **both** channels. Markdown is a superset of plain text, so
piping `**bold**` or `# Hi` through a pipe converts properly instead of
coming out verbatim, and a `.txt` file with the same bytes converts to the
same result:

```
$ printf '**bold**\n' | smelt convert - -t html
<p><strong>bold</strong></p>

$ printf '# Hi\n' | smelt convert - -t html
<h1>Hi</h1>

$ printf 'a **b** c\n' > p.txt
$ smelt convert p.txt -t html
<p>a <strong>b</strong> c</p>
```

Consequence: `-t txt` output is **lossy for markup**, identically on both
channels. Emphasis markers are parsed into the IDM and the plain-text
writer emits the text without them, exactly as an HTML-to-text conversion
drops tags. Content is never dropped, but markup is:

```
$ printf 'a **b** c\n' | smelt convert - -t txt
a b c

$ smelt convert p.txt -t txt -o out.txt && cat out.txt
a b c
```

Before M0 review the two channels disagreed here: stdin was read as
Markdown (dropping the markers) while a `.txt` file was read by the
plain-text reader (keeping them verbatim), so the same bytes produced
different output depending on how they arrived. Both channels now share one
source-format rule.

### Batch mode

Directory input converts every regular file directly inside it
(**non-recursive in M0**; `-r/--recursive` is planned). Hidden files
(dot-prefixed) and subdirectories are skipped. Each file is converted in
parallel (rayon). Output files keep their stem and get the target
extension: `notes.2026.md` -> `notes.2026.html`.

Two inputs whose stems collide (`notes.md` and `notes.html`, both mapping
to `notes.txt`) would map to the **same** output file. Because the writes
run in parallel, continuing would leave a nondeterministic winner and exit
0, silently discarding one result. This is refused before anything is
written: **error, exit 2**, naming both inputs and the shared output path.
The same check refuses a batch output that would overwrite its own input
(for example `smelt convert docs/ -t md -o docs/`): every one of those
outputs is resolved to the input path, so the run exits 2 before writing
anything.

```
$ smelt convert docs/ -t txt -o out
✗ batch output collision: docs/notes.html and docs/notes.md both map to out/notes.txt; rename one input or convert them separately
```

Progress: on a TTY, one progress bar per file inside a multi-progress
group; on a non-TTY (and under `--color=never`), plain lines:

```
converting a.md -> a.html ... ok (0 ms)
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
✗ no native writer for pdf in v0; run 'smelt doctor' for available engines
✗ no such file: /nonexistent.md        (exit 4)
```

### CSV and TSV table shape

The native CSV and TSV readers treat the first record as the IDM table header
and require every later record to have the same field count. Quoted fields may
contain delimiters, quotes, and newlines; doubled quotes inside a quoted field
represent one literal quote. The writer accepts exactly one IDM `Table` block;
attempting to write a paragraph, multiple blocks, or a ragged table returns a
serialization error instead of silently dropping data.

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
format    Markdown (guessed: extension .md)
size      3.1 KB
lines     82
words     411
bytes     3079
```

`--json` object: `{"path": ..., "format": ..., "confidence": ...,
"evidence": ..., "size_bytes": ..., "lines": ..., "words": ...,
"bytes": ...}` -- `lines`/`words`/`bytes` are `null` for formats without
native text statistics (for example PDF, DOCX, CSV, and TSV), `size_bytes` is
`null` for stdin (unknown until read). Exit codes: 0 ok, 1 input exists but
cannot be read (permission denied, directory), 4 missing file (the path does
not exist).

On any error, `--json` writes **nothing to stdout**; the usual human line
(for example `✗ no such file: /nope.md`, or the ASCII `[fail] ...`) goes to
stderr and the exit code matches the cause. JSON consumers must check the
process exit code rather than parse an error object: M0 emits no JSON on
failure.

## `smelt doctor`

```
smelt doctor
```

Reports which optional external engines are on PATH (`soffice`,
`tesseract`, `pandoc` in M0), each with a `--version` probe (5 s timeout,
degrades to path-only when the probe fails or times out). Engine paths and
version banners are environment-specific; only the row shape and the
trailing note are normative. The name column is 14 wide, and paths plus the
version detail are dimmed:

```
✓ soffice       /usr/bin/soffice (<version banner>)
✓ tesseract     /usr/bin/tesseract (<version banner>)
✗ pandoc        not found (optional)

smelt M0 uses no external engines; these are reported for future milestones.
```

A `✓` row whose probe failed or timed out prints the bare path with no
parenthesized version. Mark color follows DESIGN.md (success green, failure
red) and is suppressed off a TTY or under `NO_COLOR`/`--color=never`.

Always exits 0. Never runs on the convert path (PERFORMANCE.md guardrail 1).

## `smelt completions`

```
smelt completions <SHELL>
```

`SHELL` is one of `bash`, `elvish`, `fish`, `powershell`, `zsh`; prints a
completion script to stdout (pipe into the shell's completion dir, e.g.
`smelt completions bash > /etc/bash_completion.d/smelt`).

## Planned commands (M1+, no code yet)

| Command | Milestone | Purpose |
| --- | --- | --- |
| `smelt watch` | M3 | watch a directory and convert on change (`-r` recursive flag arrives with batch mode improvements in M1) |
| `smelt diff` | M3 | semantic diff of two documents via the IDM |
| `smelt ocr` | M3 | OCR images and scanned PDFs via Tesseract |
| `smelt man` | M4 | emit the man page (clap_mangen) |
| `smelt convert -r` | M1 | recursive batch traversal |
