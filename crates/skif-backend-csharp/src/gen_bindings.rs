use crate::type_map::csharp_type;
use heck::{ToLowerCamelCase, ToPascalCase};
use skif_codegen::naming::to_csharp_name;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, PrimitiveType, TypeDef, TypeRef};
use std::collections::HashSet;
use std::path::PathBuf;

pub struct CsharpBackend;

impl CsharpBackend {
    /// Get the library name for P/Invoke.
    fn lib_name(prefix: &str) -> String {
        // Use just the prefix if it looks like a lib name, otherwise append _ffi
        if prefix.ends_with("_ffi") || prefix == "ffi" {
            prefix.to_string()
        } else {
            format!("{}_ffi", prefix)
        }
    }
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

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let namespace = config.csharp_namespace();
        let prefix = config.ffi_prefix();
        let lib_name = Self::lib_name(&prefix);

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
            content: gen_native_methods(api, &namespace, &lib_name, &prefix),
            generated_header: true,
        });

        // 2. Generate exception class
        let exception_class_name = format!("{}Exception", api.crate_name.to_pascal_case());
        files.push(GeneratedFile {
            path: base_path.join(format!("{}.cs", exception_class_name)),
            content: gen_exception_class(&namespace, &exception_class_name),
            generated_header: true,
        });

        // 3. Generate main wrapper class
        let base_class_name = api.crate_name.to_pascal_case();
        let wrapper_class_name = if namespace == base_class_name {
            format!("{}Lib", base_class_name)
        } else {
            base_class_name
        };
        files.push(GeneratedFile {
            path: base_path.join(format!("{}.cs", wrapper_class_name)),
            content: gen_wrapper_class(api, &namespace, &wrapper_class_name, &exception_class_name, &prefix),
            generated_header: true,
        });

        // 4. Generate record types (structs)
        for typ in &api.types {
            if !typ.is_opaque {
                let type_filename = typ.name.to_pascal_case();
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.cs", type_filename)),
                    content: gen_record_type(typ, &namespace),
                    generated_header: true,
                });
            }
        }

        // 5. Generate enums
        for enum_def in &api.enums {
            let enum_filename = enum_def.name.to_pascal_case();
            files.push(GeneratedFile {
                path: base_path.join(format!("{}.cs", enum_filename)),
                content: gen_enum(enum_def, &namespace),
                generated_header: true,
            });
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Csharp)?;

        Ok(files)
    }
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
        TypeRef::Primitive(PrimitiveType::Usize) => "nuint",
        TypeRef::Primitive(PrimitiveType::Isize) => "nint",
        // Everything else is a pointer that needs manual marshalling.
        TypeRef::String
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
    matches!(ty, TypeRef::String | TypeRef::Path | TypeRef::Json)
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

// ---------------------------------------------------------------------------
// Code generation functions
// ---------------------------------------------------------------------------

