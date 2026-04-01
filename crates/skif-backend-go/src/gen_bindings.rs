use crate::type_map::{go_optional_type, go_type};
use heck::{ToPascalCase, ToSnakeCase};
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

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

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let module_path = config.go_module();
        let pkg_name = Self::package_name(&module_path);
        let ffi_prefix = config.ffi_prefix();

        let output_dir = resolve_output_dir(config.output.go.as_ref(), &config.crate_config.name, "packages/go/");

        let content = gen_go_file(api, &ffi_prefix, &pkg_name);

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("binding.go"),
            content,
            generated_header: true,
        }])
    }
}

/// Generate the complete Go binding file wrapping the C FFI layer.
fn gen_go_file(api: &ApiSurface, ffi_prefix: &str, pkg_name: &str) -> String {
    let mut out = String::new();

    // Package header and imports
    writeln!(out, "package {}\n", pkg_name).unwrap();
    writeln!(
        out,
        "/*\n#cgo LDFLAGS: -l{}_ffi\n#include \"{}.h\"\nimport \"C\"\n*/",
        ffi_prefix, ffi_prefix
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "import (\n    \"encoding/json\"\n    \"fmt\"\n    \"unsafe\"\n)\n").unwrap();

    // Error helper functions
    writeln!(out, "{}\n", gen_last_error_helper(ffi_prefix)).unwrap();

    // Generate enum types and constants
    for enum_def in &api.enums {
        writeln!(out, "{}\n", gen_enum_type(enum_def)).unwrap();
    }

    // Generate struct types
    for typ in &api.types {
        writeln!(out, "{}\n", gen_struct_type(typ)).unwrap();
    }

    // Generate free function wrappers
    for func in &api.functions {
        writeln!(out, "{}\n", gen_function_wrapper(func, ffi_prefix)).unwrap();
    }

    // Generate struct methods
    for typ in &api.types {
        for method in &typ.methods {
            writeln!(out, "{}\n", gen_method_wrapper(typ, method, ffi_prefix)).unwrap();
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
         return fmt.Errorf(\"[%d] %s\", code, message)\n\
         }}",
        ffi_prefix, ffi_prefix
    )
}

/// Generate a Go enum type definition with constants.
fn gen_enum_type(enum_def: &EnumDef) -> String {
    let mut out = String::new();

    if !enum_def.doc.is_empty() {
        writeln!(out, "// {} represents the {} enumeration.", enum_def.name, enum_def.doc).unwrap();
    } else {
        writeln!(out, "// {} is an enumeration type.", enum_def.name).unwrap();
    }
    writeln!(out, "type {} string", enum_def.name).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "const (").unwrap();

    for variant in &enum_def.variants {
        let const_name = format!("{}{}", enum_def.name, variant.name.to_pascal_case());
        let variant_snake = variant.name.to_snake_case();
        if !variant.doc.is_empty() {
            writeln!(out, "    // {} {}", const_name, variant.doc).unwrap();
        }
        writeln!(out, "    {} {} = \"{}\"", const_name, enum_def.name, variant_snake).unwrap();
    }

    writeln!(out, ")").unwrap();
    out
}

/// Generate a Go struct type definition with json tags for marshaling.
fn gen_struct_type(typ: &TypeDef) -> String {
    let mut out = String::new();

    if !typ.doc.is_empty() {
        writeln!(out, "// {} {}", typ.name, typ.doc).unwrap();
    } else {
        writeln!(out, "// {} is a type.", typ.name).unwrap();
    }
    writeln!(out, "type {} struct {{", typ.name).unwrap();

    for field in &typ.fields {
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
            writeln!(out, "    // {} {}", field.name.to_pascal_case(), field.doc).unwrap();
        }
        writeln!(out, "    {} {} `{}`", field.name.to_pascal_case(), field_type, json_tag).unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

/// Generate a wrapper function for a free function.
fn gen_function_wrapper(func: &FunctionDef, ffi_prefix: &str) -> String {
    let mut out = String::new();

    if !func.doc.is_empty() {
        writeln!(out, "// {} {}", func.name, func.doc).unwrap();
    } else {
        writeln!(out, "// {} calls the FFI function.", func.name).unwrap();
    }

    let return_type = if func.error_type.is_some() {
        if matches!(func.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("*{}, error", go_type(&func.return_type))
        }
    } else if matches!(func.return_type, TypeRef::Unit) {
        "".to_string()
    } else {
        format!("*{}", go_type(&func.return_type))
    };

    let func_snake = func.name.to_snake_case();
    let ffi_name = format!("C.{}_{}", ffi_prefix, func_snake);

    write!(out, "func {}(", func.name).unwrap();

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
    write!(out, "{}", params.join(", ")).unwrap();

    if return_type.is_empty() {
        writeln!(out, ") {{").unwrap();
    } else {
        writeln!(out, ") {} {{", return_type).unwrap();
    }

    // Convert parameters
    for param in &func.params {
        write!(out, "{}", gen_param_to_c(param)).unwrap();
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
            writeln!(out, "    {}", c_call).unwrap();
            writeln!(out, "    return lastError()").unwrap();
        } else {
            writeln!(out, "    ptr := {}", c_call).unwrap();
            writeln!(out, "    if err := lastError(); err != nil {{").unwrap();
            writeln!(out, "        return nil, err").unwrap();
            writeln!(out, "    }}").unwrap();
            writeln!(out, "    return unmarshal{}(ptr), nil", type_name(&func.return_type)).unwrap();
        }
    } else if matches!(func.return_type, TypeRef::Unit) {
        writeln!(out, "    {}", c_call).unwrap();
    } else {
        writeln!(out, "    ptr := {}", c_call).unwrap();
        writeln!(out, "    return unmarshal{}(ptr)", type_name(&func.return_type)).unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

/// Generate a wrapper method for a struct method.
fn gen_method_wrapper(typ: &TypeDef, method: &MethodDef, ffi_prefix: &str) -> String {
    let mut out = String::new();

    if !method.doc.is_empty() {
        writeln!(out, "// {} {}", method.name, method.doc).unwrap();
    } else {
        writeln!(out, "// {} is a method.", method.name).unwrap();
    }

    let return_type = if method.error_type.is_some() {
        if matches!(method.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("*{}, error", go_type(&method.return_type))
        }
    } else if matches!(method.return_type, TypeRef::Unit) {
        "".to_string()
    } else {
        format!("*{}", go_type(&method.return_type))
    };

    let receiver_name = "r";
    let receiver_type = &typ.name;

    // Determine receiver (pointer)
    write!(out, "func ({} *{}) {}(", receiver_name, receiver_type, method.name).unwrap();

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
    write!(out, "{}", params.join(", ")).unwrap();

    if return_type.is_empty() {
        writeln!(out, ") {{").unwrap();
    } else {
        writeln!(out, ") {} {{", return_type).unwrap();
    }

    if method.is_async {
        // Generate async version with channels
        let result_type = if matches!(method.return_type, TypeRef::Unit) {
            "error".to_string()
        } else {
            format!("*{}", go_type(&method.return_type))
        };

        writeln!(out, "    resultCh := make(chan {}, 1)", result_type).unwrap();
        writeln!(out, "    errCh := make(chan error, 1)").unwrap();
        writeln!(out, "    go func() {{").unwrap();

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
            .unwrap();
            writeln!(out, "        if err != nil {{").unwrap();
            writeln!(out, "            errCh <- err").unwrap();
            writeln!(out, "        }} else {{").unwrap();
            writeln!(out, "            errCh <- nil").unwrap();
            writeln!(out, "        }}").unwrap();
        } else {
            writeln!(
                out,
                "        result, err := {}.{}Sync({})",
                receiver_name, method.name, call_args
            )
            .unwrap();
            writeln!(out, "        if err != nil {{").unwrap();
            writeln!(out, "            errCh <- err").unwrap();
            writeln!(out, "        }} else {{").unwrap();
            writeln!(out, "            resultCh <- result").unwrap();
            writeln!(out, "        }}").unwrap();
        }

        writeln!(out, "    }}()").unwrap();
        writeln!(out, "    return resultCh, errCh").unwrap();
    } else {
        // Synchronous method - just convert params and call FFI
        for param in &method.params {
            write!(out, "{}", gen_param_to_c(param)).unwrap();
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
                writeln!(out, "    {}", c_call).unwrap();
                writeln!(out, "    return lastError()").unwrap();
            } else {
                writeln!(out, "    ptr := {}", c_call).unwrap();
                writeln!(out, "    if err := lastError(); err != nil {{").unwrap();
                writeln!(out, "        return nil, err").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(out, "    return unmarshal{}(ptr), nil", type_name(&method.return_type)).unwrap();
            }
        } else if matches!(method.return_type, TypeRef::Unit) {
            writeln!(out, "    {}", c_call).unwrap();
        } else {
            writeln!(out, "    ptr := {}", c_call).unwrap();
            writeln!(out, "    return unmarshal{}(ptr)", type_name(&method.return_type)).unwrap();
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

/// Generate parameter conversion code from Go to C.
fn gen_param_to_c(param: &skif_core::ir::ParamDef) -> String {
    let mut out = String::new();
    let c_name = format!("c{}", param.name.to_pascal_case());

    match &param.ty {
        TypeRef::String => {
            writeln!(
                out,
                "    {} := C.CString({})\n    defer C.free(unsafe.Pointer({}))",
                c_name, param.name, c_name
            )
            .unwrap();
        }
        TypeRef::Bytes => {
            writeln!(out, "    {} := (*C.uchar)(unsafe.Pointer(&{}[0]))", c_name, param.name).unwrap();
        }
        TypeRef::Named(_) => {
            writeln!(
                out,
                "    jsonBytes, err := json.Marshal({})\n    if err != nil {{\n        \
                 return fmt.Errorf(\"failed to marshal: %w\", err)\n    \
                 }}\n    {} := C.CString(string(jsonBytes))\n    defer C.free(unsafe.Pointer({}))",
                param.name, c_name, c_name
            )
            .unwrap();
        }
        TypeRef::Optional(inner) => {
            match inner.as_ref() {
                TypeRef::String => {
                    writeln!(
                        out,
                        "    var {} *C.char\n    if {} != nil {{\n        \
                         {} = C.CString(*{})\n        defer C.free(unsafe.Pointer({}))\n    \
                         }}",
                        c_name, param.name, c_name, param.name, c_name
                    )
                    .unwrap();
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
                    .unwrap();
                }
                _ => {
                    // For other optional types, just pass nil or default
                    writeln!(out, "    var {} *C.char", c_name).unwrap();
                }
            }
        }
        _ => {
            // Primitives and other types pass through directly
        }
    }

    if !out.is_empty() {
        writeln!(out).unwrap();
    }
    out
}

/// Get a type name suitable for a function suffix (e.g., unmarshalFoo).
fn type_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(n) => n.to_pascal_case(),
        TypeRef::String => "String".to_string(),
        TypeRef::Bytes => "Bytes".to_string(),
        TypeRef::Optional(inner) => type_name(inner),
        TypeRef::Vec(inner) => format!("List{}", type_name(inner)),
        TypeRef::Map(_, v) => format!("Map{}", type_name(v)),
        TypeRef::Json => "JSON".to_string(),
        TypeRef::Path => "Path".to_string(),
        TypeRef::Unit => "Void".to_string(),
        TypeRef::Primitive(p) => match p {
            skif_core::ir::PrimitiveType::Bool => "Bool".to_string(),
            skif_core::ir::PrimitiveType::U8 => "U8".to_string(),
            skif_core::ir::PrimitiveType::U16 => "U16".to_string(),
            skif_core::ir::PrimitiveType::U32 => "U32".to_string(),
            skif_core::ir::PrimitiveType::U64 => "U64".to_string(),
            skif_core::ir::PrimitiveType::I8 => "I8".to_string(),
            skif_core::ir::PrimitiveType::I16 => "I16".to_string(),
            skif_core::ir::PrimitiveType::I32 => "I32".to_string(),
            skif_core::ir::PrimitiveType::I64 => "I64".to_string(),
            skif_core::ir::PrimitiveType::F32 => "F32".to_string(),
            skif_core::ir::PrimitiveType::F64 => "F64".to_string(),
            skif_core::ir::PrimitiveType::Usize => "Usize".to_string(),
            skif_core::ir::PrimitiveType::Isize => "Isize".to_string(),
        },
    }
}
