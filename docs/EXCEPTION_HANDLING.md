# Exception Handling Across Language Bindings

**Issue Reference**: GitHub issue #147 (Python exception catching)  
**Document Status**: Architecture Guide for Alef Exception Generation  
**Last Updated**: 2026-06-26

---

## Overview

Exception handling is a critical aspect of polyglot bindings. When Rust code raises an error and crosses an FFI boundary into a host language, the error must:

1. **Preserve class/type identity** so that code using `except/catch/raise/throw` can properly handle errors
2. **Maintain numeric error codes** across all language bindings for error routing and logging
3. **Include context** (message, stack trace, cause chain) for debugging
4. **Follow language idioms** (exceptions in Python/Java/C#, error tuples in Elixir, error returns in Go)

## The Problem (Issue #147)

**Python Example (Fixed in alef 0.28.1+)**:

```python
# User code
from tree_sitter_language_pack import DownloadError
try:
    tree_sitter_language_pack.get_language("unknown")
except DownloadError:  # ❌ FAILED - exception was _native.DownloadError
    handle_error()
```

**Root Cause**: The native module created exception classes, but the public Python package defined its own wrapper classes. These were **different class objects**, so `isinstance()` checks failed.

**Solution**: Re-export exception classes from the native module instead of defining duplicates.

```python
# Fixed: exceptions.py
from ._native import DownloadError, ...  # Same object!

# User code now works
try:
    tree_sitter_language_pack.get_language("unknown")
except DownloadError:  # ✅ WORKS - identical class
    handle_error()
```

---

## Pattern: Type Identity Across FFI Boundaries

### Python (PyO3)

**Native Module** → Defines exceptions via `pyo3::create_exception!(_native, ExceptionName, Error)`  
**Public Module** → Re-exports from native module, NOT duplicate classes

```python
# src/lib.rs (Rust)
pyo3::create_exception!(_native, DownloadError, Error);

# exceptions.py (Generated)
from ._native import DownloadError, ...

# __init__.py (Generated)
from .exceptions import DownloadError, ...

# Result: User code catches the SAME class object
```

**Key Principle**: Single source of truth (native module) + re-exports (public API)

### Node.js (NAPI-RS)

**Pattern**: Export exception constructors from native module  
**Implementation**: Use `napi::Error` for Rust errors → JavaScript Error objects  
**Type Identity**: JavaScript Error class is unique; use `instanceof` checks with constructor references

```typescript
// Generated wrapper
import { DownloadError } from './_native';  // Re-exported from native

try {
  getLanguage("unknown");
} catch (e) {
  if (e instanceof DownloadError) { }  // ✅ Works
}
```

### Ruby (Magnus)

**Pattern**: Expose exception classes through native module  
**Implementation**: Use Magnus `#[magnus::exception]` macro for exception mapping  
**Type Identity**: Ruby exception classes created in native module, exposed to Ruby callers

```ruby
# Generated wrapper
require '_native'

begin
  get_language("unknown")
rescue _native.DownloadError => e
  handle_error(e)
end
```

### Go (cgo)

**Pattern**: Error tuples with wrapped errors and error codes  
**Implementation**: Custom error type wrapping C error code + message  
**Type Identity**: Not applicable (Go uses value types, not class inheritance)

```go
// Generated wrapper
func GetLanguage(name string) (*Language, error) {
  // Returns (value, error) tuple
  // error wraps numeric error code for routing
}
```

### Java (JNI/Panama FFM)

**Pattern**: Map Rust exceptions to Java exception classes  
**Implementation**: Use JNI `env->ThrowNew()` to throw Java exceptions with preserved error codes  
**Type Identity**: Java exception classes defined in native interface, thrown across JNI boundary

```java
// Generated wrapper
public static Language getLanguage(String name) throws DownloadError {
  // Native method throws DownloadError directly
}

try {
  getLanguage("unknown");
} catch (DownloadError e) { }  // ✅ Works
```

### C# (P/Invoke)

**Pattern**: Map Rust errors to C# exception types in native wrapper  
**Implementation**: Native wrapper catches Rust errors, throws C# exceptions  
**Type Identity**: C# exception classes defined in binding, matching native error codes

```csharp
// Generated wrapper
public static Language GetLanguage(string name)
{
  // P/Invoke call → native wrapper throws C# DownloadError
}

try {
  GetLanguage("unknown");
} catch (DownloadError e) { }  // ✅ Works
```

### Elixir (Rustler)

**Pattern**: Error tuples with numeric error codes and context  
**Implementation**: NIF returns `{:error, error_code, message}` tuples  
**Type Identity**: Not applicable (Elixir uses tagged tuples, not exception classes)

```elixir
# Generated wrapper
def get_language(name) do
  case _nif_get_language(name) do
    {:ok, lang} -> {:ok, lang}
    {:error, code, msg} -> {:error, {code, msg}}
  end
end
```

### WebAssembly (wasm-bindgen)

**Pattern**: Map Rust errors to JavaScript Error objects with numeric codes  
**Implementation**: Use `wasm_bindgen` `Error` or custom error class  
**Type Identity**: JavaScript Error class with attached error code property

```typescript
// Generated wrapper
export function getLanguage(name: string): Language {
  // Throws Error with .code property for error routing
}

try {
  getLanguage("unknown");
} catch (e) {
  if (e.code === ErrorCode.Download) { }  // ✅ Works
}
```

---

## Error Code Strategy

**Numeric Error Codes**: All Rust errors mapped to numeric codes in range `1000+` for consistency across all bindings.

**Preservation Across FFI**:
- **Python**: Included in exception message or custom attribute
- **Node.js**: Attached as property on Error object
- **Ruby**: Included in exception message
- **Go**: Wrapped in custom error type
- **Java**: Preserved as exception field
- **C#**: Preserved as exception property
- **Elixir**: Returned in error tuple
- **WASM**: Attached as `.code` property

**Example**:
```rust
// Rust core
pub enum Error {
  Download(String),  // Error code: 1001
  // ...
}

// All bindings expose code 1001 in some form
```

---

## Implementation Checklist for New Bindings

When adding a new language binding, ensure:

- [ ] **Exception Class Definitions**: Native module defines exception classes (not public wrapper)
- [ ] **Type Identity**: Exceptions raised by native code use exact same type as public API exports
- [ ] **Error Codes**: All errors include numeric code (1000+) for routing
- [ ] **Re-Export Pattern**: Public API re-exports from native, not duplicate definitions
- [ ] **Documentation**: Document how to catch exceptions in that language
- [ ] **Tests**: E2E test that exceptions can be caught using public API imports
- [ ] **Type Hints**: Include exception types in type stubs (.d.ts, .pyi, RBS, etc.)

---

## Testing Exception Handling

**Standard Test Pattern** (all languages):

```python
# Python example (similar for all languages)
def test_exception_type_identity():
    """Verify exception from native matches public API."""
    from pkg._native import DownloadError as NativeError
    from pkg import DownloadError as PublicError
    assert NativeError is PublicError

def test_exception_catching():
    """Verify exceptions can be caught with public API imports."""
    from pkg import DownloadError
    with pytest.raises(DownloadError):
        pkg.get_language("unknown")

def test_exception_hierarchy():
    """Verify catching base exception catches all variants."""
    from pkg import Error
    with pytest.raises(Error):
        pkg.get_language("unknown")
```

---

## References

- **GitHub Issue #147**: Python exception catching (tree-sitter-language-pack)
- **PyO3 Exception Documentation**: https://pyo3.rs/latest/exception.html
- **NAPI-RS Error Handling**: https://napi.rs/docs/concepts/error-handling
- **Magnus Exception Mapping**: Magnus gem documentation

---

## Maintainer Notes

- This pattern is **critical for usability** — users expect standard exception handling to work
- Each language binding may have **subtle differences** in how identity is preserved (check per-language tests)
- **Error codes must be consistent** across all bindings for error routing and logging
- When updating exception patterns, **update tests in all bindings** to ensure consistency
