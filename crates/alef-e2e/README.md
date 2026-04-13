<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-e2e">
  <img src="https://img.shields.io/crates/v/alef-e2e?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-e2e

Fixture-driven e2e test generator for alef

Reads JSON fixture files and generates complete, self-contained end-to-end test projects for 12 target languages: Rust, Python, TypeScript, Ruby, PHP, Go, Java, C#, Elixir, R, WASM, and C. Supports 40+ assertion types, field path resolution, JSON Schema validation, and per-language formatting. Each language implements the `E2eCodegen` trait to produce test files, build manifests, and assertion helpers tailored to its test framework. Generated projects are immediately runnable with each language's standard test runner.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
