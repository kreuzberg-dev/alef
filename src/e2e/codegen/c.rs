//! C e2e test generator using assert.h and a Makefile.
//!
//! Generates `e2e/c/Makefile`, per-category `test_{category}.c` files,
//! a `main.c` test runner, a `test_runner.h` header, and a
//! `download_ffi.sh` script for downloading prebuilt FFI libraries from
//! GitHub releases.

use crate::core::backend::GeneratedFile;
use crate::core::config::ResolvedCrateConfig;
use crate::core::hash::{self, CommentStyle};
use crate::e2e::config::{CallConfig, E2eConfig};
use crate::e2e::escape::{escape_c, sanitize_filename};
use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::{Fixture, FixtureGroup};
use anyhow::Result;
use heck::{ToPascalCase, ToSnakeCase};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// C e2e code generator.
pub struct CCodegen;

/// Returns true when `t` is a primitive C scalar type (uint64_t, int32_t, double,
/// etc.) that should be emitted as a typed local variable rather than a heap
/// `char*` accessor result.
fn is_primitive_c_type(t: &str) -> bool {
    matches!(
        t,
        "uint8_t"
            | "uint16_t"
            | "uint32_t"
            | "uint64_t"
            | "int8_t"
            | "int16_t"
            | "int32_t"
            | "int64_t"
            | "uintptr_t"
            | "intptr_t"
            | "size_t"
            | "ssize_t"
            | "double"
            | "float"
            | "bool"
            | "int"
    )
}

/// Returns `true` when `fields_c_types["{parent}.{field}"]` is the magic
/// sentinel `"skip"` — the C codegen should omit any assertion that touches
/// this field rather than emitting a call to a non-existent FFI function.
fn is_skipped_c_field(fields_c_types: &HashMap<String, String>, parent_snake: &str, field_snake: &str) -> bool {
    let key = format!("{parent_snake}.{field_snake}");
    fields_c_types.get(&key).is_some_and(|t| t == "skip")
}

/// Infer the opaque-handle PascalCase return type for a bare-field accessor.
///
/// Returns `Some(pascal_type)` when the accessor `{prefix}_{parent}_{field}`
/// returns a pointer to an opaque struct (e.g. `SAMPLELLMUsage*`) rather than
/// a `char*` or primitive scalar.
///
/// Detection strategy:
/// 1. Direct lookup `fields_c_types["{parent}.{field}"]` — if present and
///    NOT a primitive AND NOT `char*`, treat as an opaque handle of that
///    PascalCase type.
/// 2. Inferred lookup — when ANY key in `fields_c_types` starts with
///    `"{field}."` (the snake_case of `field` as a parent type), the field
///    must be a struct whose nested fields are mapped. Default the struct
///    type to `field.to_pascal_case()`. This mirrors the fallback used by
///    `emit_nested_accessor` for intermediate segments.
///
/// Returns `None` when the field looks like a `char*` string accessor.
fn infer_opaque_handle_type(
    fields_c_types: &HashMap<String, String>,
    parent_snake_type: &str,
    field_snake: &str,
) -> Option<String> {
    let lookup_key = format!("{parent_snake_type}.{field_snake}");
    if let Some(t) = fields_c_types.get(&lookup_key) {
        if !is_primitive_c_type(t) && t != "char*" {
            return Some(t.clone());
        }
        // Primitive or explicit char* — caller handles those paths.
        return None;
    }
    // Inferred: nested keys exist with `field_snake` as the parent type prefix.
    let nested_prefix = format!("{field_snake}.");
    if fields_c_types.keys().any(|k| k.starts_with(&nested_prefix)) {
        return Some(field_snake.to_pascal_case());
    }
    None
}

