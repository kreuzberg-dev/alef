use crate::type_map::csharp_type;
use alef_codegen::naming::to_csharp_name;
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, PrimitiveType, TypeDef, TypeRef};
use heck::{ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use std::collections::HashSet;
use std::path::PathBuf;

pub struct CsharpBackend;

impl CsharpBackend {
    // lib_name comes from config.ffi_lib_name()
}

impl Backend for CsharpBackend {
    fn name(&self) -> &str {
        "csharp"
    }

    fn language(&self) -> Language {
        Language::Csharp
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
        let namespace = config.csharp_namespace();
        let prefix = config.ffi_prefix();
        let lib_name = config.ffi_lib_name();

        let output_dir = resolve_output_dir(
            config.output.csharp.as_ref(),
            &config.crate_config.name,
            "packages/csharp/",
        );

        let base_path = PathBuf::from(&output_dir).join(namespace.replace('.', "/"));

        let mut files = Vec::new();

        // 1. Generate NativeMethods.cs
        files.push(GeneratedFile {
            path: base_path.join("NativeMethods.cs"),
            content: strip_trailing_whitespace(&gen_native_methods(api, &namespace, &lib_name, &prefix)),
            generated_header: true,
        });

        // 2. Generate error types from thiserror enums (if any), otherwise generic exception
        if !api.errors.is_empty() {
            for error in &api.errors {
                let error_files = alef_codegen::error_gen::gen_csharp_error_types(error, &namespace);
                for (class_name, content) in error_files {
                    files.push(GeneratedFile {
                        path: base_path.join(format!("{}.cs", class_name)),
                        content: strip_trailing_whitespace(&content),
                        generated_header: false, // already has header
                    });
                }
            }
        }

        // Fallback generic exception class (always generated for GetLastError)
        let exception_class_name = format!("{}Exception", api.crate_name.to_pascal_case());
        if api.errors.is_empty()
            || !api
                .errors
                .iter()
                .any(|e| format!("{}Exception", e.name) == exception_class_name)
        {
            files.push(GeneratedFile {
                path: base_path.join(format!("{}.cs", exception_class_name)),
                content: strip_trailing_whitespace(&gen_exception_class(&namespace, &exception_class_name)),
                generated_header: true,
            });
        }

        // 3. Generate main wrapper class
        let base_class_name = api.crate_name.to_pascal_case();
        let wrapper_class_name = if namespace == base_class_name {
            format!("{}Lib", base_class_name)
        } else {
            base_class_name
        };
        files.push(GeneratedFile {
            path: base_path.join(format!("{}.cs", wrapper_class_name)),
            content: strip_trailing_whitespace(&gen_wrapper_class(
                api,
                &namespace,
                &wrapper_class_name,
                &exception_class_name,
                &prefix,
            )),
            generated_header: true,
        });

        // 4. Generate opaque handle classes
        for typ in &api.types {
            if typ.is_opaque {
                let type_filename = typ.name.to_pascal_case();
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.cs", type_filename)),
                    content: strip_trailing_whitespace(&gen_opaque_handle(typ, &namespace)),
                    generated_header: true,
                });
            }
        }

        // Collect enum names so record generation can distinguish enum fields from class fields.
        let enum_names: HashSet<String> = api.enums.iter().map(|e| e.name.to_pascal_case()).collect();

        // 5. Generate record types (structs)
        for typ in &api.types {
            if !typ.is_opaque {
                // Skip types where all fields are unnamed tuple positions — they have no
                // meaningful properties to expose in C#.
                let has_named_fields = typ.fields.iter().any(|f| !is_tuple_field(f));
                if !typ.fields.is_empty() && !has_named_fields {
                    continue;
                }

                let type_filename = typ.name.to_pascal_case();
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.cs", type_filename)),
                    content: strip_trailing_whitespace(&gen_record_type(typ, &namespace, &enum_names)),
                    generated_header: true,
                });
            }
        }

        // 6. Generate enums
        for enum_def in &api.enums {
            let enum_filename = enum_def.name.to_pascal_case();
            files.push(GeneratedFile {
                path: base_path.join(format!("{}.cs", enum_filename)),
                content: strip_trailing_whitespace(&gen_enum(enum_def, &namespace)),
                generated_header: true,
            });
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Csharp)?;

        Ok(files)
    }

    /// C# wrapper class is already the public API.
    /// The `gen_wrapper_class` (generated in `generate_bindings`) provides high-level public methods
    /// that wrap NativeMethods (P/Invoke), marshal types, and handle errors.
    /// No additional facade is needed.
    fn generate_public_api(&self, _api: &ApiSurface, _config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        // C#'s wrapper class IS the public API — no additional wrapper needed.
        Ok(vec![])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "dotnet",
            crate_suffix: "",
            depends_on_ffi: true,
            post_build: vec![],
        })
    }
}

/// Returns true if a field is a tuple struct positional field (e.g., `_0`, `_1`, `0`, `1`).
fn is_tuple_field(field: &FieldDef) -> bool {
    (field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit()))
        || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
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

// ---------------------------------------------------------------------------
// Helpers: P/Invoke return type mapping
// ---------------------------------------------------------------------------

