<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-ffi">
  <img src="https://img.shields.io/crates/v/alef-backend-ffi?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-ffi

C FFI backend for alef

Generates a complete C-compatible FFI layer with `#[no_mangle] extern "C"` functions, opaque handle types, allocate/free pairs, field accessors, JSON serialization helpers, and cbindgen integration for automatic C header generation. Includes a thread-local error reporting mechanism and a Tokio runtime for async operations. This FFI layer serves as the shared foundation for the Go (cgo), Java (Panama FFM), and C# (P/Invoke) backends.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
