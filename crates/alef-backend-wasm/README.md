<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-wasm">
  <img src="https://img.shields.io/crates/v/alef-backend-wasm?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-wasm

WASM (wasm-bindgen) backend for alef

Generates Rust source code using wasm-bindgen to expose a Rust library as a WebAssembly module usable from JavaScript and TypeScript. Produces `#[wasm_bindgen]` structs and functions with proper type mappings for browser and Node.js environments, and supports async operations through wasm-bindgen-futures. Provides configurable exclusions and type mapping overrides via `WasmConfig`, allowing functions or types to be excluded and types to be remapped for WASM compatibility.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
