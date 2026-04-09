use crate::type_map::{go_optional_type, go_type};
use alef_codegen::naming::to_go_name;
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use heck::{ToPascalCase, ToSnakeCase};
use std::fmt::Write;
use std::path::PathBuf;

/// Returns true if a field is a tuple struct positional field (e.g., `_0`, `_1`, `0`, `1`).
/// Go structs require named fields, so these must be skipped.
fn is_tuple_field(field: &FieldDef) -> bool {
    (field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit()))
        || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
}

pub struct GoBackend;

impl GoBackend {
    /// Extract the package name from module path (last segment).
    /// Sanitize by removing hyphens and converting to lowercase.
    fn package_name(module_path: &str) -> String {
        module_path
            .split('/')
            .next_back()
            .unwrap_or("kreuzberg")
            .replace('-', "")
            .to_lowercase()
    }
}

impl Backend for GoBackend {
    fn name(&self) -> &str {
        "go"
    }

    fn language(&self) -> Language {
        Language::Go
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: true,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let module_path = config.go_module();
        let pkg_name = config
            .go
            .as_ref()
            .and_then(|g| g.package_name.clone())
            .unwrap_or_else(|| Self::package_name(&module_path));
        let ffi_prefix = config.ffi_prefix();

        let output_dir = resolve_output_dir(config.output.go.as_ref(), &config.crate_config.name, "packages/go/");

        let ffi_lib_name = config.ffi_lib_name();
        let ffi_header = config.ffi_header_name();
        let content = strip_trailing_whitespace(&gen_go_file(api, &ffi_prefix, &pkg_name, &ffi_lib_name, &ffi_header));

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Go)?;

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("binding.go"),
            content,
            generated_header: true,
        }])
    }

    /// Go bindings are already the public API (single .go file wrapping C FFI).
    /// This returns empty since the binding.go file serves as both the FFI layer
    /// and the high-level public API for consumers.
    fn generate_public_api(&self, _api: &ApiSurface, _config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        // Go's binding.go IS the public API — no additional wrapper needed.
        Ok(vec![])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "go",
            crate_suffix: "",
            depends_on_ffi: true,
            post_build: vec![],
        })
    }
}

