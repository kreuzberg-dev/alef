<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-go">
  <img src="https://img.shields.io/crates/v/alef-backend-go?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-go

Go (cgo) backend for alef

Generates Go source code using cgo to call the C FFI layer produced by `alef-backend-ffi`. Produces Go structs, methods, and free functions with automatic C memory management via `defer` cleanup, serde rename strategies for struct field JSON tags, and Go-idiomatic error returns. Includes `#cgo` directives for linking against the compiled FFI shared library and generates go.mod scaffolding.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
