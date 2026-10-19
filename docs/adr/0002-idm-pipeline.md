# ADR 0002: IDM pipeline (Reader -> IDM -> Writer)

Status: accepted

## Context

Converter projects typically wire each reader directly to each writer:
N x M converters, duplicated styling logic, and no substrate for features
beyond conversion (diff, fidelity, caching).

## Decision

All conversion routes through a normalized Intermediate Document Model.
Readers produce `Document` values; writers consume them; a transform
pipeline sits between (identity in M0; filters, scoring, cache later).

## Consequences

- Adding a format costs one reader or one writer, not N or M converters.
- **Semantic diff** and **fidelity reporting** fall out naturally: both are
  IDM-to-IDM comparisons (PLAN.md M3).
- The cache has a canonical payload: the serde form of a `Document` keyed by
  content hash + direction + engine version.
- The model is deliberately lossy (presentation styles, raw passthrough);
  known losses are enumerated in FORMATS.md rather than silently absorbed.