/// Returns the C# type to use in a `[DllImport]` declaration for the given return type.
///
/// Key differences from the high-level `csharp_type`:
/// - Bool is marshalled as `int` (C FFI convention) — the wrapper compares != 0.
/// - String / Named / Vec / Map / Path / Json / Bytes all come back as `IntPtr`.
/// - Numeric primitives use their natural C# types (`nuint`, `int`, etc.).
fn pinvoke_return_type(ty: &TypeRef) -> &'static str {
    match ty {
        TypeRef::Unit => "void",
        // Bool over FFI is a C int (0/1).
        TypeRef::Primitive(PrimitiveType::Bool) => "int",
        // Numeric primitives — use their real C# types.
        TypeRef::Primitive(PrimitiveType::U8) => "byte",
        TypeRef::Primitive(PrimitiveType::U16) => "ushort",
        TypeRef::Primitive(PrimitiveType::U32) => "uint",
        TypeRef::Primitive(PrimitiveType::U64) => "ulong",
        TypeRef::Primitive(PrimitiveType::I8) => "sbyte",
        TypeRef::Primitive(PrimitiveType::I16) => "short",
        TypeRef::Primitive(PrimitiveType::I32) => "int",
        TypeRef::Primitive(PrimitiveType::I64) => "long",
        TypeRef::Primitive(PrimitiveType::F32) => "float",
        TypeRef::Primitive(PrimitiveType::F64) => "double",
        TypeRef::Primitive(PrimitiveType::Usize) => "ulong",
        TypeRef::Primitive(PrimitiveType::Isize) => "long",
        // Duration as u64
        TypeRef::Duration => "ulong",
        // Everything else is a pointer that needs manual marshalling.
        TypeRef::String
        | TypeRef::Char
        | TypeRef::Bytes
        | TypeRef::Optional(_)
        | TypeRef::Vec(_)
        | TypeRef::Map(_, _)
        | TypeRef::Named(_)
        | TypeRef::Path
        | TypeRef::Json => "IntPtr",
    }
}

/// Does the return type need IntPtr→string marshalling in the wrapper?
fn returns_string(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json)
}

/// Does the return type come back as a C int that should be converted to bool?
fn returns_bool_via_int(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Primitive(PrimitiveType::Bool))
}

/// Does the return type need JSON deserialization from an IntPtr string?
fn returns_json_object(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Vec(_) | TypeRef::Map(_, _) | TypeRef::Named(_) | TypeRef::Bytes | TypeRef::Optional(_)
    )
}

/// Does this return type represent an opaque handle (Named struct type) that needs special marshalling?
///
/// Opaque handles are returned as `IntPtr` from P/Invoke.  The wrapper must call
/// `{prefix}_{type_snake}_to_json(ptr)` to obtain a JSON string, then deserialise it,
/// and finally `{prefix}_{type_snake}_free(ptr)` to release the handle.
///
/// Enum types are excluded — they are treated as JSON-string `IntPtr` returns like Vec/Map.
fn returns_opaque_handle(ty: &TypeRef, enum_names: &HashSet<String>) -> bool {
    match ty {
        TypeRef::Named(name) => !enum_names.contains(name),
        _ => false,
    }
}

/// Returns the C# type to use for a parameter in a `[DllImport]` declaration.
///
/// Managed reference types (Named structs, Vec, Map, Bytes, Optional of Named, etc.)
/// cannot be directly marshalled by P/Invoke.  They must be passed as `IntPtr` (opaque
/// handle or JSON-string pointer).  Primitive types and plain strings use their natural
/// types.
fn pinvoke_param_type(ty: &TypeRef) -> &'static str {
    match ty {
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "string",
        // Managed objects — pass as opaque IntPtr (serialised to handle before call)
        TypeRef::Named(_) | TypeRef::Vec(_) | TypeRef::Map(_, _) | TypeRef::Bytes | TypeRef::Optional(_) => "IntPtr",
        TypeRef::Unit => "void",
        TypeRef::Primitive(PrimitiveType::Bool) => "int",
        TypeRef::Primitive(PrimitiveType::U8) => "byte",
        TypeRef::Primitive(PrimitiveType::U16) => "ushort",
        TypeRef::Primitive(PrimitiveType::U32) => "uint",
        TypeRef::Primitive(PrimitiveType::U64) => "ulong",
        TypeRef::Primitive(PrimitiveType::I8) => "sbyte",
        TypeRef::Primitive(PrimitiveType::I16) => "short",
        TypeRef::Primitive(PrimitiveType::I32) => "int",
        TypeRef::Primitive(PrimitiveType::I64) => "long",
        TypeRef::Primitive(PrimitiveType::F32) => "float",
        TypeRef::Primitive(PrimitiveType::F64) => "double",
        TypeRef::Primitive(PrimitiveType::Usize) => "ulong",
        TypeRef::Primitive(PrimitiveType::Isize) => "long",
        TypeRef::Duration => "ulong",
    }
}

// ---------------------------------------------------------------------------
// Code generation functions
// ---------------------------------------------------------------------------

