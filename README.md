# eisberg

<div align="center" style="display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; margin: 20px 0;">
  <a href="https://crates.io/crates/eisberg-cli">
    <img src="https://img.shields.io/crates/v/eisberg-cli?label=crates.io&color=007ec6" alt="crates.io">
  </a>
  <a href="https://github.com/kreuzberg-dev/eisberg/releases">
    <img src="https://img.shields.io/github/v/release/kreuzberg-dev/eisberg?label=Release&color=007ec6" alt="Release">
  </a>
  <a href="https://github.com/kreuzberg-dev/eisberg/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/kreuzberg-dev/eisberg/ci.yml?label=CI&color=007ec6" alt="CI">
  </a>
  <a href="https://github.com/kreuzberg-dev/eisberg/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/License-MIT-007ec6" alt="License">
  </a>
  <a href="https://docs.rs/eisberg-cli">
    <img src="https://img.shields.io/badge/docs.rs-eisberg-007ec6" alt="docs.rs">
  </a>
</div>

<img width="3384" height="573" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<div align="center" style="margin-top: 20px;">
  <a href="https://discord.gg/xt9WY3GnKR">
      <img height="22" src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
  </a>
</div>

Polyglot binding generator for Rust. One config file, ten languages, lint-clean output.

## Supported Languages

| Language | Framework | Output |
|----------|-----------|--------|
| Python | PyO3 | Binding crate + `.pyi` stubs |
| Node.js | NAPI-RS | Binding crate + `.d.ts` types |
| Ruby | Magnus | Binding crate |
| PHP | ext-php-rs | Binding crate |
| Elixir | Rustler | NIF binding crate |
| WebAssembly | wasm-bindgen | Binding crate |
| C (FFI) | cbindgen | `extern "C"` functions + `.h` header |
| Go | cgo | Go package wrapping C FFI |
| Java | Panama FFM (JDK 21+) | Records + FFI bridge |
| C# | P/Invoke (.NET 8+) | Classes + FFI bridge |

## Install

```bash
cargo install eisberg-cli
```

Or via Homebrew:

```bash
brew install kreuzberg-dev/tap/eisberg
```

## Quick Start

Create `eisberg.toml` in your Rust workspace root:

```toml
languages = ["python", "node", "ffi", "go"]

[crate]
name = "my-lib"
sources = ["crates/my-lib/src/lib.rs"]
version_from = "Cargo.toml"

[include]
types = ["Config", "Result"]
functions = ["process"]

[output]
python = "crates/my-lib-py/src/"
node = "crates/my-lib-node/src/"
ffi = "crates/my-lib-ffi/src/"

[python]
module_name = "_my_lib"

[ffi]
prefix = "my_lib"

[go]
module = "github.com/my-org/my-lib-go"
```

Generate bindings:

```bash
eisberg generate
```

Build everything:

```bash
cargo build -p my-lib-py -p my-lib-node -p my-lib-ffi
```

## In Production

eisberg generates bindings for:

| Project | Languages | Types | Description |
|---------|-----------|-------|-------------|
| [kreuzberg](https://github.com/kreuzberg-dev/kreuzberg) | 10 | 50+ | Document extraction |
| [html-to-markdown](https://github.com/kreuzberg-dev/html-to-markdown) | 10 | 22 | HTML to Markdown converter |
| [liter-llm](https://github.com/kreuzberg-dev/liter-llm) | 10 | 30+ | LLM client library |
| [spikard](https://github.com/kreuzberg-dev/spikard) | 5 | 15+ | Web framework |

## How It Works

```text
eisberg.toml + Rust source
        |
        v
   eisberg extract          syn parses pub types, functions, enums
        |
        v
   IR (ApiSurface)          cached as JSON in .eisberg/
        |
        v
   eisberg generate         per-language backend emits code
        |
        +-> crates/{name}-py/src/lib.rs       PyO3 bindings
        +-> crates/{name}-node/src/lib.rs     NAPI-RS bindings
        +-> crates/{name}-ffi/src/lib.rs      C FFI + header
        +-> packages/go/binding.go            cgo wrapper
        +-> packages/java/src/                Panama FFM records
        +-> packages/csharp/                  P/Invoke classes
        +-> ...
```

Generated Rust code is automatically formatted with `rustfmt` and passes `cargo clippy -D warnings` with zero file-level suppressions.

## Commands

| Command | Description |
|---------|-------------|
| `eisberg generate` | Generate bindings for all configured languages |
| `eisberg generate --lang python,node` | Generate specific languages only |
| `eisberg generate --clean` | Regenerate ignoring cache |
| `eisberg stubs` | Generate type stubs (`.pyi`, `.rbs`) |
| `eisberg scaffold` | Generate package metadata |
| `eisberg readme` | Generate per-language README files |
| `eisberg sync-versions` | Sync version to all manifests |
| `eisberg verify --exit-code` | CI: fail if bindings are stale |
| `eisberg diff` | Show what would change without writing |
| `eisberg lint` | Run configured linters on generated output |
| `eisberg all` | Run everything (generate + stubs + scaffold + readme) |
| `eisberg init` | Create `eisberg.toml` interactively |

## Configuration Reference

### Include and Exclude

Control which types, functions, and methods are exposed to bindings:

```toml
[include]
types = ["Config", "Result", "Error"]
functions = ["process", "validate"]

[exclude]
types = ["InternalHelper"]
methods = ["Config.apply_update", "Config.from_update"]
```

### Language-Specific Options

```toml
[python]
module_name = "_my_lib"

[python.stubs]
output = "packages/python/my_lib/"

[node]
package_name = "@my-org/my-lib"

[ffi]
prefix = "my_lib"
header_name = "my_lib.h"
lib_name = "my_lib_ffi"

[go]
module = "github.com/my-org/my-lib-go"
package_name = "mylib"

[java]
package = "dev.myorg.mylib"

[csharp]
namespace = "MyLib"

[elixir]
app_name = "my_lib"
```

### Output Directories

```toml
[output]
python = "crates/my-lib-py/src/"
node = "crates/my-lib-node/src/"
ruby = "packages/ruby/ext/my_lib_rb/src/"
php = "crates/my-lib-php/src/"
ffi = "crates/my-lib-ffi/src/"
elixir = "packages/elixir/native/my_lib_nif/src/"
wasm = "crates/my-lib-wasm/src/"
```

## Caching

eisberg caches the extracted IR and per-language output hashes in `.eisberg/` (add to `.gitignore`). Only backends whose inputs changed are regenerated. Use `--clean` to bypass.

## CI Integration

Verify bindings are up to date in CI:

```yaml
- run: eisberg verify --exit-code
```

Or as a pre-commit hook:

```yaml
# .pre-commit-config.yaml
- repo: local
  hooks:
    - id: eisberg-verify
      name: eisberg verify
      entry: eisberg verify --exit-code
      language: system
      pass_filenames: false
```

## Key Design Decisions

- **Shared codegen** — all backends use `ConversionConfig` for type prefixes, casts, enums, Maps
- **Lint-clean** — passes clippy, Checkstyle, golangci-lint, dotnet format
- **Newtype transparency** — single-field tuple structs are resolved to their inner type
- **Auto-formatting** — generated Rust files are formatted with `rustfmt` after generation
- **Incremental** — only regenerates backends whose inputs changed

## Contributing

Contributions welcome! Please open an issue or pull request for development setup and guidelines.

## License

MIT
