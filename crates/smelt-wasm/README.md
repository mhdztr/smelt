# smelt-wasm

Browser bindings for smelt document conversion. Conversions run entirely in WebAssembly with no backend conversion server, no remote domain requirement, no filesystem access, and no external engines. Designed to power offline, local-only client-side tools and the planned `smelt ui` localhost GUI (`127.0.0.1`).

## Supported formats (wasm build)

The default wasm build supports: `md`, `html`, `txt`, `csv`, `tsv`.

Office formats, LibreOffice pools, OCR, watch, and filesystem operations are CLI-only. Availability in a specific build is reported by `supported_formats()` and enforced by `convert()`.

## API

- `version() -> string`
- `supported_formats() -> string` (JSON array of format extensions)
- `detect_format(input: Uint8Array, filename?: string) -> string`
- `convert(input: Uint8Array, from?: string | null, to: string, filename?: string | null) -> Uint8Array`
- `convert_text(input: string, from?: string | null, to: string, filename?: string | null) -> string`

`from` may be omitted, `null`, or `"auto"` to auto-detect. A `filename` hint improves detection for text-family formats.

## Build

Requires Rust with the `wasm32-unknown-unknown` target and `wasm-pack`.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build crates/smelt-wasm --target web --out-dir pkg --release
```

For the demo, build into `demo/pkg`:

```sh
wasm-pack build crates/smelt-wasm --target web --out-dir ../../demo/pkg --release
```

## Browser note

The demo loads the module as an ES module. Most browsers block ES module loading from `file://`; serve the demo directory with any static file server (conversion still happens in the browser):

```sh
python3 -m http.server --directory demo 8000
```
