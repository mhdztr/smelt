# ADR 0004: no required runtime dependencies

Status: accepted

## Context

Most converter stacks demand runtimes: Python for docx libraries, Node for
modern web formats, a specific Java for LibreOffice scripting. This makes
installation fragile and version conflicts common.

## Decision

- One statically linked binary is the only hard requirement.
- External tools (LibreOffice, pandoc, calibre, groff) are **optional
  accelerators**, detected at runtime and reported by `smelt doctor`; every
  feature they provide fails with an actionable message when absent, and
  the core (M0: md/html/txt) never needs them.
- No Python. No Node. No shell-outs to user-land scripting runtimes, ever.
- Embedded scripting for users (Lua via mlua, vendored) ships inside the
  binary in M4 -- a feature, not a requirement.

## Consequences

- CI and tests run with a bare toolchain: the full suite passes with zero
  engines installed.
- Format coverage is gated by milestone rather than by what a wrapper script
  happens to find on PATH.
- Distro packaging stays trivial: `depends=(gcc-libs glibc)` for the source
  package, `depends=()` for the prebuilt binary.
