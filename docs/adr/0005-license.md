# ADR 0005: MIT license

Status: accepted

## Context

smelt statically links third-party C libraries (ADR 0001), so the license
of those libraries interacts with distribution. The project also wants the
lowest-friction license for AUR/homebrew/corporate adoption.

## Decision

smelt is **MIT** licensed, copyright (c) 2026 mhdztr.

Compatibility notes for the statically linked libraries:

- **PDFium** is BSD-3-Clause: statically linking it into an MIT binary is
  compatible; BSD attribution is preserved in the binary's third-party
  notices when PDFium lands (M2).
- **Tesseract 5** is Apache-2.0: Apache-2.0 permits static linking into
  binaries under other licenses, but distribution must include the Apache
  license, the NOTICE attribution, and a statement of changes (none -- the
  library is unmodified). The release process must ship these alongside the
  binary (M3).
- HarfBuzz (MIT), FriBidi (LGPL-2.1 with an explicit static-linking
  exception), zlib-ng ( zlib/BSD-3 dual) are compatible under the same
  conditions.

## Consequences

- No copyleft contagion; AUR `license=(MIT)` is accurate for the smelt
  source package.
- Third-party notices become a required release artifact once FFI lands;
  tracked in PLAN.md M2/M3 milestones.
