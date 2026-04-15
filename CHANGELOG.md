# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.4] - 2026-04-15

### Added

- `alef sync-versions --set <version>` — set an explicit version (supports pre-release like `0.1.0-rc.1`)
- `alef verify` now checks version consistency across all package manifests
- `alef-sync-versions` pre-commit hook for automatic version propagation on Cargo.toml changes
- PEP 440 pre-release conversion for Python (`0.1.0-rc.1` → `0.1.0rc1`)
- `reverse_conversions` config flag to gate binding-to-core `From` impls
- E2E registry mode for test_apps generation across all 12 languages
- `alef-docs` fallback description generator for struct fields and enum variants
- `alef all` now includes e2e and docs generation; scaffold reads workspace version
- Elixir `ex_doc` support in scaffold
- Java scaffold: switch to `central-publishing-maven-plugin`; Node scaffold: `optionalDependencies`
- Enriched PHP/Python/Java scaffolds for CI and publishing
- PHP composer.json scaffold: `scripts` section with `phpstan`, `format`, `format:check`, `test`, `lint`, and `lint:fix` commands

### Fixed

- Docs generator: critical type, default value, enum, and error generation fixes across Go, C#, Rust
- Go doc signatures with empty return type; C# double `Async` suffix removed
- FFI codegen cast corrections; README version template rendering
- PHP scaffold autoload and backslash escaping in `composer.json` PSR-4 namespace
- PHP stubs: generate public property declarations on classes (ext-php-rs exposes fields as properties, PHPStan needs them declared)
- PHP stubs: camelCase naming; remove hardcoded `createEngineFromJson` from facade and stubs
- PHP codegen: remove needless borrow in `serde_json::to_value` calls (clippy fix)
- Python stubs: add `# noqa: A002` to constructor parameters that shadow Python builtins (e.g. `id`)
- Python stubs: place `noqa` comment after comma in multi-line `__init__` params
- Python scaffold: removed `[tool.ruff]` section — linter config belongs in root `pyproject.toml`
- Python duration stubs: correct mypy annotations
- Ruby: replace `serde_magnus` with `serde_json`, handle sanitized fields in `From` impls
- Ruby gemspec version sync: match single-quote `spec.version = '...'` (was only matching double quotes)
- Java: `checkLastError` throws `Throwable`; correct Jackson version to 2.19.0
- WASM: `option_duration` handling; added missing `wasm-bindgen-futures` dependency
- WASM codegen: remove unused `HashMap` import
- Node/FFI scaffolds: removed unused `serde` dependency
- Node scaffold: removed unused `serde_json` dependency
- Rustler backend: output path uses `_nif` suffix instead of `_rustler`
- Version sync: recursive `.csproj` scanning, WASM/root `composer.json`, FFI crate name
- Only generate binding-to-core `From` impls for input types, not output-only types
- `path_mismatch` false positive on re-exported types (same crate root + name)
- Add `Language::Rust` to all match arms across codebase
- Rust e2e: `assert!(bool)` for clippy, `is_empty` for len comparisons, unqualified imports
- E2e: add `setuptools packages=[]` to Python registry `pyproject.toml`
- Clippy `field_reassign_with_default` suppressed for Duration builder pattern
- Scaffold Cargo.toml templates: removed unused deps — `pyo3-async-runtimes` (Python), `serde_json` (Node), `tokio` (PHP, FFI), `wasm-bindgen-futures` (WASM), `serde`+`tokio` (Elixir/Rustler) — only include what generated binding code actually uses

## [0.3.3] - 2026-04-14

### Added

- Distributable Claude Code skill for alef consumers

### Fixed

