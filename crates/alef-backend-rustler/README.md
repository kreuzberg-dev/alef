<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-rustler">
  <img src="https://img.shields.io/crates/v/alef-backend-rustler?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-rustler

Elixir (Rustler) backend for alef

Generates Rust source code using Rustler to expose a Rust library as an Elixir NIF. Produces `#[rustler::nif]` functions, `#[derive(NifStruct)]` and `#[derive(NifUnitEnum)]` types for automatic Elixir term encoding/decoding, and `ResourceArc`-wrapped opaque types for sharing Rust state across NIF calls. Supports `struct` and `typed-struct` DTO generation styles and generates mix.exs scaffolding with proper module namespace configuration.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
