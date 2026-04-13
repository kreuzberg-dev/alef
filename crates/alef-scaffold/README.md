<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-scaffold">
  <img src="https://img.shields.io/crates/v/alef-scaffold?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-scaffold

Package scaffolding generator for alef

Generates complete package manifests and build configuration files for each target language. Supported outputs include pyproject.toml, package.json, .gemspec, composer.json, mix.exs, go.mod, pom.xml, .csproj, DESCRIPTION (R), and Cargo.toml files for binding crates. For languages with native Rust binding crates, generates both the language-side manifest and the Rust binding crate's Cargo.toml. Scaffold metadata is read from the `[scaffold]` section of alef.toml with per-language feature overrides.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
