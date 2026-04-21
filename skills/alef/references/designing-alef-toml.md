# Designing an alef.toml

Practical guide for configuring alef.toml for a Rust library. Based on patterns from kreuzberg-dev projects: html-to-markdown, kreuzberg, kreuzcrawl, liter-llm, and tree-sitter-language-pack.

## Strategy: Include vs Exclude

Choose your filtering approach based on API surface size.

### Small, focused API → use `[include]`

When most types are internal and only a few are public-facing:

```toml
[include]
types = ["ConversionOptions", "MetadataConfig", "ConversionResult"]
functions = ["convert"]
```

### Large API with internal machinery → use `[exclude]`

When most types should be exposed but some are FFI-incompatible:

```toml
[exclude]
types = [
  # Traits (can't cross FFI boundary)
  "LlmClient",
  "LlmClientRaw",
  # Async wrapper types
  "BoxFuture",
  "BoxStream",
  # Types with Arc<dyn Trait> or unresolvable generics
  "ClientConfig",
  "ClientConfigBuilder",
  # Tower/middleware types (feature-gated, generic)
  "BudgetLayer",
  "CacheLayer",
]
methods = [
  # Raw/internal methods
  "DefaultClient.chat_raw",
  "DefaultClient.chat_stream_raw",
  # Constructors that take excluded types
  "DefaultClient.new",
  # Methods returning bytes::Bytes (not FFI-friendly)
  "DefaultClient.speech",
]
functions = ["internal_helper", "build_provider"]
```

### What to exclude — decision checklist

| Category | Example | Why exclude |
|----------|---------|-------------|
| Traits | `LlmClient`, `BatchClient` | Can't cross FFI boundary |
| `Arc<dyn Trait>` fields | `ClientConfig` (holds `Arc<dyn Provider>`) | Not serializable |
| `BoxFuture` / `BoxStream` | Async wrapper types | Opaque, language-specific |
| Generic middleware | `BudgetLayer<S>`, `CacheLayer<S>` | Unresolvable type params |
| Raw/internal methods | `Client.chat_raw` | Expose high-level API only |
| Constructors needing builders | `Client.new` | Use adapter pattern instead |
| Methods returning `bytes::Bytes` | `Client.speech` | Not all languages handle raw bytes |
| Feature-gated types | Types behind `#[cfg(feature = "tower")]` | May not be enabled |

### Method-level filtering

Use dot-notation `"TypeName.method_name"` to exclude specific methods while keeping the type:

```toml
[exclude]
methods = [
  "CrawlResult.new",           # Hide constructor (use builder/factory)
  "DefaultClient.chat_raw",    # Hide raw API, expose high-level
  "DefaultClient.speech",      # Returns bytes::Bytes
]
```

## Source Configuration

### Single-crate library

```toml
[crate]
name = "my-library"
sources = ["src/lib.rs", "src/types.rs", "src/config.rs"]
```

Cherry-pick source files — don't include the whole crate tree. Only list files containing public API types.

### Multi-crate workspace with facade

When a facade crate re-exports types from internal crates:

```toml
[crate]
name = "kreuzberg"
core_import = "kreuzberg"
sources = [
  "crates/kreuzberg/src/lib.rs",
  "crates/kreuzberg-core/src/extraction.rs",
]
workspace_root = "."
```

### Multi-crate with separate extraction (source_crates)

When types come from different crates and you need rust_path to reflect the actual crate:

```toml
[crate]
name = "tree-sitter-language-pack"
core_import = "tree_sitter_language_pack"
sources = []  # Ignored when source_crates is non-empty

[[crate.source_crates]]
name = "tree-sitter-language-pack"
sources = ["crates/ts-pack-core/src/lib.rs"]
```

### Path mappings

When extracted paths don't match import paths in binding crates:

```toml
[crate]
path_mappings = { "spikard" = "spikard_http" }
auto_path_mappings = true  # default: auto-derive from crates/{name}/src/
```

### Features

Enable features to include `#[cfg(feature)]` gated fields:

