//! Kotlin/Native binding generator — Phase 3.
//!
//! Emits Kotlin/Native source that calls into the cbindgen-produced C FFI library
//! via `kotlinx.cinterop.*`. The consumer pattern mirrors the Go and Zig backends:
//! use `config.ffi_prefix()`, `config.ffi_header_name()`, and `config.ffi_lib_name()`
//! as the single source of truth for symbol names and linking directives.

use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;

/// Emit all Kotlin/Native files for the given API surface.
///
/// Returns the full set of generated files:
/// 1. `packages/kotlin-native/src/nativeMain/kotlin/<package>/<Module>.kt` — Kotlin source.
/// 2. `packages/kotlin-native/<crate>.def` — cinterop definition file.
/// 3. `packages/kotlin-native/build.gradle.kts` — minimal Kotlin/Native Gradle build.
pub fn emit(_api: &ApiSurface, _config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    // Full implementation lands in Commit 2.
    Ok(vec![])
}
