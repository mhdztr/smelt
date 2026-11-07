# smelt demo (static, browser-side conversion)

This demo uses `smelt-wasm` to convert documents entirely in the browser. There is **no backend conversion server** and **no remote domain hosting**.

It operates strictly locally (`localhost` / `127.0.0.1`), demonstrating the client-side conversion engine that powers the planned `smelt ui` command for users who prefer a graphical interface over the CLI.

## Build the WASM package

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build crates/smelt-wasm --target web --out-dir ../../demo/pkg --release
```

## Run locally

Most browsers block ES module loading from `file://`. Serve the `demo/` directory with any static file server (conversion still happens client-side):

```sh
python3 -m http.server --directory demo 8000
```

Then open http://localhost:8000

## Notes

- Supported formats depend on the WASM build (`supported_formats()` in the UI/status).
- The default wasm build targets text/table formats: `md`, `html`, `txt`, `csv`, `tsv`.
- Office formats and external-engine conversions are available in the CLI, not in this browser demo.