/// Strip trailing whitespace from every line and ensure the file ends with a single newline.
fn strip_trailing_whitespace(content: &str) -> String {
    let mut result: String = content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Generate the complete Go binding file wrapping the C FFI layer.
fn gen_go_file(api: &ApiSurface, ffi_prefix: &str, pkg_name: &str, _ffi_lib_name: &str, ffi_header: &str) -> String {
    let mut out = String::with_capacity(4096);

    // Package header and imports
    writeln!(out, "package {}\n", pkg_name).ok();
    writeln!(out, "/*\n#include \"{}\"\n*/\nimport \"C\"", ffi_header).ok();
    writeln!(out).ok();
    // Determine imports — add "errors" if we have error types
    if api.errors.is_empty() {
        writeln!(out, "import (\n    \"encoding/json\"\n    \"fmt\"\n    \"unsafe\"\n)\n").ok();
    } else {
        writeln!(
            out,
            "import (\n    \"encoding/json\"\n    \"errors\"\n    \"fmt\"\n    \"unsafe\"\n)\n"
        )
        .ok();
    }

    // Error helper functions
    writeln!(out, "{}\n", gen_last_error_helper(ffi_prefix)).ok();

    // Generate error types (sentinel errors + structured error type)
    for error in &api.errors {
        writeln!(out, "{}\n", alef_codegen::error_gen::gen_go_error_types(error)).ok();
    }

    // Generate enum types and constants
    for enum_def in &api.enums {
        writeln!(out, "{}\n", gen_enum_type(enum_def)).ok();
    }

    // Generate struct types
    for typ in &api.types {
        writeln!(out, "{}\n", gen_struct_type(typ)).ok();
        // Generate functional options pattern if type has defaults
        if typ.has_default {
            writeln!(out, "{}\n", gen_config_options(typ)).ok();
        }
    }

    // Generate free function wrappers
    for func in &api.functions {
        writeln!(out, "{}\n", gen_function_wrapper(func, ffi_prefix)).ok();
    }

    // Generate struct methods
    for typ in &api.types {
        for method in &typ.methods {
            writeln!(out, "{}\n", gen_method_wrapper(typ, method, ffi_prefix)).ok();
        }
    }

    out
}

/// Generate the lastError() helper function.
fn gen_last_error_helper(ffi_prefix: &str) -> String {
    format!(
        "// lastError retrieves the last error from the FFI layer.\nfunc lastError() error {{\n    \
         code := int32(C.{}_last_error_code())\n    \
         if code == 0 {{\n        return nil\n    }}\n    \
         ctx := C.{}_last_error_context()\n    \
         message := C.GoString(ctx)\n    \
         C.{}_free_string(ctx)\n    \
         return fmt.Errorf(\"[%d] %s\", code, message)\n\
         }}",
        ffi_prefix, ffi_prefix, ffi_prefix
    )
}

/// Generate a Go enum type definition.
///
/// For unit enums (all variants have no fields): generates `type X string` with constants.
/// For data enums (any variant has fields): generates a flattened Go struct with all
/// variant fields collected and deduplicated, using pointer types for fields not present
/// in every variant.
fn gen_enum_type(enum_def: &EnumDef) -> String {
    let is_data_enum = enum_def.variants.iter().any(|v| !v.fields.is_empty());

    if is_data_enum {
        gen_data_enum_type(enum_def)
    } else {
        gen_unit_enum_type(enum_def)
    }
}

/// Generate a Go unit enum as `type X string` with const block.
fn gen_unit_enum_type(enum_def: &EnumDef) -> String {
    let mut out = String::with_capacity(1024);

    if !enum_def.doc.is_empty() {
        for line in enum_def.doc.lines() {
            writeln!(out, "// {}", line.trim()).ok();
        }
    } else {
        writeln!(out, "// {} is an enumeration type.", enum_def.name).ok();
    }
    writeln!(out, "type {} string", enum_def.name).ok();
    writeln!(out).ok();
    writeln!(out, "const (").ok();

    for variant in &enum_def.variants {
        let const_name = format!("{}{}", enum_def.name, variant.name.to_pascal_case());
        let variant_snake = variant.name.to_snake_case();
        if !variant.doc.is_empty() {
            for line in variant.doc.lines() {
                writeln!(out, "    // {}", line.trim()).ok();
            }
        }
        writeln!(out, "    {} {} = \"{}\"", const_name, enum_def.name, variant_snake).ok();
    }

    writeln!(out, ")").ok();
    out
}

/// Generate a Go data enum as a flattened struct with JSON tags.
///
/// All fields from all variants are collected and deduplicated by name.
/// Fields that don't appear in every variant are made optional (pointer type).
fn gen_data_enum_type(enum_def: &EnumDef) -> String {
    let mut out = String::with_capacity(1024);

    // Collect variant names for the doc comment
    let variant_names: Vec<&str> = enum_def.variants.iter().map(|v| v.name.as_str()).collect();
    let total_variants = enum_def.variants.len();

    if !enum_def.doc.is_empty() {
        for line in enum_def.doc.lines() {
            writeln!(out, "// {}", line.trim()).ok();
        }
    } else {
        writeln!(
            out,
            "// {} is a tagged union type (discriminated by JSON tag).",
            enum_def.name
        )
        .ok();
    }
    writeln!(out, "// Variants: {}", variant_names.join(", ")).ok();
    writeln!(out, "type {} struct {{", enum_def.name).ok();

    // Collect and deduplicate fields across all variants.
    // Track: field name -> (FieldDef, count of variants containing it)
    let mut seen_fields: Vec<(String, FieldDef, usize)> = Vec::new();

    for variant in &enum_def.variants {
        for field in &variant.fields {
            if is_tuple_field(field) {
                continue;
            }
            if let Some(entry) = seen_fields.iter_mut().find(|(name, _, _)| *name == field.name) {
                entry.2 += 1;
            } else {
                seen_fields.push((field.name.clone(), field.clone(), 1));
            }
        }
    }

    for (field_name, field, count) in &seen_fields {
        // A field is optional if it's already marked optional OR if it doesn't appear in all variants
        let is_optional = field.optional || *count < total_variants;

        let field_type = if is_optional {
            go_optional_type(&field.ty)
        } else {
            go_type(&field.ty)
        };

        let json_tag = if is_optional {
            format!("json:\"{},omitempty\"", field_name)
        } else {
            format!("json:\"{}\"", field_name)
        };

        if !field.doc.is_empty() {
            for line in field.doc.lines() {
                writeln!(out, "    // {}", line.trim()).ok();
            }
        }
        writeln!(out, "    {} {} `{}`", to_go_name(field_name), field_type, json_tag).ok();
    }

    writeln!(out, "}}").ok();
    out
}

/// Generate a Go struct type definition with json tags for marshaling.
fn gen_struct_type(typ: &TypeDef) -> String {
    let mut out = String::with_capacity(1024);

    if !typ.doc.is_empty() {
        for line in typ.doc.lines() {
            writeln!(out, "// {}", line.trim()).ok();
        }
    } else {
        writeln!(out, "// {} is a type.", typ.name).ok();
    }
    writeln!(out, "type {} struct {{", typ.name).ok();

    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }

        let field_type = if field.optional {
            go_optional_type(&field.ty)
        } else {
            go_type(&field.ty)
        };

        // Determine json tag - use omitempty for optional fields
        let json_tag = if field.optional {
            format!("json:\"{},omitempty\"", field.name)
        } else {
            format!("json:\"{}\"", field.name)
        };

        if !field.doc.is_empty() {
            for line in field.doc.lines() {
                writeln!(out, "    // {}", line.trim()).ok();
            }
        }
        writeln!(out, "    {} {} `{}`", to_go_name(&field.name), field_type, json_tag).ok();
    }

    writeln!(out, "}}").ok();
    out
}

