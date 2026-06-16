use crate::codegen::shared::binding_fields;
use crate::core::backend::GeneratedFile;
use crate::core::config::{AdapterConfig, AdapterPattern, Language, ResolvedCrateConfig};
use crate::core::ir::{
    ApiSurface, EnumDef, ErrorDef, FunctionDef, MethodDef, ParamDef, TypeDef, TypeRef, VersionAnnotation,
};
use heck::{ToPascalCase, ToSnakeCase};
use std::collections::HashSet;
use std::path::PathBuf;

use super::descriptions::{
    generate_enum_variant_description, generate_error_variant_description, generate_field_description,
    generate_param_description,
};
use super::doc_cleaning::{clean_doc_inline, demote_headings, extract_param_docs};
use super::formatting::{doc_type_with_optional, escape_table_cell, format_error_phrase, format_field_default};
use super::naming::{
    enum_variant_name, field_name, func_name, lang_code_fence, lang_display_name, lang_slug, type_name,
};
use super::signatures::{MethodSignatureOverride, render_function_signature, render_method_signature_with_override};
use super::sorting::{is_update_type, type_sort_key};
use super::{clean_doc, doc_type, template_env, version_labels};
use crate::docs::examples::{MethodExampleOverride, render_function_example, render_method_example_with_override};

