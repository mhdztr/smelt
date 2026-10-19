# PERFORMANCE.md -- speed targets and guardrails

The performance thesis: smelt must feel instant on single files and beat
per-file engine wrappers by a wide margin on batches.

## Targets

| Scenario | Target |
| --- | --- |
| Startup (`smelt --version` wall time) | ~5 ms |
| md -> html, single small file | < 10 ms total |
| Batch of N files vs LibreOffice-per-file (M1) | >= 5-15x faster |
| md/docx conversion vs pandoc (M1) | >= 2x faster |
| Cache hit (M1) | ~0 engine time; hash + lookup only |
| PDF page-parallel speedup (M2) | >= 3x on 8+ pages with 8 threads |

Numbers are measured, not vibes: every milestone that claims a target also
ships the harness output that proves it.

## Benchmark harness plan

- `scripts/bench.sh` -- builds release, generates a synthetic fixture set
  (50/200 markdown files of mixed size), times:
  - `smelt` single-file conversion, batch conversion, and `--version`
    startup,
  - the equivalent pandoc / LibreOffice-per-file command when the tool is
    installed, otherwise prints a clear "not installed, skipping" line and
    continues.
- Tool: `hyperfine` when available; **hyperfine is not installed on the
  current machine** -- `bench.sh` checks, explains how to install it
  (`pacman -S hyperfine`), and falls back to plain `time`-based loops with
  best-of-5 reporting.
- Results are recorded in milestone PRs/commits, not committed as files, to
  avoid stale numbers in the repo.

## Guardrails (numbered rules; violations are review blockers)

1. **Never spawn a subprocess per file.** External engines run in warm pools
   (ARCHITECTURE.md). Version probes (`--version`) run only in
   `smelt doctor`, never on convert paths.
2. **Never block the progress/UI thread on engine I/O.** Workers report
   through channels; indicatif consumes events.
3. **Every new dependency is justified against the startup budget.** The
   binary must stay near the ~5 ms startup thesis; heavy runtime
   initialization (regex compilation at startup, async runtimes, large
   static tables) needs a written justification in the commit message.
4. **Readers stream from bounded buffers.** Detection reads at most a 1 MiB
   prefix; converters must not require the whole file in memory when the
   format allows chunking (OOXML zip scanning, PDF pages).
5. **Cache (M1+) is keyed by content hash + engine version.** No mtime-based
   keys; no caching across differing engine versions.

## Measured baseline

Filled in at M1 with the first real engine comparisons; M0 ships the
harness skeleton and the startup measurement in `scripts/bench.sh`.