/// Generate a wrapper function for a free function.
fn gen_function_wrapper(func: &FunctionDef, ffi_prefix: &str) -> String {
    let mut out = String::with_capacity(2048);

    let func_go_name = to_go_name(&func.name);

    if !func.doc.is_empty() {
        for line in func.doc.lines() {
            writeln!(out, "// {}", line.trim()).ok();
        }
    } else {
        writeln!(out, "// {} calls the FFI function.", func_go_name).ok();
    }

    let return_type = if func.error_type.is_some() {
        if matches!(func.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("(*{}, error)", go_type(&func.return_type))
        }
    } else if matches!(func.return_type, TypeRef::Unit) {
        "".to_string()
    } else {
        format!("*{}", go_type(&func.return_type))
    };

    let func_snake = func.name.to_snake_case();
    let ffi_name = format!("C.{}_{}", ffi_prefix, func_snake);

    write!(out, "func {}(", func_go_name).ok();

    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let param_type = if p.optional {
                go_optional_type(&p.ty)
            } else {
                go_type(&p.ty)
            };
            format!("{} {}", p.name, param_type)
        })
        .collect();
    write!(out, "{}", params.join(", ")).ok();

    if return_type.is_empty() {
        writeln!(out, ") {{").ok();
    } else {
        writeln!(out, ") {} {{", return_type).ok();
    }

    // Convert parameters
    for param in &func.params {
        write!(out, "{}", gen_param_to_c(param)).ok();
    }

    // Build the C call with converted parameters
    let c_params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("c{}", p.name.to_pascal_case()))
        .collect();

    let c_call = format!("{}({})", ffi_name, c_params.join(", "));

    // Handle result and error
    if func.error_type.is_some() {
        if matches!(func.return_type, TypeRef::Unit) {
            writeln!(out, "    {}", c_call).ok();
            writeln!(out, "    return lastError()").ok();
        } else {
            writeln!(out, "    ptr := {}", c_call).ok();
            writeln!(out, "    if err := lastError(); err != nil {{").ok();
            // Free the pointer if non-nil even on error, to avoid leaks
            if matches!(
                func.return_type,
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
            ) {
                writeln!(out, "        if ptr != nil {{").ok();
                writeln!(out, "            C.{}_free_string(ptr)", ffi_prefix).ok();
                writeln!(out, "        }}").ok();
            }
            writeln!(out, "        return nil, err").ok();
            writeln!(out, "    }}").ok();
            // Free the FFI-allocated string after unmarshaling
            if matches!(
                func.return_type,
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
            ) {
                writeln!(out, "    defer C.{}_free_string(ptr)", ffi_prefix).ok();
            }
            writeln!(out, "    return unmarshal{}(ptr), nil", type_name(&func.return_type)).ok();
        }
    } else if matches!(func.return_type, TypeRef::Unit) {
        writeln!(out, "    {}", c_call).ok();
    } else {
        writeln!(out, "    ptr := {}", c_call).ok();
        // Add defer free for C string returns
        if matches!(
            func.return_type,
            TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
        ) {
            writeln!(out, "    defer C.{}_free_string(ptr)", ffi_prefix).ok();
        }
        writeln!(out, "    return unmarshal{}(ptr)", type_name(&func.return_type)).ok();
    }

    writeln!(out, "}}").ok();
    out
}

