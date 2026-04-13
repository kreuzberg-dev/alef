<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-cli">
  <img src="https://img.shields.io/crates/v/alef-cli?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-cli

CLI for the alef polyglot binding generator

The main entry point for alef, providing the `alef` command-line tool that orchestrates the full binding generation pipeline. Commands include extract, generate, stubs, scaffold, readme, docs, build, test, lint, verify, diff, sync-versions, e2e, and cache. Uses blake3-based caching for incremental regeneration and includes a backend registry wiring all 11 language backends (Python, TypeScript, Ruby, PHP, Go, Java, C#, Elixir, R, WASM, FFI/C).

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
