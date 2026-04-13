<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-extract">
  <img src="https://img.shields.io/crates/v/alef-extract?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-extract

Rust source extraction for alef

Parses Rust source files using `syn` to extract the public API surface into the alef IR (`ApiSurface`). Recursively resolves `mod` declarations from root source files, handles `pub use` re-exports, `#[cfg]`-gated fields, doc comments, serde `rename_all` attributes, newtype resolution, `#[derive]` trait detection, and default value extraction. Produces a complete `ApiSurface` containing struct definitions, enum definitions, error enums, free functions, and impl block methods for downstream code generation.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