fn language_excludes(config: &ResolvedCrateConfig, lang: Language) -> (HashSet<String>, HashSet<String>) {
    let mut functions: HashSet<String> = config.exclude.functions.iter().cloned().collect();
    let mut types: HashSet<String> = config.exclude.types.iter().cloned().collect();

    match lang {
        Language::Python => {
            if let Some(c) = &config.python {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Node => {
            if let Some(c) = &config.node {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Ruby => {
            if let Some(c) = &config.ruby {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Php => {
            if let Some(c) = &config.php {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Elixir => {
            if let Some(c) = &config.elixir {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Wasm => {
            if let Some(c) = &config.wasm {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Ffi | Language::C => {
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Go => {
            if let Some(c) = &config.go {
                types.extend(c.exclude_types.iter().cloned());
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Java => {
            if let Some(c) = &config.java {
                types.extend(c.exclude_types.iter().cloned());
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Kotlin => {
            if let Some(c) = &config.kotlin {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::KotlinAndroid => {
            if let Some(c) = &config.kotlin_android {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Jni => {
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Swift => {
            if let Some(c) = &config.swift {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Dart => {
            if let Some(c) = &config.dart {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Gleam => {
            if let Some(c) = &config.gleam {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Csharp => {
            if let Some(c) = &config.csharp {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Zig => {
            if let Some(c) = &config.zig {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::R | Language::Rust => {}
    }

    (functions, types)
}

fn extend_excludes(
    functions: &mut HashSet<String>,
    types: &mut HashSet<String>,
    exclude_functions: &[String],
    exclude_types: &[String],
) {
    functions.extend(exclude_functions.iter().cloned());
    types.extend(exclude_types.iter().cloned());
}

// ---------------------------------------------------------------------------
// Per-language doc page
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Per-language doc page
// ---------------------------------------------------------------------------

pub(super) fn generate_lang_doc(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    lang: Language,
    output_dir: &str,
    ffi_prefix: &str,
) -> anyhow::Result<GeneratedFile> {
    let lang_display = lang_display_name(lang);
    let version = &api.version;
    let lang_slug = lang_slug(lang);

    let mut out = String::with_capacity(8192);
    let (exclude_functions, exclude_types) = language_excludes(config, lang);

    out.push_str(&template_env::render(
        "front_matter.jinja",
        minijinja::context! { title => format!("{lang_display} API Reference") },
    ));
    // MD071: blank line required between frontmatter and first heading.
    out.push('\n');
    out.push_str(&template_env::render(
        "version_heading.jinja",
        minijinja::context! { marker => "##", title => format!("{lang_display} API Reference"), version => version },
    ));

    // --- Functions section ---
    let public_fns: Vec<&FunctionDef> = api
        .functions
        .iter()
        .filter(|f| !exclude_functions.contains(&f.name) && (lang == Language::Rust || !f.binding_excluded))
        .collect();
    if !public_fns.is_empty() {
        out.push_str("### Functions\n\n");
        for func in &public_fns {
            out.push_str(&render_function(func, lang, config, api, ffi_prefix));
            out.push_str("\n---\n\n");
        }
    }

    // --- Types section ---
    // Order: ParseOptions, ParseOutput, then rest alphabetical
    // Skip opaque types and *Update types in main section
    let mut types_to_doc: Vec<&TypeDef> = api
        .types
        .iter()
        .filter(|t| {
            !is_update_type(&t.name)
                && !exclude_types.contains(&t.name)
                && (lang == Language::Rust || !t.binding_excluded)
        })
        .collect();

    // Sort: ParseOptions first, ParseOutput second, rest alphabetical
    types_to_doc.sort_by(|a, b| type_sort_key(&a.name).cmp(&type_sort_key(&b.name)));

    if !types_to_doc.is_empty() {
        out.push_str("### Types\n\n");
        for ty in &types_to_doc {
            out.push_str(&render_type(ty, lang, config, api, ffi_prefix));
            out.push_str("\n---\n\n");
        }
    }

    // --- Enums section ---
    let enums_to_doc: Vec<&EnumDef> = api
        .enums
        .iter()
        .filter(|e| !exclude_types.contains(&e.name) && (lang == Language::Rust || !e.binding_excluded))
        .collect();
    if !enums_to_doc.is_empty() {
        out.push_str("### Enums\n\n");
        for en in &enums_to_doc {
            out.push_str(&render_enum(en, lang, ffi_prefix));
            out.push_str("\n---\n\n");
        }
    }

    // --- Errors section ---
    let errors_to_doc: Vec<&ErrorDef> = api
        .errors
        .iter()
        .filter(|e| lang == Language::Rust || !e.binding_excluded)
        .collect();
    if !errors_to_doc.is_empty() {
        out.push_str("### Errors\n\n");
        for err in &errors_to_doc {
            out.push_str(&render_error(err, lang, ffi_prefix));
            out.push_str("\n---\n\n");
        }
    }

    let path = PathBuf::from(format!("{output_dir}/api-{lang_slug}.md"));

    Ok(GeneratedFile {
        path,
        content: out,
        generated_header: false,
    })
}

// ---------------------------------------------------------------------------
// Version annotation rendering
// ---------------------------------------------------------------------------

fn push_version_annotation(out: &mut String, version: &VersionAnnotation) {
    if let Some(ref since) = version.since {
        let since = version_labels::major_minor(since);
        out.push_str(&template_env::render(
            "since_badge.jinja",
            minijinja::context! { since => since },
        ));
        out.push('\n');
        out.push('\n');
    }
    if let Some(ref dep) = version.deprecated {
        let since = dep
            .since
            .as_deref()
            .map(version_labels::major_minor)
            .unwrap_or_default();
        out.push_str(&template_env::render(
            "deprecated_notice.jinja",
            minijinja::context! {
                since => since,
                note => dep.note.as_deref().unwrap_or(""),
            },
        ));
        out.push('\n');
        out.push('\n');
    }
}

// ---------------------------------------------------------------------------
// Function rendering
// ---------------------------------------------------------------------------

fn render_function(
    func: &FunctionDef,
    lang: Language,
    _config: &ResolvedCrateConfig,
    api: &ApiSurface,
    ffi_prefix: &str,
) -> String {
    let mut out = String::new();
    let fn_name = func_name(&func.name, lang, ffi_prefix);

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "####", title => format!("{fn_name}()") },
    ));

    push_version_annotation(&mut out, &func.version);

    // Extract parameter descriptions from the RAW doc string BEFORE cleaning
    let param_docs = extract_param_docs(&func.doc);

    if !func.doc.is_empty() {
        let doc = clean_doc(&func.doc, lang);
        // Demote any embedded headings in the function documentation by 2 levels
        // to ensure they stay nested under the function heading (####).
        let doc = demote_headings(&doc, 2);
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    // Signature
    out.push_str("**Signature:**\n\n");
    let lang_code = lang_code_fence(lang);
    let sig = render_function_signature(func, lang, ffi_prefix);
    out.push_str(&template_env::render(
        "code_block.jinja",
        minijinja::context! { lang_code => lang_code, body => sig },
    ));
    // MD031: blank line required after fenced code block.
    out.push('\n');

    out.push_str(&render_function_example(func, lang, ffi_prefix));

    push_parameters_table(&mut out, &func.params, &param_docs, lang, ffi_prefix);

    push_returns(&mut out, &func.return_type, lang, ffi_prefix);
    push_errors(&mut out, func.error_type.as_deref(), lang);

    let _ = api; // api is available for future use in function rendering
    out
}

fn push_parameters_table(
    out: &mut String,
    params: &[ParamDef],
    param_docs: &std::collections::HashMap<String, String>,
    lang: Language,
    ffi_prefix: &str,
) {
    if params.is_empty() {
        return;
    }
    out.push_str("**Parameters:**\n\n");
    out.push_str("| Name | Type | Required | Description |\n");
    out.push_str("|------|------|----------|-------------|\n");
    for param in params {
        let pname = field_name(&param.name, lang);
        let pty = doc_type_with_optional(&param.ty, lang, param.optional, ffi_prefix);
        let required = if param.optional { "No" } else { "Yes" };
        let pdoc = param_docs
            .get(param.name.as_str())
            .map(|s| clean_doc_inline(s, lang))
            .unwrap_or_else(|| generate_param_description(&param.name, &param.ty));
        out.push_str(&template_env::render(
            "param_row.jinja",
            minijinja::context! {
                name => escape_table_cell(&pname),
                ty => escape_table_cell(&pty),
                required => required,
                doc => escape_table_cell(&pdoc),
            },
        ));
    }
    out.push('\n');
}

fn push_returns(out: &mut String, return_type: &TypeRef, lang: Language, ffi_prefix: &str) {
    push_returns_with_override(out, return_type, None, lang, ffi_prefix);
}

fn push_returns_with_override(
    out: &mut String,
    return_type: &TypeRef,
    return_type_override: Option<&str>,
    lang: Language,
    ffi_prefix: &str,
) {
    if matches!(return_type, TypeRef::Unit) {
        out.push_str("**Returns:** No return value.\n");
        out.push('\n');
        return;
    }

    let ret_ty = return_type_override
        .map(str::to_string)
        .unwrap_or_else(|| doc_type(return_type, lang, ffi_prefix));
    if ret_ty.is_empty() {
        out.push_str("**Returns:** No return value.\n");
        out.push('\n');
    } else {
        out.push_str(&template_env::render(
            "returns.jinja",
            minijinja::context! { ty => ret_ty },
        ));
        out.push('\n');
    }
}

fn push_errors(out: &mut String, error_type: Option<&str>, lang: Language) {
    if let Some(err) = error_type {
        let error_phrase = format_error_phrase(err, lang);
        out.push_str(&template_env::render(
            "errors_phrase.jinja",
            minijinja::context! { phrase => error_phrase },
        ));
        out.push('\n');
    }
}

#[derive(Debug, Clone)]
struct MethodDocsOverride {
    heading_name: String,
    signature: MethodSignatureOverride,
    example: MethodExampleOverride,
    return_type: String,
}

fn streaming_method_docs_override(
    config: &ResolvedCrateConfig,
    method: &MethodDef,
    type_name_str: &str,
    lang: Language,
    ffi_prefix: &str,
) -> Option<MethodDocsOverride> {
    let adapter = config.adapters.iter().find(|adapter| {
        matches!(adapter.pattern, AdapterPattern::Streaming)
            && adapter.owner_type.as_deref() == Some(type_name_str)
            && !adapter.skip_languages.iter().any(|skip| skip == &lang.to_string())
            && streaming_adapter_matches_method(adapter, method)
    })?;
    let item_type = adapter.item_type.as_deref()?;
    let heading_name = streaming_method_name(adapter, method, lang, ffi_prefix);
    let signature =
        streaming_method_signature_override(config, adapter, method, type_name_str, item_type, lang, ffi_prefix);
    let return_type = streaming_return_type(adapter, type_name_str, item_type, lang, ffi_prefix, true);
    let example = MethodExampleOverride {
        body: streaming_example(config, adapter, method, type_name_str, item_type, lang, ffi_prefix),
    };

    Some(MethodDocsOverride {
        heading_name,
        signature,
        example,
        return_type,
    })
}

fn streaming_adapter_matches_method(adapter: &AdapterConfig, method: &MethodDef) -> bool {
    let method_name = method.name.to_snake_case();
    adapter.name.to_snake_case() == method_name
        || adapter
            .core_path
            .rsplit("::")
            .next()
            .is_some_and(|core_name| core_name.to_snake_case() == method_name)
}

fn streaming_adapter_skips_method(
    config: &ResolvedCrateConfig,
    method: &MethodDef,
    type_name_str: &str,
    lang: Language,
) -> bool {
    config.adapters.iter().any(|adapter| {
        matches!(adapter.pattern, AdapterPattern::Streaming)
            && adapter.owner_type.as_deref() == Some(type_name_str)
            && adapter.skip_languages.iter().any(|skip| skip == &lang.to_string())
            && streaming_adapter_matches_method(adapter, method)
    })
}

fn method_visible_in_lang(
    config: &ResolvedCrateConfig,
    method: &MethodDef,
    type_name_str: &str,
    lang: Language,
) -> bool {
    (lang == Language::Rust || !method.binding_excluded)
        && !streaming_adapter_skips_method(config, method, type_name_str, lang)
}

fn streaming_method_name(adapter: &AdapterConfig, method: &MethodDef, lang: Language, ffi_prefix: &str) -> String {
    match lang {
        Language::Csharp => {
            let base = func_name(&adapter.name, lang, ffi_prefix);
            if base.ends_with("Async") {
                base
            } else {
                format!("{base}Async")
            }
        }
        Language::Ffi | Language::C | Language::Jni => streaming_c_start_name(adapter, method, ffi_prefix),
        Language::Zig => adapter.name.to_snake_case(),
        _ => func_name(&adapter.name, lang, ffi_prefix),
    }
}

fn streaming_method_signature_override(
    config: &ResolvedCrateConfig,
    adapter: &AdapterConfig,
    method: &MethodDef,
    type_name_str: &str,
    item_type: &str,
    lang: Language,
    ffi_prefix: &str,
) -> MethodSignatureOverride {
    let name = streaming_method_name(adapter, method, lang, ffi_prefix);
    let return_type = streaming_return_type(adapter, type_name_str, item_type, lang, ffi_prefix, false);
    let signature = match lang {
        Language::Python => Some(format!(
            "async def {}(self, req: {}) -> {}",
            adapter.name.to_snake_case(),
            first_param_type(method, lang, ffi_prefix),
            return_type
        )),
        Language::Rust => Some(format!(
            "fn {}(&self, req: {}) -> {}",
            adapter.name.to_snake_case(),
            first_param_type(method, Language::Rust, ffi_prefix),
            return_type
        )),
        Language::Java => Some(format!(
            "public java.util.stream.Stream<{}> {}({} req) throws {}RsException",
            type_name(item_type, lang, ffi_prefix),
            name,
            first_param_type(method, lang, ffi_prefix),
            java_streaming_exception_prefix(config, ffi_prefix)
        )),
        Language::Csharp => Some(format!(
            "public async IAsyncEnumerable<{}> {}({} req, CancellationToken cancellationToken = default)",
            type_name(item_type, lang, ffi_prefix),
            name,
            first_param_type(method, lang, ffi_prefix)
        )),
        Language::Swift => Some(format!(
            "public func {}(_ req: {}) async throws -> {}",
            name,
            first_param_type(method, lang, ffi_prefix),
            return_type
        )),
        Language::Elixir => Some(format!("def {}(client, req)", adapter.name.to_snake_case())),
        Language::Ffi | Language::C | Language::Jni => Some(streaming_c_start_signature(
            adapter,
            method,
            type_name_str,
            item_type,
            ffi_prefix,
        )),
        Language::Zig => Some(format!(
            "pub fn {}(self: *{}, req: []const u8) {}",
            adapter.name.to_snake_case(),
            type_name(type_name_str, lang, ffi_prefix),
            streaming_zig_return_type(method, item_type, ffi_prefix)
        )),
        _ => None,
    };

    MethodSignatureOverride {
        name: Some(name),
        return_type: Some(return_type),
        signature,
    }
}

fn first_param_type(method: &MethodDef, lang: Language, ffi_prefix: &str) -> String {
    method
        .params
        .first()
        .map(|param| doc_type(&param.ty, lang, ffi_prefix))
        .unwrap_or_else(|| "void".to_string())
}

fn java_streaming_exception_prefix(config: &ResolvedCrateConfig, ffi_prefix: &str) -> String {
    let crate_name = config.name.trim();
    if crate_name.is_empty() {
        ffi_prefix.to_pascal_case()
    } else {
        crate_name.to_pascal_case()
    }
}

fn streaming_return_type(
    adapter: &AdapterConfig,
    type_name_str: &str,
    item_type: &str,
    lang: Language,
    ffi_prefix: &str,
    include_outer_result: bool,
) -> String {
    let item = type_name(item_type, lang, ffi_prefix);
    match lang {
        Language::Python => format!("AsyncIterator[{item}]"),
        Language::Node | Language::Wasm => {
            let iter = format!("{}Iterator", adapter.name.to_pascal_case());
            if include_outer_result {
                format!("Promise<{iter}>")
            } else {
                iter
            }
        }
        Language::Ruby => format!("{}Iterator", adapter.name.to_pascal_case()),
        Language::Php => "array<string>".to_string(),
        Language::Elixir => "{:ok, Stream.t()}".to_string(),
        Language::Go => {
            if include_outer_result {
                format!("(<-chan {item}, error)")
            } else {
                format!("<-chan {item}")
            }
        }
        Language::Java => format!("java.util.stream.Stream<{item}>"),
        Language::Csharp => format!("IAsyncEnumerable<{item}>"),
        Language::Rust => format!("BoxFuture<'_, Result<BoxStream<'static, Result<{item}>>>>"),
        Language::Kotlin => format!("Flow<{item}>"),
        Language::KotlinAndroid => format!("kotlinx.coroutines.flow.Flow<{item}>"),
        Language::Swift => format!("AsyncThrowingStream<{item}, Error>"),
        Language::Dart => format!("Stream<{item}>"),
        Language::Ffi | Language::C | Language::Jni => streaming_c_handle_type(adapter, type_name_str, ffi_prefix),
        Language::Zig => streaming_zig_return_type_placeholder(item_type, ffi_prefix),
        Language::R | Language::Gleam => item,
    }
}

fn streaming_zig_return_type_placeholder(item_type: &str, ffi_prefix: &str) -> String {
    format!("{}Stream", type_name(item_type, Language::Zig, ffi_prefix))
}

fn streaming_zig_return_type(method: &MethodDef, item_type: &str, ffi_prefix: &str) -> String {
    let stream_type = streaming_zig_return_type_placeholder(item_type, ffi_prefix);
    let error_type = method
        .error_type
        .as_deref()
        .map(|error| type_name(error, Language::Zig, ffi_prefix))
        .unwrap_or_else(|| "anyerror".to_string());
    format!("({error_type}||error{{OutOfMemory}})!{stream_type}")
}

fn streaming_c_start_name(adapter: &AdapterConfig, method: &MethodDef, ffi_prefix: &str) -> String {
    let _ = method;
    format!(
        "{}_{}_{}_start",
        ffi_prefix.to_snake_case(),
        adapter.owner_type.as_deref().unwrap_or_default().to_snake_case(),
        adapter.name.to_snake_case()
    )
}

fn streaming_c_handle_type(adapter: &AdapterConfig, type_name_str: &str, ffi_prefix: &str) -> String {
    format!(
        "struct {}{}{}{}StreamHandle *",
        ffi_prefix.to_uppercase(),
        ffi_prefix.to_pascal_case(),
        type_name_str.to_pascal_case(),
        adapter.name.to_pascal_case()
    )
}

fn streaming_c_start_signature(
    adapter: &AdapterConfig,
    method: &MethodDef,
    type_name_str: &str,
    item_type: &str,
    ffi_prefix: &str,
) -> String {
    let handle_type = streaming_c_handle_type(adapter, type_name_str, ffi_prefix);
    let start_name = streaming_c_start_name(adapter, method, ffi_prefix);
    let owner_type = format!("{}{}", ffi_prefix.to_uppercase(), type_name_str.to_pascal_case());
    let request_type = method
        .params
        .first()
        .map(|param| match &param.ty {
            TypeRef::Named(name) => format!("{}{}", ffi_prefix.to_uppercase(), name.to_pascal_case()),
            _ => "void".to_string(),
        })
        .unwrap_or_else(|| "void".to_string());
    let _ = item_type;
    format!("{handle_type} {start_name}(const {owner_type} *client, const {request_type} *req);")
}

fn streaming_example(
    config: &ResolvedCrateConfig,
    adapter: &AdapterConfig,
    method: &MethodDef,
    type_name_str: &str,
    item_type: &str,
    lang: Language,
    ffi_prefix: &str,
) -> String {
    let method_name = streaming_method_name(adapter, method, lang, ffi_prefix);
    let req_value = streaming_request_sample(method, lang, ffi_prefix);
    let item = type_name(item_type, lang, ffi_prefix);
    match lang {
        Language::Python => {
            format!("stream = instance.{method_name}({req_value})\nasync for chunk in stream:\n    print(chunk)")
        }
        Language::Node => format!(
            "const stream = await instance.{method_name}({req_value});\nfor await (const chunk of stream) {{\n  console.log(chunk);\n}}"
        ),
        Language::Wasm => format!(
            "const stream = await instance.{method_name}({req_value});\nwhile (true) {{\n  const chunk = await stream.next();\n  if (chunk === null) break;\n  console.log(chunk);\n}}"
        ),
        Language::Ruby => {
            format!("stream = instance.{method_name}({req_value})\nstream.each do |chunk|\n  puts chunk\nend")
        }
        Language::Php => {
            format!("foreach ($instance->{method_name}({req_value}) as $chunk) {{\n    var_dump($chunk);\n}}")
        }
        Language::Elixir => format!(
            "{{:ok, stream}} = {}.{}(instance, {req_value})\nEnum.each(stream, &IO.inspect/1)",
            config.name.to_pascal_case(),
            adapter.name.to_snake_case()
        ),
        Language::Go => format!(
            "stream, err := instance.{method_name}({req_value})\nif err != nil {{\n    return err\n}}\nfor chunk := range stream {{\n    fmt.Println(chunk)\n}}"
        ),
        Language::Java => format!(
            "try (var stream = instance.{method_name}({req_value})) {{\n    stream.forEach(System.out::println);\n}}"
        ),
        Language::Csharp => format!(
            "await foreach (var chunk in instance.{method_name}({req_value})) {{\n    Console.WriteLine(chunk);\n}}"
        ),
        Language::Rust => format!(
            "let mut stream = instance.{}({req_value}).await?;\nwhile let Some(chunk) = stream.next().await {{\n    let chunk = chunk?;\n    println!(\"{{chunk:?}}\");\n}}",
            adapter.name.to_snake_case()
        ),
        Language::Kotlin | Language::KotlinAndroid => {
            format!("instance.{method_name}({req_value}).collect {{ chunk ->\n    println(chunk)\n}}")
        }
        Language::Swift => format!(
            "let stream = try await instance.{method_name}({req_value})\nfor try await chunk in stream {{\n    print(chunk)\n}}"
        ),
        Language::Dart => {
            format!("await for (final chunk in instance.{method_name}({req_value})) {{\n  print(chunk);\n}}")
        }
        Language::Ffi | Language::C | Language::Jni => {
            streaming_c_example(adapter, method, type_name_str, item_type, ffi_prefix)
        }
        Language::Zig => format!(
            "var stream = try instance.{method_name}(\"{{}}\");\ndefer stream.deinit();\nwhile (try stream.next()) |chunk| {{\n    _ = chunk;\n}}"
        ),
        Language::R | Language::Gleam => {
            format!("stream <- instance.{method_name}({req_value})\n# Iterate over {item} chunks.")
        }
    }
}

fn streaming_request_sample(method: &MethodDef, lang: Language, ffi_prefix: &str) -> String {
    let Some(param) = method.params.first() else {
        return String::new();
    };
    match &param.ty {
        TypeRef::Named(name) => {
            let ty = type_name(name, lang, ffi_prefix);
            match lang {
                Language::Python | Language::Kotlin | Language::KotlinAndroid | Language::Swift | Language::Dart => {
                    format!("{ty}()")
                }
                Language::Node | Language::Wasm | Language::Java | Language::Csharp | Language::Php => {
                    format!("new {ty}()")
                }
                Language::Ruby => format!("{ty}.new"),
                Language::Go => format!("{ty}{{}}"),
                Language::Rust => format!("{ty}::default()"),
                Language::Zig => "\"{}\"".to_string(),
                Language::Elixir => "%{}".to_string(),
                Language::Ffi | Language::C | Language::Jni => "req".to_string(),
                Language::R | Language::Gleam => "{}".to_string(),
            }
        }
        _ => "req".to_string(),
    }
}

fn streaming_c_example(
    adapter: &AdapterConfig,
    method: &MethodDef,
    type_name_str: &str,
    item_type: &str,
    ffi_prefix: &str,
) -> String {
    let start_name = streaming_c_start_name(adapter, method, ffi_prefix);
    let handle_type = streaming_c_handle_type(adapter, type_name_str, ffi_prefix);
    let prefix = ffi_prefix.to_snake_case();
    let owner = type_name_str.to_snake_case();
    let method_name = adapter.name.to_snake_case();
    let item_c = format!("{}{}", ffi_prefix.to_uppercase(), item_type.to_pascal_case());
    let item_free = format!("{}_{}_free", prefix, item_type.to_snake_case());
    format!(
        "{handle_type} stream = {start_name}(instance, req);\nwhile (stream != NULL) {{\n    {item_c} *chunk = {prefix}_{owner}_{method_name}_next(stream);\n    if (chunk == NULL) {{\n        break;\n    }}\n    {item_free}(chunk);\n}}\n{prefix}_{owner}_{method_name}_free(stream);"
    )
}

fn render_method(
    method: &MethodDef,
    type_name_str: &str,
    lang: Language,
    config: &ResolvedCrateConfig,
    ffi_prefix: &str,
) -> String {
    let mut out = String::new();
    let docs_override = streaming_method_docs_override(config, method, type_name_str, lang, ffi_prefix);
    let mname = docs_override
        .as_ref()
        .map(|override_| override_.heading_name.clone())
        .unwrap_or_else(|| func_name(&method.name, lang, ffi_prefix));

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "######", title => format!("{mname}()") },
    ));

    push_version_annotation(&mut out, &method.version);

    let param_docs = extract_param_docs(&method.doc);

    let doc = clean_doc(&method.doc, lang);
    // Demote embedded headings under the generated method heading (######).
    let doc = demote_headings(&doc, 4);
    if !doc.is_empty() {
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    let lang_code = lang_code_fence(lang);
    let sig = render_method_signature_with_override(
        method,
        type_name_str,
        lang,
        ffi_prefix,
        docs_override.as_ref().map(|override_| &override_.signature),
    );
    out.push_str("**Signature:**\n\n");
    out.push_str(&template_env::render(
        "code_block.jinja",
        minijinja::context! { lang_code => lang_code, body => sig },
    ));
    // MD031: blank line required after fenced code block.
    out.push('\n');

    out.push_str(&render_method_example_with_override(
        method,
        type_name_str,
        lang,
        ffi_prefix,
        docs_override.as_ref().map(|override_| &override_.example),
    ));
    push_parameters_table(&mut out, &method.params, &param_docs, lang, ffi_prefix);
    push_returns_with_override(
        &mut out,
        &method.return_type,
        docs_override.as_ref().map(|override_| override_.return_type.as_str()),
        lang,
        ffi_prefix,
    );
    push_errors(&mut out, method.error_type.as_deref(), lang);

    out
}

// ---------------------------------------------------------------------------
// Type rendering
// ---------------------------------------------------------------------------

fn render_type(
    ty: &TypeDef,
    lang: Language,
    config: &ResolvedCrateConfig,
    api: &ApiSurface,
    ffi_prefix: &str,
) -> String {
    let mut out = String::new();
    let tname = type_name(&ty.name, lang, ffi_prefix);

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "####", title => tname },
    ));

    push_version_annotation(&mut out, &ty.version);

    let doc = clean_doc(&ty.doc, lang);
    // Demote any embedded headings in the type documentation by 2 levels
    // to ensure they stay nested under the type heading (####).
    let doc = demote_headings(&doc, 2);
    if !doc.is_empty() {
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    // Fields table (only for non-opaque types or opaque types with documented fields)
    let fields: Vec<_> = if lang == Language::Rust {
        ty.fields.iter().collect()
    } else {
        binding_fields(&ty.fields).collect()
    };
    if !ty.is_opaque && !fields.is_empty() {
        out.push('\n');
        out.push_str("| Field | Type | Default | Description |\n");
        out.push_str("|-------|------|---------|-------------|\n");
        for field in fields {
            let fname = field_name(&field.name, lang);
            let fty = doc_type_with_optional(&field.ty, lang, field.optional, ffi_prefix);
            let fdefault = format_field_default(field, lang, api, ffi_prefix);
            let fdoc = {
                let raw = clean_doc_inline(&field.doc, lang);
                if raw.is_empty() {
                    generate_field_description(&field.name, &field.ty)
                } else {
                    raw
                }
            };
            out.push_str(&template_env::render(
                "field_row.jinja",
                minijinja::context! {
                    name => escape_table_cell(&fname),
                    ty => escape_table_cell(&fty),
                    default => escape_table_cell(&fdefault),
                    doc => escape_table_cell(&fdoc),
                },
            ));
        }
        out.push('\n');
    }

    // Methods (called "Functions" in Elixir)
    let methods: Vec<_> = ty
        .methods
        .iter()
        .filter(|method| method_visible_in_lang(config, method, &ty.name, lang))
        .collect();
    if !methods.is_empty() {
        let methods_heading = if lang == Language::Elixir {
            "Functions"
        } else {
            "Methods"
        };
        out.push_str(&template_env::render(
            "heading.jinja",
            minijinja::context! { marker => "#####", title => methods_heading },
        ));
        for method in methods {
            out.push_str(&render_method(method, &ty.name, lang, config, ffi_prefix));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Enum rendering
// ---------------------------------------------------------------------------

fn render_enum(en: &EnumDef, lang: Language, ffi_prefix: &str) -> String {
    let mut out = String::new();
    let ename = type_name(&en.name, lang, ffi_prefix);

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "####", title => ename },
    ));

    push_version_annotation(&mut out, &en.version);

    let doc = clean_doc(&en.doc, lang);
    // Demote any embedded headings in the enum documentation by 2 levels
    // to ensure they stay nested under the enum heading (####).
    let doc = demote_headings(&doc, 2);
    if !doc.is_empty() {
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    out.push_str("| Value | Description |\n");
    out.push_str("|-------|-------------|\n");
    for variant in &en.variants {
        let vname = enum_variant_name(&variant.name, lang, ffi_prefix);
        let mut vdoc = if !variant.doc.is_empty() {
            clean_doc_inline(&variant.doc, lang)
        } else {
            generate_enum_variant_description(&variant.name)
        };
        // Append field info for data variants
        let variant_fields: Vec<_> = if lang == Language::Rust {
            variant.fields.iter().collect()
        } else {
            binding_fields(&variant.fields).collect()
        };
        if !variant_fields.is_empty() {
            let fields_desc: Vec<String> = variant_fields
                .into_iter()
                .map(|f| {
                    let fname = field_name(&f.name, lang);
                    let fty = doc_type(&f.ty, lang, ffi_prefix);
                    format!("`{fname}`: `{fty}`")
                })
                .collect();
            vdoc = format!("{vdoc} — Fields: {}", fields_desc.join(", "));
        }
        // Inline version annotations into the description cell (block-level elements
        // cannot appear inside a Markdown table row).
        if let Some(ref since) = variant.version.since {
            let since = version_labels::major_minor(since);
            vdoc = format!("{vdoc} — **Since:** `v{since}`");
        }
        if let Some(ref dep) = variant.version.deprecated {
            let dep_note = match (&dep.since, &dep.note) {
                (Some(s), Some(n)) => format!("Deprecated since `v{}`: {n}", version_labels::major_minor(s)),
                (Some(s), None) => format!("Deprecated since `v{}`", version_labels::major_minor(s)),
                (None, Some(n)) => format!("Deprecated: {n}"),
                (None, None) => "Deprecated".to_string(),
            };
            vdoc = format!("{vdoc} — {dep_note}");
        }
        out.push_str(&template_env::render(
            "variant_row.jinja",
            minijinja::context! { name => escape_table_cell(&vname), doc => escape_table_cell(&vdoc) },
        ));
    }
    out.push('\n');

    out
}

// ---------------------------------------------------------------------------
// Error rendering
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Error rendering
// ---------------------------------------------------------------------------

fn render_error(err: &ErrorDef, lang: Language, ffi_prefix: &str) -> String {
    let mut out = String::new();
    let ename = type_name(&err.name, lang, ffi_prefix);

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "####", title => &ename },
    ));

    let doc = clean_doc(&err.doc, lang);
    // Demote any embedded headings in the error documentation by 2 levels
    // to ensure they stay nested under the error heading (####).
    let doc = demote_headings(&doc, 2);
    if !doc.is_empty() {
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    // For Node/WASM, note that errors are plain Error objects
    if matches!(lang, Language::Node | Language::Wasm) {
        out.push_str("Errors are thrown as plain `Error` objects with descriptive messages.\n\n");
    }

    // For Python, render as exception class hierarchy
    if lang == Language::Python {
        out.push_str(&template_env::render(
            "base_class.jinja",
            minijinja::context! { name => &ename },
        ));
        out.push('\n');
        out.push_str("| Exception | Description |\n");
        out.push_str("|-----------|-------------|\n");
        for variant in &err.variants {
            let vname = variant.name.to_pascal_case();
            let vdoc = if !variant.doc.is_empty() {
                clean_doc_inline(&variant.doc, lang)
            } else if let Some(tmpl) = &variant.message_template {
                clean_doc_inline(tmpl, lang)
            } else {
                generate_error_variant_description(&variant.name)
            };
            out.push_str(&template_env::render(
                "exception_row.jinja",
                minijinja::context! {
                    variant => escape_table_cell(&vname),
                    error => escape_table_cell(&ename),
                    doc => escape_table_cell(&vdoc),
                },
            ));
        }
    } else {
        out.push('\n');
        out.push_str("| Variant | Description |\n");
        out.push_str("|---------|-------------|\n");
        for variant in &err.variants {
            let vname = enum_variant_name(&variant.name, lang, ffi_prefix);
            let vdoc = if !variant.doc.is_empty() {
                clean_doc_inline(&variant.doc, lang)
            } else if let Some(tmpl) = &variant.message_template {
                clean_doc_inline(tmpl, lang)
            } else {
                generate_error_variant_description(&variant.name)
            };
            out.push_str(&template_env::render(
                "variant_row.jinja",
                minijinja::context! { name => escape_table_cell(&vname), doc => escape_table_cell(&vdoc) },
            ));
        }
    }
    out.push('\n');

    out
}

// ---------------------------------------------------------------------------
// Configuration page
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Configuration page
