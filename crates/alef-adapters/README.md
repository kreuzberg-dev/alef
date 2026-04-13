<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-adapters">
  <img src="https://img.shields.io/crates/v/alef-adapters?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-adapters

FFI adapter pattern generators for alef

Generates glue code for sync_function, async_method, callback_bridge, streaming, and server_lifecycle adapter patterns. Each adapter reads definitions from the `[[adapters]]` section of alef.toml and produces language-specific implementations that bridge host language callbacks to Rust trait impls. The generated method/function bodies are keyed by type and method name, which backends then splice into their binding code.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
