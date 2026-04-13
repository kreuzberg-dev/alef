<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-csharp">
  <img src="https://img.shields.io/crates/v/alef-backend-csharp?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-csharp

C# (P/Invoke) backend for alef

Generates C# source code using P/Invoke to call the C FFI layer produced by `alef-backend-ffi`. Produces a `NativeMethods` class with `extern` declarations, C# record types for data structures, enum definitions, typed exception classes, and a public wrapper class with idiomatic C# method signatures and namespace organization. Generates NuGet package scaffolding and targets .NET 8+ with nullable reference types enabled.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