/// Generate a wrapper method for a struct method.
fn gen_method_wrapper(typ: &TypeDef, method: &MethodDef, ffi_prefix: &str) -> String {
    let mut out = String::with_capacity(2048);

    let method_go_name = to_go_name(&method.name);

    if !method.doc.is_empty() {
        for line in method.doc.lines() {
            writeln!(out, "// {}", line.trim()).ok();
        }
    } else {
        writeln!(out, "// {} is a method.", method_go_name).ok();
    }

    let return_type = if method.error_type.is_some() {
        if matches!(method.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("(*{}, error)", go_type(&method.return_type))
        }
    } else if matches!(method.return_type, TypeRef::Unit) {
        "".to_string()
    } else {
        format!("*{}", go_type(&method.return_type))
    };

    let receiver_name = "r";
    let receiver_type = &typ.name;

    // Determine receiver (pointer)
    write!(out, "func ({} *{}) {}(", receiver_name, receiver_type, method_go_name).ok();

    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let param_type = if p.optional {
                go_optional_type(&p.ty)
            } else {
                go_type(&p.ty)
            };
            format!("{} {}", p.name, param_type)
        })
        .collect();
    write!(out, "{}", params.join(", ")).ok();

    if return_type.is_empty() {
        writeln!(out, ") {{").ok();
    } else {
        writeln!(out, ") {} {{", return_type).ok();
    }

    if method.is_async {
        // Generate async version with channels
        let result_type = if matches!(method.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("*{}", go_type(&method.return_type))
        };

        writeln!(out, "    resultCh := make(chan {}, 1)", result_type).ok();
        writeln!(out, "    errCh := make(chan error, 1)").ok();
        writeln!(out, "    go func() {{").ok();

        // Call sync version
        let call_args = method
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        if return_type.is_empty() {
            writeln!(
                out,
                "        err := {}.{}Sync({})",
                receiver_name, method.name, call_args
            )
            .ok();
            writeln!(out, "        if err != nil {{").ok();
            writeln!(out, "            errCh <- err").ok();
            writeln!(out, "        }} else {{").ok();
            writeln!(out, "            errCh <- nil").ok();
            writeln!(out, "        }}").ok();
        } else {
            writeln!(
                out,
                "        result, err := {}.{}Sync({})",
                receiver_name, method.name, call_args
            )
            .ok();
            writeln!(out, "        if err != nil {{").ok();
            writeln!(out, "            errCh <- err").ok();
            writeln!(out, "        }} else {{").ok();
            writeln!(out, "            resultCh <- result").ok();
            writeln!(out, "        }}").ok();
        }

        writeln!(out, "    }}()").ok();
        writeln!(out, "    return resultCh, errCh").ok();
    } else {
        // Synchronous method - just convert params and call FFI
        for param in &method.params {
            write!(out, "{}", gen_param_to_c(param)).ok();
        }

        let c_params: Vec<String> = method
            .params
            .iter()
            .map(|p| format!("c{}", p.name.to_pascal_case()))
            .collect();

        let type_snake = typ.name.to_snake_case();
        let method_snake = method.name.to_snake_case();
        let c_call = format!(
            "C.{}_{}_{} (unsafe.Pointer({}), {})",
            ffi_prefix,
            type_snake,
            method_snake,
            receiver_name,
            c_params.join(", ")
        );

        if method.error_type.is_some() {
            if matches!(method.return_type, TypeRef::Unit) {
                writeln!(out, "    {}", c_call).ok();
                writeln!(out, "    return lastError()").ok();
            } else {
                writeln!(out, "    ptr := {}", c_call).ok();
                writeln!(out, "    if err := lastError(); err != nil {{").ok();
                // Free the pointer if non-nil even on error, to avoid leaks
                if matches!(
                    method.return_type,
                    TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
                ) {
                    writeln!(out, "        if ptr != nil {{").ok();
                    writeln!(out, "            C.{}_free_string(ptr)", ffi_prefix).ok();
                    writeln!(out, "        }}").ok();
                }
                writeln!(out, "        return nil, err").ok();
                writeln!(out, "    }}").ok();
                // Free the FFI-allocated string after unmarshaling
                if matches!(
                    method.return_type,
                    TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
                ) {
                    writeln!(out, "    defer C.{}_free_string(ptr)", ffi_prefix).ok();
                }
                writeln!(out, "    return unmarshal{}(ptr), nil", type_name(&method.return_type)).ok();
            }
        } else if matches!(method.return_type, TypeRef::Unit) {
            writeln!(out, "    {}", c_call).ok();
        } else {
            writeln!(out, "    ptr := {}", c_call).ok();
            // Add defer free for C string returns
            if matches!(
                method.return_type,
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Bytes
            ) {
                writeln!(out, "    defer C.{}_free_string(ptr)", ffi_prefix).ok();
            }
            writeln!(out, "    return unmarshal{}(ptr)", type_name(&method.return_type)).ok();
        }
    }

    writeln!(out, "}}").ok();
    out
}

