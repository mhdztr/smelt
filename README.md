# smelt

**Melt any document into any other.**

`smelt` is an advanced document converter CLI: a fast, single-binary
alternative to pandoc invocations and LibreOffice-per-file wrappers. It reads
documents into a normalized intermediate model (the IDM), then writes that
model out to any supported target format -- so adding a format costs one
reader or one writer, not N x M converters.

- **Fast**: ~5 ms startup, persistent warm engine pools (never one subprocess
  per file), page-parallel PDF work, content-hash caching, parallel batches.
- **Single self-contained binary**: external tools (LibreOffice, pandoc,
  calibre, groff) are optional accelerators detected at runtime, never
  requirements. No Python, no Node. The current Linux build is dynamically
  linked against glibc (libc, libm, libgcc_s); a fully static musl artifact
  is planned for M4 release packaging.
- **Honest output**: fidelity reporting, actionable errors, documented exit
  codes, clean Unicode UI with automatic ASCII fallback.

Status: **M1 in progress** -- native Markdown/HTML/plain-text and CSV/TSV
conversion, magic-byte format detection, `convert`, `info`, `doctor`, and
`completions`. The full roadmap lives in [PLAN.md](PLAN.md); the full format
matrix in [FORMATS.md](FORMATS.md).

## Install

From source (requires a stable Rust toolchain):

```sh
git clone https://github.com/mhdztr/smelt
cd smelt
cargo build --release
cargo install --path crates/smelt-cli   # installs the `smelt` binary
```

Arch Linux: AUR packages (`smelt`, `smelt-bin`, `smelt-git`) are planned;
see [docs/AUR.md](docs/AUR.md) for the publish runbook.

## Quickstart

Every command below works on a fresh clone after `cargo build --release`.
Examples use `./target/release/smelt`; substitute `smelt` once installed.

```sh
# Convert one file. The output format is inferred from the -o extension.
./target/release/smelt convert README.md -o README.html

# Explicit target format; stdin/stdout streaming with -
printf '# Hi\n\n**bold**\n' | ./target/release/smelt convert - -t html

# Round-trip the other way
printf '<h1>Hi</h1><p>text</p>' | ./target/release/smelt convert - -t md

# Inspect a file: detected format, size, and basic stats
./target/release/smelt info README.md
./target/release/smelt info README.md --json

# Environment report: which optional engines are available
./target/release/smelt doctor

# Batch-convert a directory (parallel, with a summary box)
mkdir -p docs-in
printf '# One\n' > docs-in/one.md
printf 'Two\n\nsome text\n' > docs-in/two.md
./target/release/smelt convert docs-in -t html -o docs-out
```

## Documentation

| Document | Contents |
| --- | --- |
| [PLAN.md](PLAN.md) | Master plan, milestones M0-M4, risks |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crate graph, IDM pipeline, pools, FFI |
| [DESIGN.md](DESIGN.md) | CLI UX spec: glyphs, colors, output samples |
| [FORMATS.md](FORMATS.md) | Full format matrix and lossy-case list |
| [PERFORMANCE.md](PERFORMANCE.md) | Speed targets and perf guardrails |
| [AGENTS.md](AGENTS.md) | Conventions and rules for AI builders |
| [docs/cli.md](docs/cli.md) | Full command reference and exit codes |
| [docs/idm-spec.md](docs/idm-spec.md) | Intermediate Document Model spec |

## License

MIT. See [LICENSE](LICENSE).