fn gen_native_methods(api: &ApiSurface, namespace: &str, lib_name: &str, prefix: &str) -> String {
    let mut out = String::from(
        "// This file is auto-generated by skif. DO NOT EDIT.\n\
         using System.Runtime.InteropServices;\n\n",
    );

    out.push_str(&format!("namespace {};\n\n", namespace));

    out.push_str("internal static partial class NativeMethods\n{\n");
    out.push_str(&format!("    private const string LibName = \"{}\";\n\n", lib_name));

    // Track emitted C entry-point names to avoid duplicates when the same FFI
    // function appears both as a free function and as a type method.
    let mut emitted: HashSet<String> = HashSet::new();

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
            if param.ty == TypeRef::String {
                out.push_str("[MarshalAs(UnmanagedType.LPStr)] ");
            }
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("{} {}", csharp_type(&param.ty), param_name));

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
            if param.ty == TypeRef::String {
                out.push_str("[MarshalAs(UnmanagedType.LPStr)] ");
            }
            let param_name = param.name.to_lower_camel_case();
            out.push_str(&format!("{} {}", csharp_type(&param.ty), param_name));

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
        "// This file is auto-generated by skif. DO NOT EDIT.\n\
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
        "// This file is auto-generated by skif. DO NOT EDIT.\n\
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

    // Generate wrapper methods for functions
    for func in &api.functions {
        out.push_str(&gen_wrapper_function(func, exception_name, prefix));
    }

    // Generate wrapper methods for type methods
    for typ in &api.types {
        if !typ.is_opaque {
            for method in &typ.methods {
                out.push_str(&gen_wrapper_method(method, exception_name, prefix));
            }
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

fn gen_wrapper_function(func: &FunctionDef, _exception_name: &str, _prefix: &str) -> String {
    let mut out = String::with_capacity(1024);

    out.push_str("    public static ");

    // Return type
    if func.return_type == TypeRef::Unit {
        out.push_str("void");
    } else {
        out.push_str(&csharp_type(&func.return_type));
    }

    out.push_str(&format!(" {}", to_csharp_name(&func.name)));
    out.push('(');

    // Parameters
    for (i, param) in func.params.iter().enumerate() {
        let param_name = param.name.to_lower_camel_case();
        if param.optional {
            out.push_str(&format!("{}? {}", csharp_type(&param.ty), param_name));
        } else {
            out.push_str(&format!("{} {}", csharp_type(&param.ty), param_name));
        }

        if i < func.params.len() - 1 {
            out.push_str(", ");
        }
    }

    out.push_str(")\n    {\n");

    // Method body - delegation to native method with proper marshalling
    let cs_native_name = to_csharp_name(&func.name);

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
            out.push_str(&format!("            {}", param_name));
            if i < func.params.len() - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("        );\n");
    }

    emit_return_marshalling(&mut out, &func.return_type);

    out.push_str("    }\n\n");

    out
}

fn gen_wrapper_method(method: &MethodDef, _exception_name: &str, _prefix: &str) -> String {
    let mut out = String::with_capacity(1024);

    // The wrapper class is always `static class`, so all methods must be static.
    out.push_str("    public static ");

    // Return type
    if method.return_type == TypeRef::Unit {
        out.push_str("void");
    } else {
        out.push_str(&csharp_type(&method.return_type));
    }

    out.push_str(&format!(" {}", to_csharp_name(&method.name)));
    out.push('(');

    // Parameters
    for (i, param) in method.params.iter().enumerate() {
        let param_name = param.name.to_lower_camel_case();
        if param.optional {
            out.push_str(&format!("{}? {}", csharp_type(&param.ty), param_name));
        } else {
            out.push_str(&format!("{} {}", csharp_type(&param.ty), param_name));
        }

        if i < method.params.len() - 1 {
            out.push_str(", ");
        }
    }

    out.push_str(")\n    {\n");

    // Method body - delegation to native method with proper marshalling
    let cs_native_name = to_csharp_name(&method.name);

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
            out.push_str(&format!("            {}", param_name));
            if i < method.params.len() - 1 {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("        );\n");
    }

    emit_return_marshalling(&mut out, &method.return_type);

    out.push_str("    }\n\n");

    out
}

/// Emit the return-value marshalling code shared by both function and method wrappers.
///
/// `result` is the local variable holding the native call's return value.
fn emit_return_marshalling(out: &mut String, return_type: &TypeRef) {
    if *return_type == TypeRef::Unit {
        // void — nothing to return
        return;
    }

    if returns_string(return_type) {
        // IntPtr → string, then free the native buffer.
        out.push_str("        var str = Marshal.PtrToStringUTF8(result);\n");
        out.push_str("        NativeMethods.FreeString(result);\n");
        out.push_str("        return str ?? string.Empty;\n");
    } else if returns_bool_via_int(return_type) {
        // C int → bool
        out.push_str("        return result != 0;\n");
    } else if returns_json_object(return_type) {
        // IntPtr → JSON string → deserialized object, then free the native buffer.
        let cs_ty = csharp_type(return_type);
        out.push_str("        var json = Marshal.PtrToStringUTF8(result);\n");
        out.push_str("        NativeMethods.FreeString(result);\n");
        out.push_str(&format!(
            "        return JsonSerializer.Deserialize<{}>(json ?? \"null\")!;\n",
            cs_ty
        ));
    } else {
        // Numeric primitives — direct return.
        out.push_str("        return result;\n");
    }
}

fn gen_record_type(typ: &TypeDef, namespace: &str) -> String {
    let mut out = String::from(
        "// This file is auto-generated by skif. DO NOT EDIT.\n\
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
            let field_type = format!("{}?", csharp_type(&field.ty));
            out.push_str(&format!("    public {} {} {{ get; set; }}", field_type, cs_name));
            out.push_str(" = null;\n");
        } else if let Some(default) = &field.default {
            // Field with an explicit default value
            let field_type = csharp_type(&field.ty).to_string();
            out.push_str(&format!("    public {} {} {{ get; set; }}", field_type, cs_name));
            out.push_str(&format!(" = {};\n", default));
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
    let mut out = String::from("// This file is auto-generated by skif. DO NOT EDIT.\n\n");

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