/// Generate parameter conversion code from Go to C.
fn gen_param_to_c(param: &alef_core::ir::ParamDef) -> String {
    let mut out = String::with_capacity(512);
    let c_name = format!("c{}", param.name.to_pascal_case());

    match &param.ty {
        TypeRef::String | TypeRef::Char => {
            writeln!(
                out,
                "    {} := C.CString({})\n    defer C.free(unsafe.Pointer({}))",
                c_name, param.name, c_name
            )
            .ok();
        }
        TypeRef::Path => {
            writeln!(
                out,
                "    {} := C.CString({})\n    defer C.free(unsafe.Pointer({}))",
                c_name, param.name, c_name
            )
            .ok();
        }
        TypeRef::Bytes => {
            writeln!(out, "    {} := (*C.uchar)(unsafe.Pointer(&{}[0]))", c_name, param.name).ok();
        }
        TypeRef::Named(_) => {
            writeln!(
                out,
                "    jsonBytes, err := json.Marshal({})\n    if err != nil {{\n        \
                 return fmt.Errorf(\"failed to marshal: %w\", err)\n    \
                 }}\n    {} := C.CString(string(jsonBytes))\n    defer C.free(unsafe.Pointer({}))",
                param.name, c_name, c_name
            )
            .ok();
        }
        TypeRef::Optional(inner) => {
            match inner.as_ref() {
                TypeRef::String | TypeRef::Char | TypeRef::Path => {
                    writeln!(
                        out,
                        "    var {} *C.char\n    if {} != nil {{\n        \
                         {} = C.CString(*{})\n        defer C.free(unsafe.Pointer({}))\n    \
                         }}",
                        c_name, param.name, c_name, param.name, c_name
                    )
                    .ok();
                }
                TypeRef::Named(_) => {
                    writeln!(
                        out,
                        "    var {} *C.char\n    if {} != nil {{\n        \
                         jsonBytes, _ := json.Marshal({})\n        \
                         {} = C.CString(string(jsonBytes))\n        \
                         defer C.free(unsafe.Pointer({}))\n    \
                         }}",
                        c_name, param.name, param.name, c_name, c_name
                    )
                    .ok();
                }
                _ => {
                    // For other optional types, just pass nil or default
                    writeln!(out, "    var {} *C.char", c_name).ok();
                }
            }
        }
        _ => {
            // Primitives and other types pass through directly
        }
    }

    if !out.is_empty() {
        writeln!(out).ok();
    }
    out
}

