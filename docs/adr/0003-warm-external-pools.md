# ADR 0003: warm external engine pools

Status: accepted (implemented in M1)

## Context

LibreOffice, pandoc, calibre, and groff are the only practical routes to
many proprietary/legacy formats, but cold-starting them per file dominates
runtime: `soffice` startup alone is on the order of a second. Wrappers that
spawn one process per file are exactly the slowness smelt exists to fix.

## Decision

External engines run in **persistent warm pools**: N long-lived worker
processes started on first use, serving a bounded request queue with
health checks and idle shutdown. A worker that fails K consecutive
requests is recycled. `smelt doctor` runs the same health check as a
precondition report; convert paths never run version probes
(PERFORMANCE.md guardrail 1). Pool startup failure is an actionable error,
never a silent fallback to per-file spawning.

## Consequences

- Batch conversion pays engine startup once per pool lifetime, not once per
  file.
- `smelt` must reap idle workers so it never leaves stray processes.
- Engine version is part of the cache key; pool health failures map to a
  documented error string (`soffice pool: no workers. run 'smelt doctor'`).
- Tests never require an engine to be installed; pools are optional by
  design (ADR 0004).