/// Try to emit an enum-aware field accessor: when `raw_field`/`resolved_field`
/// is registered in `fields_enum` AND `fields_c_types[parent.field]` resolves
/// to a non-primitive PascalCase type name, treat the accessor return as an
/// opaque enum pointer and convert it to `char*` via the FFI's
/// `{prefix}_{enum_snake}_to_string` accessor.
///
/// Without this, the C codegen would default-declare the accessor result as
/// `char* status = {prefix}_batch_object_status(result);` and string-compare
/// it — but the FFI returns `SAMPLELLMBatchStatus*` (an opaque enum struct
/// pointer), not a C string. The mismatch causes immediate `Abort trap: 6` /
/// `strcmp(NULL,...)` failures in every assertion that targets an enum field.
///
/// Returns `true` when an accessor was emitted (caller must NOT emit the
/// default `char*` declaration). When emitted, the opaque-enum handle is
/// pushed to `intermediate_handles` so the existing cleanup loop frees it via
/// `{prefix}_{enum_snake}_free(...)` after the test body runs.
#[allow(clippy::too_many_arguments)]
fn try_emit_enum_accessor(
    out: &mut String,
    prefix: &str,
    prefix_upper: &str,
    raw_field: &str,
    resolved_field: &str,
    parent_snake_type: &str,
    accessor_fn: &str,
    parent_handle: &str,
    local_var: &str,
    fields_c_types: &HashMap<String, String>,
    fields_enum: &HashSet<String>,
    intermediate_handles: &mut Vec<(String, String)>,
) -> bool {
    if !(fields_enum.contains(raw_field) || fields_enum.contains(resolved_field)) {
        return false;
    }
    let lookup_key = format!("{parent_snake_type}.{resolved_field}");
    let Some(enum_pascal) = fields_c_types.get(&lookup_key) else {
        return false;
    };
    if is_primitive_c_type(enum_pascal) || enum_pascal == "char*" {
        return false;
    }
    let enum_snake = enum_pascal.to_snake_case();
    let handle_var = format!("{local_var}_handle");
    let _ = writeln!(
        out,
        "    {prefix_upper}{enum_pascal}* {handle_var} = {accessor_fn}({parent_handle});"
    );
    let _ = writeln!(out, "    assert({handle_var} != NULL);");
    let _ = writeln!(
        out,
        "    char* {local_var} = {prefix}_{enum_snake}_to_string({handle_var});"
    );
    intermediate_handles.push((handle_var, enum_snake));
    true
}