- PHP 100% coverage — `createEngineFromJson`, JSON config e2e support
- PHP snake_case properties, enum scalars, serde casing, camelCase facade
- PHP class registration, property access, per-field attributes
- PHP setters documented, enum serde casing via `serde_json`
- PHP tests updated for `Api` class pattern (no more standalone `#[php_function]`)
- Correct DTO style naming (`typed-dict` not `typeddict`), add `serde_rename_all` to config docs
- Add Credo to Elixir scaffold, PHPStan + PHP-CS-Fixer to PHP scaffold
- Drop unused `tokio` from Python scaffold, add e2e license, split assertions
- Remove unused `serde` dep from Node.js and FFI scaffolds
- PHP and Rustler miscellaneous fixes

## [0.3.2] - 2026-04-13

### Fixed

- Disable `jsonschema` default features (`resolve-http`, `tls-aws-lc-rs`) to remove `reqwest`/`aws-lc-sys` from dependency tree, fixing GoReleaser cross-compilation

## [0.3.1] - 2026-04-13

### Fixed

- Elixir resource registration and enum conversion fixes
- C# opaque handle wrapping, error handling, `Json` to `object` type mapping, `WhenWritingDefault` serialization
- Dropped `x86_64-apple-darwin` from GoReleaser targets (`aws-lc-sys` cross-compile failure)

## [0.3.0] - 2026-04-13

### Added

- Pre-commit hook support for consumer repos (`alef-verify` and `alef-generate` hooks via `.pre-commit-hooks.yaml`)
- Blake3-based caching for stubs, docs, readme, scaffold, and e2e generation — all commands now skip work when inputs are unchanged
- `cargo binstall alef-cli` support via `[package.metadata.binstall]` metadata
- `alef-docs` and `alef-e2e` added to crates.io publish workflow

### Changed

- All workspace dependencies consolidated to root `Cargo.toml` — every crate now uses `{ workspace = true }` for both internal and shared external deps
- Bumped all crates from 0.2.0 to 0.3.0

### Fixed

- Replaced `.unwrap()` with `.extend()` in `to_snake_case` (`alef-adapters`)
- Replaced runtime `todo!()`/`panic!()` with `compile_error!()` in generated code for unimplemented adapter patterns
- Made `PrimitiveType` matches exhaustive, removing `unreachable!()` (`alef-codegen`)
- Added inline SAFETY comments to FFI pointer dereferences (`alef-backend-ffi`)
- Clamped negative Duration values before `u64` cast in NAPI bindings
- Fixed rustler public API test assertions to match Elixir conventions (no parens on zero-arg defs, keyword defstruct)

## [0.2.0] - 2026-04-13

### Added

- `alef e2e` CLI command with `generate`, `list`, `validate`, `scaffold`, `init` subcommands
- E2E test generators for 12 languages (Rust, Python, TypeScript, Go, Java, C#, PHP, Ruby, Elixir, R, WASM, C)
- `alef-e2e` crate with fixture loading, JSON Schema validation, and `FieldResolver` for nested field paths
- `options_type`, `enum_module`, `enum_fields`, `options_via` config support for flexible argument passing

### Fixed

- crates.io version specifier fixes for path dependencies
- Backend-specific e2e test generation fixes across multiple languages

## [0.1.0] - 2026-04-09

### Added

- Initial release with 20 crates in a Cargo workspace
- Full CLI: `extract`, `generate`, `stubs`, `scaffold`, `readme`, `docs`, `sync-versions`, `build`, `lint`, `test`, `verify`, `diff`, `all`, `init`
- 11 language backends: Python (PyO3), TypeScript (NAPI-RS), Ruby (Magnus), Go (cgo/FFI), Java (Panama FFM), C# (P/Invoke), PHP (ext-php-rs), Elixir (Rustler), R (extendr), WebAssembly (wasm-bindgen), C (FFI)
- Async bridging and adapter patterns: streaming, callback bridge, sync function, async method
- Method delegation with opaque type wrapping via `Arc<T>`
- Error type generation from `thiserror` enums with cross-language exception mapping
- Type alias and trait extraction from Rust source
- Blake3-based caching for `extract` and `generate` commands
- CI pipeline: cargo fmt, clippy, deny, machete, sort, taplo
- GoReleaser-based publish workflow with cross-platform binaries and Homebrew tap
