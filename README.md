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
  calibre, tesseract, groff) are optional accelerators detected at runtime,
  never requirements. No Python, no Node. Release Linux binaries are
  dynamically linked against glibc (libc, libm, libgcc_s); fully static musl
  artifacts are not a 2.0.0 ship claim.
- **CLI-first, zero domain hassle**: Designed purely as a fast local CLI.
  No remote web servers, no public cloud endpoints, and no domain names to manage.
  For users needing a graphical interface, the CLI provides an optional local
  GUI (`smelt ui`) bound strictly to `localhost` (`127.0.0.1`) in the browser.
- **Browser bindings**: `smelt-wasm` exposes core conversion in the browser
  for a wasm-safe format subset (md/html/txt/csv/tsv). The included demo is a
  static client-side page -- not a backend conversion server and not a CLI
  replacement.
- **Ponytail engineering philosophy**: All contributions and AI sessions follow
  the Ponytail methodology (`.agent/skills/cleancode-ponytail/ponytail`):
  the smallest working change, strict YAGNI, standard library first, no code bloat,
  and explicit debt tracking.
- **Anti-slop frontend & interaction design**: Any UI work (localhost GUI / WASM demo)
  strictly follows the design skills in `.agent/skills/` (`frontend-design.md`,
  `design-taste-frontend`, and `design-kowalski`): distinctive typography, OKLCH
  palettes, Apple-grade fluid physical motion, and zero AI template tropes.
- **Honest output**: actionable errors, documented exit codes, clean Unicode
  UI with automatic ASCII fallback.

Status: **2.0.0 / M5 ship** -- native Markdown/HTML/plain-text and CSV/TSV
conversion in core/CLI, basic `watch`/`diff`/`ocr`/`man` commands, usable
`smelt-wasm` browser bindings, a static demo, and CI/release artifacts.
The full roadmap and historical milestones live in [PLAN.md](PLAN.md); the
full format matrix in [FORMATS.md](FORMATS.md).

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

## Release artifacts

Tagged `v*` releases publish GitHub release assets:
- `smelt-x86_64-unknown-linux-gnu.tar.gz`
- `smelt-aarch64-unknown-linux-gnu.tar.gz` (built via `cross`)
- `smelt-wasm-web-pkg.tar.gz` (wasm-pack web package)

Download, extract, and place the `smelt` binary on your `PATH` (Linux release
binaries depend on glibc/gcc-libs at runtime).

## Browser demo (static, client-side)

The demo in `demo/` uses `smelt-wasm` in the browser. It performs conversion
client-side only -- there is **no backend conversion server**.

```sh
# Build the wasm package (requires wasm-pack)
wasm-pack build crates/smelt-wasm --target web --out-dir ../../demo/pkg --release

# Serve the static demo (ES modules require HTTP)
python3 -m http.server --directory demo 8000
# Open http://localhost:8000
```

v1 wasm formats: Markdown, HTML, plain text, CSV, TSV. Unsupported formats
return actionable errors.

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
| [AGENTS.md](AGENTS.md) | Conventions and rules for AI builders (Mandatory Skills Hierarchy) |
| [.agent/skills/](.agent/skills/) | Agent skills: Clean code Ponytail, frontend design, animation |
| [.agent/workflows/](.agent/workflows/) | Design and audit workflows: audit, polish, simplify, harden |
| [docs/cli.md](docs/cli.md) | Full command reference and exit codes |
| [docs/idm-spec.md](docs/idm-spec.md) | Intermediate Document Model spec |

## License

MIT. See [LICENSE](LICENSE).