impl E2eCodegen for CCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        config: &ResolvedCrateConfig,
        type_defs: &[crate::core::ir::TypeDef],
        _enums: &[crate::core::ir::EnumDef],
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(e2e_config.effective_output()).join(lang);

        let mut files = Vec::new();

        // Resolve default call config with overrides.
        let call = &e2e_config.call;
        let overrides = call.overrides.get(lang);
        let result_var = &call.result_var;
        let prefix = overrides
            .and_then(|o| o.prefix.as_ref())
            .cloned()
            .or_else(|| config.ffi.as_ref().and_then(|ffi| ffi.prefix.as_ref()).cloned())
            .unwrap_or_default();
        let header = overrides
            .and_then(|o| o.header.as_ref())
            .cloned()
            .unwrap_or_else(|| config.ffi_header_name());

        // Resolve package config.
        let c_pkg = e2e_config.resolve_package("c");
        // lib_name is the actual Rust library name (for linking)
        let lib_name = config.ffi_lib_name();

        // ffi_pkg_name is the release artifact package name (for downloads).
        // Derived from lib_name (e.g., "liter_llm_ffi" → "liter_llm_ffi") because
        // the publish workflow stages tarballs as "${lib_name}-v${VERSION}-${TRIPLE}.tar.gz".
        // The explicit e2e package name is a fallback for edge cases where the release
        // artifact name differs from the library name.
        let ffi_pkg_name = c_pkg
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| lib_name.clone());

        // Filter active groups (with non-skipped fixtures).
        let active_groups: Vec<(&FixtureGroup, Vec<&Fixture>)> = groups
            .iter()
            .filter_map(|group| {
                let active: Vec<&Fixture> = group
                    .fixtures
                    .iter()
                    .filter(|f| super::should_include_fixture(f, lang, e2e_config))
                    .filter(|f| f.visitor.is_none())
                    .collect();
                if active.is_empty() { None } else { Some((group, active)) }
            })
            .collect();

        // Collect active visitor fixtures (flattened across all groups).
        let visitor_fixtures: Vec<&Fixture> = groups
            .iter()
            .flat_map(|group| group.fixtures.iter())
            .filter(|f| super::should_include_fixture(f, lang, e2e_config))
            .filter(|f| f.visitor.is_some())
            .filter(|f| c_visitor_fixture_has_typed_call(f, e2e_config))
            .collect();

        // Resolve FFI crate path for local repo builds.
        // Default to `../../crates/{name}-ffi` derived from the crate name so that
        // projects with named FFI crates resolve to `../../crates/{name}-ffi/include/`
        // rather than the generic (incorrect) `../../crates/ffi`.
        // When `[crates.output] ffi` is set explicitly, derive the crate path from
        // that value so that renamed FFI crates (e.g. `parser-core-core-ffi`) resolve
        // correctly without any hardcoded special cases.
        let ffi_crate_path = c_pkg
            .as_ref()
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| config.ffi_crate_path());

        // Generate Makefile.
        let mut category_names: Vec<String> = active_groups
            .iter()
            .map(|(g, _)| sanitize_filename(&g.category))
            .collect();
        if !visitor_fixtures.is_empty() {
            category_names.push("visitor".to_string());
        }
        let needs_mock_server = active_groups
            .iter()
            .flat_map(|(_, fixtures)| fixtures.iter())
            .any(|f| f.needs_mock_server());
        files.push(GeneratedFile {
            path: output_base.join("Makefile"),
            content: render_makefile(&category_names, &header, &ffi_crate_path, &lib_name, needs_mock_server),
            generated_header: true,
        });

        // Generate download_ffi.sh for downloading prebuilt FFI from GitHub releases.
        let github_repo = config.github_repo();
        let version = config.resolved_version().unwrap_or_else(|| "0.0.0".to_string());
        files.push(GeneratedFile {
            path: output_base.join("download_ffi.sh"),
            content: render_download_script(&github_repo, &version, &ffi_pkg_name),
            generated_header: true,
        });

        // Generate test_runner.h.
        files.push(GeneratedFile {
            path: output_base.join("test_runner.h"),
            content: render_test_runner_header(&active_groups, &visitor_fixtures),
            generated_header: true,
        });

        // Generate main.c.
        files.push(GeneratedFile {
            path: output_base.join("main.c"),
            content: render_main_c(&active_groups, &visitor_fixtures, &e2e_config.env),
            generated_header: true,
        });

        // Generate .gitignore so locally-built binaries and mock-server pipe
        // artifacts are never accidentally checked in. A committed macOS Mach-O
        // `run_tests` binary will fail Linux CI with `Exec format error`.
        files.push(GeneratedFile {
            path: output_base.join(".gitignore"),
            content: render_gitignore(),
            generated_header: false,
        });

        let field_resolver = FieldResolver::new(
            &e2e_config.fields,
            &e2e_config.fields_optional,
            &e2e_config.result_fields,
            &e2e_config.fields_array,
            &std::collections::HashSet::new(),
        );

        // Generate per-category test files.
        // Each fixture may reference a named call config (fixture.call), so we pass
        // e2e_config to render_test_file so it can resolve per-fixture call settings.
        for (group, active) in &active_groups {
            let filename = format!("test_{}.c", sanitize_filename(&group.category));
            let content = render_test_file(
                &group.category,
                active,
                &header,
                &prefix,
                result_var,
                e2e_config,
                lang,
                &field_resolver,
                config,
                type_defs,
            );
            files.push(GeneratedFile {
                path: output_base.join(filename),
                content,
                generated_header: true,
            });
        }

        // Generate test_visitor.c if there are visitor fixtures.
        if !visitor_fixtures.is_empty() {
            files.push(GeneratedFile {
                path: output_base.join("test_visitor.c"),
                content: render_visitor_test_file(&visitor_fixtures, &header, &prefix, e2e_config, config),
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "c"
    }
}

/// Resolve per-call-config C-specific settings for a given call config and lang.
struct ResolvedCallInfo {
    function_name: String,
    result_type_name: String,
    options_type_name: String,
    client_factory: Option<String>,
    args: Vec<crate::e2e::config::ArgMapping>,
    raw_c_result_type: Option<String>,
    c_free_fn: Option<String>,
    c_engine_factory: Option<String>,
    result_is_option: bool,
    /// When `true`, the FFI signature for this method follows the byte-buffer
    /// out-pointer pattern: `int32_t fn(this, req, uint8_t** out_ptr,
    /// uintptr_t* out_len, uintptr_t* out_cap)`. The C codegen emits out-param
    /// declarations, a status-code check, and `<prefix>_free_bytes` rather
    /// than treating the result as an opaque response handle.
    result_is_bytes: bool,
    streaming: Option<bool>,
    /// Per-language `extra_args` from call overrides — verbatim trailing
    /// arguments appended after the configured `args`. The C codegen passes
    /// `NULL` for absent optional pointers via this mechanism.
    extra_args: Vec<String>,
}

fn resolve_call_info(call: &CallConfig, lang: &str) -> ResolvedCallInfo {
    let overrides = call.overrides.get(lang);
    let function_name = overrides
        .and_then(|o| o.function.as_ref())
        .cloned()
        .unwrap_or_else(|| call.function.clone());
    // Fall back to the *base* (non-C-overridden) function name when no explicit
    // result_type is set.  Using the C-overridden name (e.g. "htm_convert") would
    // produce a doubled-prefix type like `HTMHtmConvert*`; the base name
    // ("convert") yields the correct `HTMConvert*` shape.
    let result_type_name = overrides
        .and_then(|o| o.result_type.as_ref())
        .cloned()
        .unwrap_or_else(|| call.function.to_pascal_case());
    let options_type_name = overrides
        .and_then(|o| o.options_type.as_deref())
        .or(call.options_type.as_deref())
        .unwrap_or_default()
        .to_string();
    let client_factory = overrides.and_then(|o| o.client_factory.as_ref()).cloned();
    let raw_c_result_type = overrides.and_then(|o| o.raw_c_result_type.clone());
    let c_free_fn = overrides.and_then(|o| o.c_free_fn.clone());
    let c_engine_factory = overrides.and_then(|o| o.c_engine_factory.clone());
    let result_is_option = overrides
        .and_then(|o| if o.result_is_option { Some(true) } else { None })
        .unwrap_or(call.result_is_option);
    // result_is_bytes is read from either the call-level config (preferred —
    // the byte-buffer FFI shape is identical across languages that use the
    // same FFI crate) or the per-language override (back-compat with the
    // pattern used by Java / PHP / etc.).
    let result_is_bytes = call.result_is_bytes || overrides.is_some_and(|o| o.result_is_bytes);
    let extra_args = overrides.map(|o| o.extra_args.clone()).unwrap_or_default();
    ResolvedCallInfo {
        function_name,
        result_type_name,
        options_type_name,
        client_factory,
        args: call.args.clone(),
        raw_c_result_type,
        c_free_fn,
        c_engine_factory,
        result_is_option,
        result_is_bytes,
        streaming: call.streaming_enabled(),
        extra_args,
    }
}

/// Resolve call info for a fixture, with fallback to default call's client_factory.
///
/// Named call configs (e.g. `[e2e.calls.embed]`) may not repeat the `client_factory`
/// setting. We fall back to the default `[e2e.call]` override's client_factory so that
/// all methods on the same client use the same pattern.
fn resolve_fixture_call_info(fixture: &Fixture, e2e_config: &E2eConfig, lang: &str) -> ResolvedCallInfo {
    let call = e2e_config.resolve_call_for_fixture(
        fixture.call.as_deref(),
        &fixture.id,
        &fixture.resolved_category(),
        &fixture.tags,
        &fixture.input,
    );
    let mut info = resolve_call_info(call, lang);

    let default_overrides = e2e_config.call.overrides.get(lang);

    // Fallback: if the named call has no client_factory override, inherit from the
    // default call config so all calls use the same client pattern.
    if info.client_factory.is_none() {
        if let Some(factory) = default_overrides.and_then(|o| o.client_factory.as_ref()) {
            info.client_factory = Some(factory.clone());
        }
    }

    // Fallback: if the named call has no c_engine_factory override, inherit from the
    // default call config so all calls use the same engine pattern.
    if info.c_engine_factory.is_none() {
        if let Some(factory) = default_overrides.and_then(|o| o.c_engine_factory.as_ref()) {
            info.c_engine_factory = Some(factory.clone());
        }
    }

    info
}

fn c_visitor_fixture_has_typed_call(fixture: &Fixture, e2e_config: &E2eConfig) -> bool {
    let call = e2e_config.resolve_call_for_fixture(
        fixture.call.as_deref(),
        &fixture.id,
        &fixture.resolved_category(),
        &fixture.tags,
        &fixture.input,
    );
    let info = resolve_call_info(call, "c");
    let has_function = call
        .overrides
        .get("c")
        .and_then(|override_config| override_config.function.as_deref())
        .is_some_and(|function| !function.is_empty());
    has_function && !info.options_type_name.is_empty()
}

mod assertions;
mod call_patterns;
mod project;
mod runner;
mod streaming;
mod test_function;
mod visitor;

use assertions::{build_args_string_c, emit_nested_accessor, render_assertion};
use call_patterns::{render_bytes_test_function, render_engine_factory_test_function};
use project::{render_download_script, render_gitignore, render_makefile};
use runner::{render_main_c, render_test_runner_header};
use streaming::{
    render_c_diagnostic_skip, render_streaming_test_function, resolve_c_client_owner_type, resolve_c_streaming_adapter,
};
use test_function::render_test_function;
use visitor::render_visitor_test_file;

#[allow(clippy::too_many_arguments)]
fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    header: &str,
    prefix: &str,
    result_var: &str,
    e2e_config: &E2eConfig,
    lang: &str,
    field_resolver: &FieldResolver,
    config: &ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
) -> String {
    let mut out = String::new();
    out.push_str(&hash::header(CommentStyle::Block));
    let _ = writeln!(out, "/* E2e tests for category: {category} */");
    let _ = writeln!(out);
    let _ = writeln!(out, "#include <assert.h>");
    let _ = writeln!(out, "#include <stdint.h>");
    let _ = writeln!(out, "#include <string.h>");
    let _ = writeln!(out, "#include <stdio.h>");
    let _ = writeln!(out, "#include <stdlib.h>");
    let _ = writeln!(out, "#include \"{header}\"");
    let _ = writeln!(out, "#include \"test_runner.h\"");
    let _ = writeln!(out);

    for (i, fixture) in fixtures.iter().enumerate() {
        // Visitor fixtures are filtered out before render_test_file is called.
        // This guard is a safety net in case a fixture reaches here unexpectedly.
        if fixture.visitor.is_some() {
            panic!(
                "C e2e generator: visitor pattern not supported for fixture: {}",
                fixture.id
            );
        }

        let call_info = resolve_fixture_call_info(fixture, e2e_config, lang);

        // Effective enum fields for this fixture: merge global e2e_config.fields_enum
        // (HashSet) with the per-call C override's enum_fields (HashMap keys). This
        // mirrors Ruby/Java's pattern: global = always-enum-typed paths; per-call =
        // context-dependent paths (BatchObject.status is BatchStatus, but
        // ResponseObject.status is plain String).
        let mut effective_fields_enum = e2e_config.fields_enum.clone();
        let fixture_call = e2e_config.resolve_call_for_fixture(
            fixture.call.as_deref(),
            &fixture.id,
            &fixture.resolved_category(),
            &fixture.tags,
            &fixture.input,
        );
        if let Some(co) = fixture_call.overrides.get(lang) {
            for k in co.enum_fields.keys() {
                effective_fields_enum.insert(k.clone());
            }
        }

        // Per-call field resolver: overrides the top-level resolver when this call
        // declares its own result_fields / fields / fields_optional / fields_array.
        // Without this, `pages.length` on a `crawl` call would skip because the
        // default `result_fields` (configured for the top-level `scrape` call)
        // does not contain `pages`.
        let per_call_field_resolver = FieldResolver::new(
            e2e_config.effective_fields(fixture_call),
            e2e_config.effective_fields_optional(fixture_call),
            e2e_config.effective_result_fields(fixture_call),
            e2e_config.effective_fields_array(fixture_call),
            &std::collections::HashSet::new(),
        );
        let _ = field_resolver; // top-level resolver retained for compat; per-call wins
        let field_resolver = &per_call_field_resolver;

        render_test_function(
            &mut out,
            fixture,
            prefix,
            &call_info.function_name,
            result_var,
            &call_info.args,
            field_resolver,
            &e2e_config.fields_c_types,
            &effective_fields_enum,
            &call_info.result_type_name,
            &call_info.options_type_name,
            call_info.client_factory.as_deref(),
            call_info.raw_c_result_type.as_deref(),
            call_info.c_free_fn.as_deref(),
            call_info.c_engine_factory.as_deref(),
            call_info.result_is_option,
            call_info.result_is_bytes,
            call_info.streaming,
            &call_info.extra_args,
            config,
            type_defs,
        );
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    out
}

#[allow(clippy::too_many_arguments)]
/// Convert a `serde_json::Value` to a C literal string.
fn json_to_c(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_c(s)),
        serde_json::Value::Bool(true) => "1".to_string(),
        serde_json::Value::Bool(false) => "0".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        other => format!("\"{}\"", escape_c(&other.to_string())),
    }
}

/// Emit a test backend stub.
pub fn emit_test_backend(
    _trait_bridge: &crate::core::config::TraitBridgeConfig,
    _methods: &[&crate::core::ir::MethodDef],
    _fixture: &crate::e2e::fixture::Fixture,
) -> super::TestBackendEmission {
    super::TestBackendEmission::unimplemented("c")
}
