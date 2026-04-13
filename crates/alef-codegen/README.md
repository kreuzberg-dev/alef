<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-codegen">
  <img src="https://img.shields.io/crates/v/alef-codegen?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-codegen

Shared codegen utilities for the alef polyglot binding generator

Provides the common code generation infrastructure that language backends build upon. Includes the `TypeMapper` trait for translating IR types to language-specific type strings, struct/enum/function/method generators, conversion generators for binding-to-core and core-to-binding directions, naming convention utilities (snake_case, camelCase, PascalCase), and minijinja template support. Backends use these shared generators to produce consistent binding code without duplicating iteration and conversion logic.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
