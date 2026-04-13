<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-core">
  <img src="https://img.shields.io/crates/v/alef-core?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-core

Core types, config schema, and backend trait for the alef polyglot binding generator

Defines the intermediate representation (IR) used to describe a Rust crate's public API surface, including `ApiSurface`, `TypeDef`, `FunctionDef`, `EnumDef`, `ErrorDef`, `FieldDef`, `MethodDef`, and `TypeRef`. Also defines the `Backend` trait that all language backends implement, the `AlefConfig` schema for alef.toml configuration files, the `Language` enum covering all supported targets, and the `GeneratedFile`/`Capabilities` types. The IR is serializable via serde and serves as the exchange format between extraction and code generation stages.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
