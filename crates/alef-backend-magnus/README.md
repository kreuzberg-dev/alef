<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-magnus">
  <img src="https://img.shields.io/crates/v/alef-backend-magnus?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-magnus

Ruby (Magnus) backend for alef

Generates Rust source code using the Magnus framework to expose a Rust library as a native Ruby extension. Produces Ruby class definitions with constructors, instance methods, and static methods using `magnus::function!` and `magnus::method!` macros. Enum types are mapped to Ruby constants, and types conflicting with Magnus internals are automatically renamed. Supports `struct`, `dry-struct`, and `data` DTO generation styles, and generates `.rbs` type signature files for static type checking.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