```toml
[crate]
features = ["full", "metadata", "serde", "visitor"]
```

## Opaque Types

For external crate types that alef can't extract from source — they become handle-based wrappers:

```toml
[opaque_types]
Language = "tree_sitter_language_pack::Language"
Parser = "tree_sitter_language_pack::Parser"
Tree = "tree_sitter_language_pack::Tree"
```

Use when:

- Type comes from a dependency (not your source)
- Type has complex internals but simple API
- Bindings just pass handles back to Rust functions

## Adapters

Define adapters when alef can't auto-generate the binding pattern.

### Streaming (iterator/stream)

```toml
[[adapters]]
name = "chat_stream"
pattern = "streaming"
core_path = "chat_stream"
owner_type = "DefaultClient"
item_type = "ChatCompletionChunk"
error_type = "LiterLlmError"

[[adapters.params]]
name = "req"
type = "ChatCompletionRequest"
```

### Sync function with GIL release

```toml
[[adapters]]
name = "convert"
pattern = "sync_function"
core_path = "html_to_markdown_rs::convert"
returns = "ConversionResult"
gil_release = true

[[adapters.params]]
name = "html"
type = "String"

[[adapters.params]]
name = "options"
type = "ConversionOptions"
optional = true
```

### Callback bridge (host implements Rust trait)

```toml
[[adapters]]
name = "visitor"
pattern = "callback_bridge"
core_path = "html_to_markdown_rs::visitor"
trait_name = "HtmlVisitor"
trait_method = "handle_element"
returns = "VisitorAction"
detect_async = true

[[adapters.params]]
name = "element"
type = "ElementInfo"
```

## FFI Configuration

Go, Java, and C# require the C FFI layer. Configure it with:

```toml
[ffi]
prefix = "htm"                  # C symbol prefix (htm_new, htm_free, etc.)
header_name = "html_to_markdown.h"
lib_name = "html_to_markdown_ffi"
visitor_callbacks = true         # Enable when using callback_bridge adapters
```

## Language Configurations — Essential Fields

### Python

```toml
[python]
module_name = "_html_to_markdown"  # Native extension name (underscore prefix convention)

[python.stubs]
output = "packages/python/html_to_markdown/"
```

### Node/TypeScript

```toml
[node]
package_name = "@kreuzberg/html-to-markdown-node"
```

### Go

```toml
[go]
module = "github.com/kreuzberg-dev/html-to-markdown/packages/go/v3"
package_name = "htmltomarkdown"
```

### Ruby

```toml
[ruby]
gem_name = "html_to_markdown"

[ruby.stubs]
output = "packages/ruby/sig/"
```

## DTO Style Selection

```toml
[dto]
python = "dataclass"         # Most common; "typed-dict" for read-only return types
python_output = "typed-dict" # Optional: different style for return types
node = "interface"           # "zod" when runtime validation needed
ruby = "struct"              # "data" for Ruby 3.2+ frozen value objects
php = "readonly-class"       # "array" for associative array consumers
```

## Extra Dependencies

Add Cargo dependencies to all generated binding crate Cargo.tomls:

```toml
[crate]
extra_dependencies = { tokio = { version = "1", features = ["rt-multi-thread"] } }
```

## Common Patterns

### Hide constructor, expose factory

When a type's `new()` takes complex args, exclude the constructor and use an adapter:

```toml
[exclude]
methods = ["MyClient.new"]

[[adapters]]
name = "create_client"
pattern = "sync_function"
core_path = "my_crate::MyClient::builder"
returns = "MyClient"
```

### Expose subset of methods

Keep the type but hide internal methods:

```toml
[exclude]
methods = [
  "MyType.internal_helper",
  "MyType.debug_dump",
  "MyType.raw_handle",
]
```

### WASM-specific exclusions

Some functions can't work in WASM (blocking I/O, threads):

```toml
[wasm]
exclude_functions = ["blocking_read", "spawn_worker"]
exclude_types = ["ThreadPool"]
```