/// Get a type name suitable for a function suffix (e.g., unmarshalFoo).
fn type_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(n) => n.to_pascal_case(),
        TypeRef::String | TypeRef::Char => "String".to_string(),
        TypeRef::Bytes => "Bytes".to_string(),
        TypeRef::Optional(inner) => type_name(inner),
        TypeRef::Vec(inner) => format!("List{}", type_name(inner)),
        TypeRef::Map(_, v) => format!("Map{}", type_name(v)),
        TypeRef::Json => "JSON".to_string(),
        TypeRef::Path => "Path".to_string(),
        TypeRef::Unit => "Void".to_string(),
        TypeRef::Duration => "U64".to_string(),
        TypeRef::Primitive(p) => match p {
            alef_core::ir::PrimitiveType::Bool => "Bool".to_string(),
            alef_core::ir::PrimitiveType::U8 => "U8".to_string(),
            alef_core::ir::PrimitiveType::U16 => "U16".to_string(),
            alef_core::ir::PrimitiveType::U32 => "U32".to_string(),
            alef_core::ir::PrimitiveType::U64 => "U64".to_string(),
            alef_core::ir::PrimitiveType::I8 => "I8".to_string(),
            alef_core::ir::PrimitiveType::I16 => "I16".to_string(),
            alef_core::ir::PrimitiveType::I32 => "I32".to_string(),
            alef_core::ir::PrimitiveType::I64 => "I64".to_string(),
            alef_core::ir::PrimitiveType::F32 => "F32".to_string(),
            alef_core::ir::PrimitiveType::F64 => "F64".to_string(),
            alef_core::ir::PrimitiveType::Usize => "Usize".to_string(),
            alef_core::ir::PrimitiveType::Isize => "Isize".to_string(),
        },
    }
}

/// Generate functional options pattern for Go config types with defaults.
/// Produces ConfigOption type and WithFieldName constructors.
fn gen_config_options(typ: &TypeDef) -> String {
    let mut out = String::with_capacity(2048);

    // ConfigOption type definition
    writeln!(out, "// {} option function", typ.name).ok();
    writeln!(out, "type {}Option func(*{})", typ.name, typ.name).ok();
    writeln!(out).ok();

    // Generate WithFieldName constructors for each field
    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }

        let field_go_name = to_go_name(&field.name);
        // For the function parameter, always accept the direct type (not wrapped in optional)
        let param_type = go_type(&field.ty);

        writeln!(out, "// With{} sets the {} field.", field_go_name, field.name).ok();
        writeln!(
            out,
            "func With{}(v {}) {}Option {{",
            field_go_name, param_type, typ.name
        )
        .ok();
        writeln!(out, "    return func(c *{}) {{ c.{} = v }}", typ.name, field_go_name).ok();
        writeln!(out, "}}").ok();
        writeln!(out).ok();
    }

    // Generate NewConfig constructor
    writeln!(
        out,
        "// New{} creates a {} with optional parameters.",
        typ.name, typ.name
    )
    .ok();
    writeln!(out, "func New{}(opts ...{}Option) *{} {{", typ.name, typ.name, typ.name).ok();
    writeln!(out, "    c := &{} {{", typ.name).ok();

    // Set default values for fields
    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }

        let field_go_name = to_go_name(&field.name);
        let default_val = if let Some(default) = &field.default {
            default.clone()
        } else {
            // Use type-appropriate zero value
            match &field.ty {
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "\"\"".to_string(),
                TypeRef::Bytes => "[]byte{}".to_string(),
                TypeRef::Primitive(p) => match p {
                    alef_core::ir::PrimitiveType::Bool => "false".to_string(),
                    alef_core::ir::PrimitiveType::F32 | alef_core::ir::PrimitiveType::F64 => "0.0".to_string(),
                    _ => "0".to_string(),
                },
                TypeRef::Vec(_) => "[]".to_string() + &go_type(&field.ty),
                TypeRef::Map(_, _) => "make(".to_string() + &go_type(&field.ty) + ")",
                TypeRef::Optional(_) => "nil".to_string(),
                TypeRef::Named(name) => format!("&{}{{}}", name),
                TypeRef::Unit => "".to_string(),
                TypeRef::Duration => "0".to_string(),
            }
        };
        writeln!(out, "        {}: {},", field_go_name, default_val).ok();
    }

    writeln!(out, "    }}").ok();
    writeln!(out, "    for _, opt := range opts {{").ok();
    writeln!(out, "        opt(c)").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "    return c").ok();
    writeln!(out, "}}").ok();

    out
}