fn gen_native_methods(api: &ApiSurface, namespace: &str, lib_name: &str, prefix: &str) -> String {
    let mut out = String::from(
        "// This file is auto-generated by alef. DO NOT EDIT.\n\
         using System.Runtime.InteropServices;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    out.push_str("internal static partial class NativeMethods\n{\n");
    out.push_str(&format!("    private const string LibName = \"{}\";\n\n", lib_name));

    // Track emitted C entry-point names to avoid duplicates when the same FFI
    // function appears both as a free function and as a type method.
    let mut emitted: HashSet<String> = HashSet::new();

    // Enum type names — these are NOT opaque handles and must not have from_json / to_json / free
    // helpers emitted for them.
    let enum_names: HashSet<String> = api.enums.iter().map(|e| e.name.clone()).collect();

    // Collect opaque struct type names that appear as parameters or return types so we can
    // emit their from_json / to_json / free P/Invoke helpers.
    // Enum types are excluded.
    let mut opaque_param_types: HashSet<String> = HashSet::new();
    let mut opaque_return_types: HashSet<String> = HashSet::new();

    for func in &api.functions {
        for param in &func.params {
            if let TypeRef::Named(name) = &param.ty {
                if !enum_names.contains(name) {
                    opaque_param_types.insert(name.clone());
                }
            }
        }
        if let TypeRef::Named(name) = &func.return_type {
            if !enum_names.contains(name) {
                opaque_return_types.insert(name.clone());
            }
        }
    }
    for typ in &api.types {
        for method in &typ.methods {
            for param in &method.params {
                if let TypeRef::Named(name) = &param.ty {
                    if !enum_names.contains(name) {
                        opaque_param_types.insert(name.clone());
                    }
                }
            }
            if let TypeRef::Named(name) = &method.return_type {
                if !enum_names.contains(name) {
                    opaque_return_types.insert(name.clone());
                }
            }
        }
    }

    // Emit from_json + free helpers for opaque types used as parameters.
    // E.g. `htm_conversion_options_from_json(const char *json) -> HTMConversionOptions*`
    for type_name in &opaque_param_types {
        let snake = type_name.to_snake_case();
        let from_json_entry = format!("{prefix}_{snake}_from_json");
        let from_json_cs = format!("{}FromJson", type_name.to_pascal_case());
        if emitted.insert(from_json_entry.clone()) {
            out.push_str(&format!(
                "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{from_json_entry}\")]\n"
            ));
            out.push_str(&format!(
                "    internal static extern IntPtr {from_json_cs}([MarshalAs(UnmanagedType.LPStr)] string json);\n\n"
            ));
        }
        let free_entry = format!("{prefix}_{snake}_free");
        let free_cs = format!("{}Free", type_name.to_pascal_case());
        if emitted.insert(free_entry.clone()) {
            out.push_str(&format!(
                "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{free_entry}\")]\n"
            ));
            out.push_str(&format!("    internal static extern void {free_cs}(IntPtr ptr);\n\n"));
        }
    }

    // Emit to_json + free helpers for opaque types returned from functions.
    for type_name in &opaque_return_types {
        let snake = type_name.to_snake_case();
        let to_json_entry = format!("{prefix}_{snake}_to_json");
        let to_json_cs = format!("{}ToJson", type_name.to_pascal_case());
        if emitted.insert(to_json_entry.clone()) {
            out.push_str(&format!(
                "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{to_json_entry}\")]\n"
            ));
            out.push_str(&format!(
                "    internal static extern IntPtr {to_json_cs}(IntPtr ptr);\n\n"
            ));
        }
        let free_entry = format!("{prefix}_{snake}_free");
        let free_cs = format!("{}Free", type_name.to_pascal_case());
        if emitted.insert(free_entry.clone()) {
            out.push_str(&format!(
                "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{free_entry}\")]\n"
            ));
            out.push_str(&format!("    internal static extern void {free_cs}(IntPtr ptr);\n\n"));
        }
    }

    // Generate P/Invoke declarations for functions
    for func in &api.functions {
        let c_func_name = format!("{}_{}", prefix, func.name.to_lowercase());
        if emitted.insert(c_func_name.clone()) {
            out.push_str(&gen_pinvoke_for_func(&c_func_name, func));
        }
    }

    // Generate P/Invoke declarations for type methods
    for typ in &api.types {
        for method in &typ.methods {
            let c_method_name = format!("{}_{}", prefix, method.name.to_lowercase());
            if emitted.insert(c_method_name.clone()) {
                out.push_str(&gen_pinvoke_for_method(&c_method_name, method));
            }
        }
    }

    // Add error handling functions with PascalCase names
    out.push_str(&format!(
        "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{prefix}_last_error_code\")]\n"
    ));
    out.push_str("    internal static extern int LastErrorCode();\n\n");

    out.push_str(&format!(
        "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{prefix}_last_error_context\")]\n"
    ));
    out.push_str("    internal static extern IntPtr LastErrorContext();\n\n");

    out.push_str(&format!(
        "    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{prefix}_free_string\")]\n"
    ));
    out.push_str("    internal static extern void FreeString(IntPtr ptr);\n");

    out.push_str("}\n");

    out
}

fn gen_pinvoke_for_func(c_name: &str, func: &FunctionDef) -> String {
    let cs_name = to_csharp_name(&func.name);
    let mut out =
        format!("    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{c_name}\")]\n");
    out.push_str("    internal static extern ");

    // Return type — use the correct P/Invoke type for each kind.
    out.push_str(pinvoke_return_type(&func.return_type));

    out.push_str(&format!(" {}(", cs_name));

    if func.params.is_empty() {
        out.push_str(");\n\n");
    } else {
        out.push('\n');
        for (i, param) in func.params.iter().enumerate() {
            out.push_str("        ");
            let pinvoke_ty = pinvoke_param_type(&param.ty);
            if pinvoke_ty == "string" {
                out.push_str("[MarshalAs(UnmanagedType.LPStr)] ");
            }
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("{pinvoke_ty} {param_name}"));

            if i < func.params.len() - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("    );\n\n");
    }

    out
}

fn gen_pinvoke_for_method(c_name: &str, method: &MethodDef) -> String {
    let cs_name = to_csharp_name(&method.name);
    let mut out =
        format!("    [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, EntryPoint = \"{c_name}\")]\n");
    out.push_str("    internal static extern ");

    // Return type — use the correct P/Invoke type for each kind.
    out.push_str(pinvoke_return_type(&method.return_type));

    out.push_str(&format!(" {}(", cs_name));

    if method.params.is_empty() {
        out.push_str(");\n\n");
    } else {
        out.push('\n');
        for (i, param) in method.params.iter().enumerate() {
            out.push_str("        ");
            let pinvoke_ty = pinvoke_param_type(&param.ty);
            if pinvoke_ty == "string" {
                out.push_str("[MarshalAs(UnmanagedType.LPStr)] ");
            }
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("{pinvoke_ty} {param_name}"));

            if i < method.params.len() - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("    );\n\n");
    }

    out
}

fn gen_exception_class(namespace: &str, class_name: &str) -> String {
    let mut out = String::from(
        "// This file is auto-generated by alef. DO NOT EDIT.\n\
         using System;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    out.push_str(&format!("public class {} : Exception\n", class_name));
    out.push_str("{\n");
    out.push_str("    public int Code { get; }\n\n");
    out.push_str(&format!(
        "    public {}(int code, string message) : base(message)\n",
        class_name
    ));
    out.push_str("    {\n");
    out.push_str("        Code = code;\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    out
}

fn gen_wrapper_class(
    api: &ApiSurface,
    namespace: &str,
    class_name: &str,
    exception_name: &str,
    prefix: &str,
) -> String {
    let mut out = String::from(
        "// This file is auto-generated by alef. DO NOT EDIT.\n\
         using System;\n\
         using System.Collections.Generic;\n\
         using System.Runtime.InteropServices;\n\
         using System.Text.Json;\n\
         using System.Text.Json.Serialization;\n\
         using System.Threading.Tasks;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    out.push_str(&format!("public static class {}\n", class_name));
    out.push_str("{\n");

    // Enum names: used to distinguish opaque struct handles from enum return types.
    let enum_names: HashSet<String> = api.enums.iter().map(|e| e.name.to_pascal_case()).collect();

    // Generate wrapper methods for functions
    for func in &api.functions {
        out.push_str(&gen_wrapper_function(func, exception_name, prefix, &enum_names));
    }

    // Generate wrapper methods for type methods (prefixed with type name to avoid collisions)
    for typ in &api.types {
        // Skip opaque types (no C# representation for their methods)
        if typ.is_opaque {
            continue;
        }
        for method in &typ.methods {
            // Skip methods that return opaque types not representable in C#
            if let alef_core::ir::TypeRef::Named(ref name) = method.return_type {
                if api.types.iter().any(|t| t.name == *name && t.is_opaque) {
                    continue;
                }
            }
            out.push_str(&gen_wrapper_method(
                method,
                exception_name,
                prefix,
                &typ.name,
                &enum_names,
            ));
        }
    }

    // Add error handling helper
    out.push_str("    private static ");
    out.push_str(&format!("{} GetLastError()\n", exception_name));
    out.push_str("    {\n");
    out.push_str("        var code = NativeMethods.LastErrorCode();\n");
    out.push_str("        var ctxPtr = NativeMethods.LastErrorContext();\n");
    out.push_str("        var message = Marshal.PtrToStringAnsi(ctxPtr) ?? \"Unknown error\";\n");
    out.push_str(&format!("        return new {}(code, message);\n", exception_name));
    out.push_str("    }\n");

    out.push_str("}\n");

    out
}

// ---------------------------------------------------------------------------
// Helpers: Named-param setup/teardown for opaque handle marshalling
// ---------------------------------------------------------------------------

/// For each `Named` parameter, emit code to serialise it to JSON and obtain a native handle.
///
/// ```text
/// var optionsJson = options != null ? JsonSerializer.Serialize(options) : "null";
/// var optionsHandle = NativeMethods.ConversionOptionsFromJson(optionsJson);
/// ```
fn emit_named_param_setup(out: &mut String, params: &[alef_core::ir::ParamDef], indent: &str) {
    for param in params {
        if let TypeRef::Named(type_name) = &param.ty {
            let param_name = param.name.to_lower_camel_case();
            let json_var = format!("{param_name}Json");
            let handle_var = format!("{param_name}Handle");
            let from_json_method = format!("{}FromJson", type_name.to_pascal_case());

            if param.optional {
                out.push_str(&format!(
                    "{indent}var {json_var} = {param_name} != null ? JsonSerializer.Serialize({param_name}) : \"null\";\n"
                ));
            } else {
                out.push_str(&format!(
                    "{indent}var {json_var} = JsonSerializer.Serialize({param_name});\n"
                ));
            }
            out.push_str(&format!(
                "{indent}var {handle_var} = NativeMethods.{from_json_method}({json_var});\n"
            ));
        }
    }
}

/// Returns the argument expression to pass to the native method for a given parameter.
///
/// For `Named` types this is the handle variable (e.g. `optionsHandle`).
/// For everything else it is the parameter name (with `!` for optional).
fn native_call_arg(ty: &TypeRef, param_name: &str, optional: bool) -> String {
    if matches!(ty, TypeRef::Named(_)) {
        format!("{param_name}Handle")
    } else {
        let bang = if optional { "!" } else { "" };
        format!("{param_name}{bang}")
    }
}

/// Emit cleanup code to free native handles allocated for `Named` parameters.
fn emit_named_param_teardown(out: &mut String, params: &[alef_core::ir::ParamDef]) {
    for param in params {
        if let TypeRef::Named(type_name) = &param.ty {
            let param_name = param.name.to_lower_camel_case();
            let handle_var = format!("{param_name}Handle");
            let free_method = format!("{}Free", type_name.to_pascal_case());
            out.push_str(&format!("        NativeMethods.{free_method}({handle_var});\n"));
        }
    }
}

/// Emit cleanup code with configurable indentation (used inside `Task.Run` lambdas).
fn emit_named_param_teardown_indented(out: &mut String, params: &[alef_core::ir::ParamDef], indent: &str) {
    for param in params {
        if let TypeRef::Named(type_name) = &param.ty {
            let param_name = param.name.to_lower_camel_case();
            let handle_var = format!("{param_name}Handle");
            let free_method = format!("{}Free", type_name.to_pascal_case());
            out.push_str(&format!("{indent}NativeMethods.{free_method}({handle_var});\n"));
        }
    }
}

fn gen_wrapper_function(
    func: &FunctionDef,
    _exception_name: &str,
    _prefix: &str,
    enum_names: &HashSet<String>,
) -> String {
    let mut out = String::with_capacity(1024);

    // XML doc comment
    if !func.doc.is_empty() {
        out.push_str("    /// <summary>\n");
        for line in func.doc.lines() {
            out.push_str(&format!("    /// {}\n", line));
        }
        out.push_str("    /// </summary>\n");
        for param in &func.params {
            out.push_str(&format!(
                "    /// <param name=\"{}\">{}</param>\n",
                param.name.to_lower_camel_case(),
                if param.optional { "Optional." } else { "" }
            ));
        }
    }

    out.push_str("    public static ");

    // Return type — use async Task<T> for async methods
    if func.is_async {
        if func.return_type == TypeRef::Unit {
            out.push_str("async Task");
        } else {
            out.push_str(&format!("async Task<{}>", csharp_type(&func.return_type)));
        }
    } else if func.return_type == TypeRef::Unit {
        out.push_str("void");
    } else {
        out.push_str(&csharp_type(&func.return_type));
    }

    out.push_str(&format!(" {}", to_csharp_name(&func.name)));
    out.push('(');

    // Parameters
    for (i, param) in func.params.iter().enumerate() {
        let param_name = param.name.to_lower_camel_case();
        let mapped = csharp_type(&param.ty);
        if param.optional && !mapped.ends_with('?') {
            out.push_str(&format!("{mapped}? {param_name}"));
        } else {
            out.push_str(&format!("{mapped} {param_name}"));
        }

        if i < func.params.len() - 1 {
            out.push_str(", ");
        }
    }

    out.push_str(")\n    {\n");

    // Null checks for required string/object parameters
    for param in &func.params {
        if !param.optional && matches!(param.ty, TypeRef::String | TypeRef::Named(_) | TypeRef::Bytes) {
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("        ArgumentNullException.ThrowIfNull({param_name});\n"));
        }
    }

    // Serialize Named (opaque handle) params to JSON and obtain native handles.
    emit_named_param_setup(&mut out, &func.params, "        ");

    // Method body - delegation to native method with proper marshalling
    let cs_native_name = to_csharp_name(&func.name);

    if func.is_async {
        // Async: wrap in Task.Run for non-blocking execution
        out.push_str("        return await Task.Run(() =>\n        {\n");

        if func.return_type != TypeRef::Unit {
            out.push_str("            var result = ");
        } else {
            out.push_str("            ");
        }

        out.push_str(&format!("NativeMethods.{}(", cs_native_name));

        if func.params.is_empty() {
            out.push_str(");\n");
        } else {
            out.push('\n');
            for (i, param) in func.params.iter().enumerate() {
                let param_name = param.name.to_lower_camel_case();
                let arg = native_call_arg(&param.ty, &param_name, param.optional);
                out.push_str(&format!("                {arg}"));
                if i < func.params.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("            );\n");
        }

        emit_return_marshalling_indented(&mut out, &func.return_type, "            ", enum_names);
        emit_named_param_teardown_indented(&mut out, &func.params, "            ");
        emit_return_statement_indented(&mut out, &func.return_type, "            ");
        out.push_str("        });\n");
    } else {
        if func.return_type != TypeRef::Unit {
            out.push_str("        var result = ");
        } else {
            out.push_str("        ");
        }

        out.push_str(&format!("NativeMethods.{}(", cs_native_name));

        if func.params.is_empty() {
            out.push_str(");\n");
        } else {
            out.push('\n');
            for (i, param) in func.params.iter().enumerate() {
                let param_name = param.name.to_lower_camel_case();
                let arg = native_call_arg(&param.ty, &param_name, param.optional);
                out.push_str(&format!("            {arg}"));
                if i < func.params.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("        );\n");
        }

        emit_return_marshalling(&mut out, &func.return_type, enum_names);
        emit_named_param_teardown(&mut out, &func.params);
        emit_return_statement(&mut out, &func.return_type);
    }

    out.push_str("    }\n\n");

    out
}

fn gen_wrapper_method(
    method: &MethodDef,
    _exception_name: &str,
    _prefix: &str,
    type_name: &str,
    enum_names: &HashSet<String>,
) -> String {
    let mut out = String::with_capacity(1024);

    // XML doc comment
    if !method.doc.is_empty() {
        out.push_str("    /// <summary>\n");
        for line in method.doc.lines() {
            out.push_str(&format!("    /// {}\n", line));
        }
        out.push_str("    /// </summary>\n");
        for param in &method.params {
            out.push_str(&format!(
                "    /// <param name=\"{}\">{}</param>\n",
                param.name.to_lower_camel_case(),
                if param.optional { "Optional." } else { "" }
            ));
        }
    }

    // The wrapper class is always `static class`, so all methods must be static.
    out.push_str("    public static ");

    // Return type — use async Task<T> for async methods
    if method.is_async {
        if method.return_type == TypeRef::Unit {
            out.push_str("async Task");
        } else {
            out.push_str(&format!("async Task<{}>", csharp_type(&method.return_type)));
        }
    } else if method.return_type == TypeRef::Unit {
        out.push_str("void");
    } else {
        out.push_str(&csharp_type(&method.return_type));
    }

    // Prefix method name with type name to avoid collisions (e.g., MetadataConfigDefault)
    let method_cs_name = format!("{}{}", type_name, to_csharp_name(&method.name));
    out.push_str(&format!(" {method_cs_name}"));
    out.push('(');

    // Parameters
    for (i, param) in method.params.iter().enumerate() {
        let param_name = param.name.to_lower_camel_case();
        let mapped = csharp_type(&param.ty);
        if param.optional && !mapped.ends_with('?') {
            out.push_str(&format!("{mapped}? {param_name}"));
        } else {
            out.push_str(&format!("{mapped} {param_name}"));
        }

        if i < method.params.len() - 1 {
            out.push_str(", ");
        }
    }

    out.push_str(")\n    {\n");

    // Null checks for required string/object parameters
    for param in &method.params {
        if !param.optional && matches!(param.ty, TypeRef::String | TypeRef::Named(_) | TypeRef::Bytes) {
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("        ArgumentNullException.ThrowIfNull({param_name});\n"));
        }
    }

    // Serialize Named (opaque handle) params to JSON and obtain native handles.
    emit_named_param_setup(&mut out, &method.params, "        ");

    // Method body - delegation to native method with proper marshalling
    let cs_native_name = to_csharp_name(&method.name);

    if method.is_async {
        // Async: wrap in Task.Run for non-blocking execution
        out.push_str("        return await Task.Run(() =>\n        {\n");

        if method.return_type != TypeRef::Unit {
            out.push_str("            var result = ");
        } else {
            out.push_str("            ");
        }

        out.push_str(&format!("NativeMethods.{}(", cs_native_name));

        if method.params.is_empty() {
            out.push_str(");\n");
        } else {
            out.push('\n');
            for (i, param) in method.params.iter().enumerate() {
                let param_name = param.name.to_lower_camel_case();
                let arg = native_call_arg(&param.ty, &param_name, param.optional);
                out.push_str(&format!("                {arg}"));
                if i < method.params.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("            );\n");
        }

        emit_return_marshalling_indented(&mut out, &method.return_type, "            ", enum_names);
        emit_named_param_teardown_indented(&mut out, &method.params, "            ");
        emit_return_statement_indented(&mut out, &method.return_type, "            ");
        out.push_str("        });\n");
    } else {
        if method.return_type != TypeRef::Unit {
            out.push_str("        var result = ");
        } else {
            out.push_str("        ");
        }

        out.push_str(&format!("NativeMethods.{}(", cs_native_name));

        if method.params.is_empty() {
            out.push_str(");\n");
        } else {
            out.push('\n');
            for (i, param) in method.params.iter().enumerate() {
                let param_name = param.name.to_lower_camel_case();
                let arg = native_call_arg(&param.ty, &param_name, param.optional);
                out.push_str(&format!("            {arg}"));
                if i < method.params.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("        );\n");
        }

        emit_return_marshalling(&mut out, &method.return_type, enum_names);
        emit_named_param_teardown(&mut out, &method.params);
        emit_return_statement(&mut out, &method.return_type);
    }

    out.push_str("    }\n\n");

    out
}

/// Emit the return-value marshalling code shared by both function and method wrappers.
///
/// This function emits the code to convert the raw P/Invoke `result` into the managed return
/// type and store it in a local variable `returnValue`.  It intentionally does **not** emit
/// the `return` statement so that callers can interpose cleanup (param handle teardown) between
/// the value computation and the return.
///
/// `enum_names`: the set of C# type names that are enums (not opaque handles).
///
/// Callers must invoke `emit_return_statement` after their cleanup to complete the method body.
fn emit_return_marshalling(out: &mut String, return_type: &TypeRef, enum_names: &HashSet<String>) {
    if *return_type == TypeRef::Unit {
        // void — nothing to return
        return;
    }

    if returns_string(return_type) {
        // IntPtr → string, then free the native buffer.
        out.push_str("        var returnValue = Marshal.PtrToStringUTF8(result) ?? string.Empty;\n");
        out.push_str("        NativeMethods.FreeString(result);\n");
    } else if returns_bool_via_int(return_type) {
        // C int → bool
        out.push_str("        var returnValue = result != 0;\n");
    } else if returns_opaque_handle(return_type, enum_names) {
        // result is an opaque C handle (e.g. HTMConversionResult*).
        // Call to_json on the handle to get a JSON string, deserialise, then free both.
        let cs_ty = csharp_type(return_type);
        let type_name = match return_type {
            TypeRef::Named(n) => n.to_pascal_case(),
            _ => unreachable!(),
        };
        let to_json_method = format!("{}ToJson", type_name);
        let free_method = format!("{}Free", type_name);
        out.push_str(&format!(
            "        var jsonPtr = NativeMethods.{to_json_method}(result);\n"
        ));
        out.push_str("        var json = Marshal.PtrToStringUTF8(jsonPtr);\n");
        out.push_str("        NativeMethods.FreeString(jsonPtr);\n");
        out.push_str(&format!("        NativeMethods.{free_method}(result);\n"));
        out.push_str(&format!(
            "        var returnValue = JsonSerializer.Deserialize<{}>(json ?? \"null\")!;\n",
            cs_ty
        ));
    } else if returns_json_object(return_type) {
        // IntPtr → JSON string → deserialized object, then free the native buffer.
        let cs_ty = csharp_type(return_type);
        out.push_str("        var json = Marshal.PtrToStringUTF8(result);\n");
        out.push_str("        NativeMethods.FreeString(result);\n");
        out.push_str(&format!(
            "        var returnValue = JsonSerializer.Deserialize<{}>(json ?? \"null\")!;\n",
            cs_ty
        ));
    } else {
        // Numeric primitives — direct return.
        out.push_str("        var returnValue = result;\n");
    }
}

/// Emit the final `return returnValue;` statement after cleanup.
fn emit_return_statement(out: &mut String, return_type: &TypeRef) {
    if *return_type != TypeRef::Unit {
        out.push_str("        return returnValue;\n");
    }
}

/// Emit the return-value marshalling code with configurable indentation.
///
/// Like `emit_return_marshalling` this stores the value in `returnValue` without emitting
/// the final `return` statement.  Callers must call `emit_return_statement_indented` after.
fn emit_return_marshalling_indented(
    out: &mut String,
    return_type: &TypeRef,
    indent: &str,
    enum_names: &HashSet<String>,
) {
    if *return_type == TypeRef::Unit {
        return;
    }

    if returns_string(return_type) {
        out.push_str(&format!(
            "{indent}var returnValue = Marshal.PtrToStringUTF8(result) ?? string.Empty;\n"
        ));
        out.push_str(&format!("{indent}NativeMethods.FreeString(result);\n"));
    } else if returns_bool_via_int(return_type) {
        out.push_str(&format!("{indent}var returnValue = result != 0;\n"));
    } else if returns_opaque_handle(return_type, enum_names) {
        let cs_ty = csharp_type(return_type);
        let type_name = match return_type {
            TypeRef::Named(n) => n.to_pascal_case(),
            _ => unreachable!(),
        };
        let to_json_method = format!("{}ToJson", type_name);
        let free_method = format!("{}Free", type_name);
        out.push_str(&format!(
            "{indent}var jsonPtr = NativeMethods.{to_json_method}(result);\n"
        ));
        out.push_str(&format!("{indent}var json = Marshal.PtrToStringUTF8(jsonPtr);\n"));
        out.push_str(&format!("{indent}NativeMethods.FreeString(jsonPtr);\n"));
        out.push_str(&format!("{indent}NativeMethods.{free_method}(result);\n"));
        out.push_str(&format!(
            "{indent}var returnValue = JsonSerializer.Deserialize<{}>(json ?? \"null\")!;\n",
            cs_ty
        ));
    } else if returns_json_object(return_type) {
        let cs_ty = csharp_type(return_type);
        out.push_str(&format!("{indent}var json = Marshal.PtrToStringUTF8(result);\n"));
        out.push_str(&format!("{indent}NativeMethods.FreeString(result);\n"));
        out.push_str(&format!(
            "{indent}var returnValue = JsonSerializer.Deserialize<{}>(json ?? \"null\")!;\n",
            cs_ty
        ));
    } else {
        out.push_str(&format!("{indent}var returnValue = result;\n"));
    }
}

/// Emit the final `return returnValue;` with configurable indentation.
fn emit_return_statement_indented(out: &mut String, return_type: &TypeRef, indent: &str) {
    if *return_type != TypeRef::Unit {
        out.push_str(&format!("{indent}return returnValue;\n"));
    }
}

fn gen_opaque_handle(typ: &TypeDef, namespace: &str) -> String {
    let mut out = String::from(
        "// This file is auto-generated by alef. DO NOT EDIT.\n\
         using System;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    // Generate doc comment if available
    if !typ.doc.is_empty() {
        out.push_str("/// <summary>\n");
        for line in typ.doc.lines() {
            out.push_str(&format!("/// {}\n", line));
        }
        out.push_str("/// </summary>\n");
    }

    let class_name = typ.name.to_pascal_case();
    out.push_str(&format!("public sealed class {} : IDisposable\n", class_name));
    out.push_str("{\n");
    out.push_str("    internal IntPtr Handle { get; }\n\n");
    out.push_str(&format!("    internal {}(IntPtr handle)\n", class_name));
    out.push_str("    {\n");
    out.push_str("        Handle = handle;\n");
    out.push_str("    }\n\n");
    out.push_str("    public void Dispose()\n");
    out.push_str("    {\n");
    out.push_str("        // Native free will be called by the runtime\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    out
}

fn gen_record_type(typ: &TypeDef, namespace: &str, enum_names: &HashSet<String>) -> String {
    let mut out = String::from(
        "// This file is auto-generated by alef. DO NOT EDIT.\n\
         using System;\n\
         using System.Collections.Generic;\n\
         using System.Text.Json.Serialization;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    // Generate doc comment if available
    if !typ.doc.is_empty() {
        out.push_str("/// <summary>\n");
        for line in typ.doc.lines() {
            out.push_str(&format!("/// {}\n", line));
        }
        out.push_str("/// </summary>\n");
    }

    out.push_str(&format!("public sealed class {}\n", typ.name.to_pascal_case()));
    out.push_str("{\n");

    for field in &typ.fields {
        // Skip unnamed tuple struct fields (e.g., _0, _1, 0, 1, etc.)
        if is_tuple_field(field) {
            continue;
        }

        // Doc comment for field
        if !field.doc.is_empty() {
            out.push_str("    /// <summary>\n");
            for line in field.doc.lines() {
                out.push_str(&format!("    /// {}\n", line));
            }
            out.push_str("    /// </summary>\n");
        }

        // [JsonPropertyName("camelCaseName")]
        let json_name = field.name.to_lower_camel_case();
        out.push_str(&format!("    [JsonPropertyName(\"{}\")]\n", json_name));

        let cs_name = to_csharp_name(&field.name);

        if field.optional {
            // Optional fields: nullable type, no `required`, default = null
            let mapped = csharp_type(&field.ty);
            let field_type = if mapped.ends_with('?') {
                mapped.to_string()
            } else {
                format!("{mapped}?")
            };
            out.push_str(&format!("    public {} {} {{ get; set; }}", field_type, cs_name));
            out.push_str(" = null;\n");
        } else if typ.has_default || field.default.is_some() {
            // Field with an explicit default value or part of a type with defaults
            let field_type = csharp_type(&field.ty).to_string();
            out.push_str(&format!("    public {} {} {{ get; set; }}", field_type, cs_name));
            if let Some(default) = &field.default {
                out.push_str(&format!(" = {};\n", default));
            } else {
                // Use type-appropriate zero value
                let default_val = match &field.ty {
                    TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "\"\"".to_string(),
                    TypeRef::Bytes => "Array.Empty<byte>()".to_string(),
                    TypeRef::Primitive(p) => match p {
                        PrimitiveType::Bool => "false".to_string(),
                        PrimitiveType::F32 | PrimitiveType::F64 => "0.0".to_string(),
                        _ => "0".to_string(),
                    },
                    TypeRef::Vec(_) => "[]".to_string(),
                    TypeRef::Map(k, v) => format!("new Dictionary<{}, {}>()", csharp_type(k), csharp_type(v)),
                    TypeRef::Duration => "0".to_string(),
                    TypeRef::Named(name) => {
                        let pascal = name.to_pascal_case();
                        if enum_names.contains(&pascal) {
                            // Enums are value types in C# — use `default` (not `null`)
                            "default".to_string()
                        } else {
                            // Reference types — use `default!` to satisfy non-nullable analysis
                            "default!".to_string()
                        }
                    }
                    _ => "default!".to_string(),
                };
                out.push_str(&format!(" = {};\n", default_val));
            }
        } else {
            // Required field: no default, not optional
            let field_type = csharp_type(&field.ty).to_string();
            out.push_str(&format!(
                "    public required {} {} {{ get; set; }}\n",
                field_type, cs_name
            ));
        }

        out.push('\n');
    }

    out.push_str("}\n");

    out
}

fn gen_enum(enum_def: &EnumDef, namespace: &str) -> String {
    let mut out = String::from("// This file is auto-generated by alef. DO NOT EDIT.\n\n");

    out.push_str(&format!("namespace {};\n\n", namespace));

    // Generate doc comment if available
    if !enum_def.doc.is_empty() {
        out.push_str("/// <summary>\n");
        for line in enum_def.doc.lines() {
            out.push_str(&format!("/// {}\n", line));
        }
        out.push_str("/// </summary>\n");
    }

    out.push_str(&format!("public enum {}\n", enum_def.name.to_pascal_case()));
    out.push_str("{\n");

    // Enum variants
    for variant in &enum_def.variants {
        if !variant.doc.is_empty() {
            out.push_str("    /// <summary>\n");
            for line in variant.doc.lines() {
                out.push_str(&format!("    /// {}\n", line));
            }
            out.push_str("    /// </summary>\n");
        }

        out.push_str(&format!("    {},\n", variant.name.to_pascal_case()));
    }

    out.push_str("}\n");

    out
}
