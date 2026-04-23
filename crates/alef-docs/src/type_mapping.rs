use crate::naming::type_name;
use alef_core::config::Language;
use alef_core::ir::{PrimitiveType, TypeRef};

pub fn doc_type(ty: &TypeRef, lang: Language, ffi_prefix: &str) -> String {
    match ty {
        TypeRef::String | TypeRef::Char => match lang {
            Language::Python => "str".to_string(),
            Language::Node | Language::Wasm => "string".to_string(),
            Language::Go => "string".to_string(),
            Language::Java => "String".to_string(),
            Language::Csharp => "string".to_string(),
            Language::Ruby => "String".to_string(),
            Language::Php => "string".to_string(),
            Language::Elixir => "String.t()".to_string(),
            Language::R => "character".to_string(),
            Language::Rust => "String".to_string(),
            Language::Ffi => "const char*".to_string(),
        },
        TypeRef::Bytes => match lang {
            Language::Python => "bytes".to_string(),
            Language::Node | Language::Wasm => "Buffer".to_string(),
            Language::Go => "[]byte".to_string(),
            Language::Java => "byte[]".to_string(),
            Language::Csharp => "byte[]".to_string(),
            Language::Ruby => "String".to_string(),
            Language::Php => "string".to_string(),
            Language::Elixir => "binary()".to_string(),
            Language::R => "raw".to_string(),
            Language::Rust => "Vec<u8>".to_string(),
            Language::Ffi => "const uint8_t*".to_string(),
        },
        TypeRef::Primitive(p) => doc_primitive(p, lang),
        TypeRef::Optional(inner) => {
            let inner_ty = doc_type(inner, lang, ffi_prefix);
            match lang {
                Language::Python => format!("{inner_ty} | None"),
                Language::Node | Language::Wasm => format!("{inner_ty} | null"),
                Language::Go => format!("*{inner_ty}"),
                Language::Java => {
                    let boxed = java_boxed_type(inner);
                    format!("Optional<{boxed}>")
                }
                Language::Csharp => format!("{inner_ty}?"),
                Language::Ruby => format!("{inner_ty}?"),
                Language::Php => format!("?{inner_ty}"),
                Language::Elixir => format!("{inner_ty} | nil"),
                Language::R => format!("{inner_ty} or NULL"),
                Language::Rust => format!("Option<{inner_ty}>"),
                Language::Ffi => format!("{inner_ty}*"),
            }
        }
        TypeRef::Vec(inner) => {
            match lang {
                Language::Java => {
                    // Java generics can't use primitives — box them
                    let inner_ty = java_boxed_type(inner);
                    format!("List<{inner_ty}>")
                }
                Language::Csharp => {
                    let inner_ty = doc_type(inner, lang, ffi_prefix);
                    format!("List<{inner_ty}>")
                }
                _ => {
                    let inner_ty = doc_type(inner, lang, ffi_prefix);
                    match lang {
                        Language::Python => format!("list[{inner_ty}]"),
                        Language::Node | Language::Wasm => format!("Array<{inner_ty}>"),
                        Language::Go => format!("[]{inner_ty}"),
                        Language::Ruby => format!("Array<{inner_ty}>"),
                        Language::Php => format!("array<{inner_ty}>"),
                        Language::Elixir => format!("list({inner_ty})"),
                        Language::R => "list".to_string(),
                        Language::Rust => format!("Vec<{inner_ty}>"),
                        Language::Ffi => format!("{inner_ty}*"),
                        Language::Java | Language::Csharp => unreachable!(),
                    }
                }
            }
        }
        TypeRef::Map(k, v) => {
            if lang == Language::Java {
                // Java generics require boxed types
                let kty = java_boxed_type(k);
                let vty = java_boxed_type(v);
                return format!("Map<{kty}, {vty}>");
            }
            let kty = doc_type(k, lang, ffi_prefix);
            let vty = doc_type(v, lang, ffi_prefix);
            match lang {
                Language::Python => format!("dict[{kty}, {vty}]"),
                Language::Node | Language::Wasm => format!("Record<{kty}, {vty}>"),
                Language::Go => format!("map[{kty}]{vty}"),
                Language::Java => format!("Map<{kty}, {vty}>"),
                Language::Csharp => format!("Dictionary<{kty}, {vty}>"),
                Language::Ruby => format!("Hash{{{kty}=>{vty}}}"),
                Language::Php => format!("array<{kty}, {vty}>"),
                Language::Elixir => "map()".to_string(),
                Language::R => "list".to_string(),
                Language::Rust => format!("HashMap<{kty}, {vty}>"),
                Language::Ffi => "void*".to_string(),
            }
        }
        TypeRef::Named(name) if name.starts_with('(') && name.ends_with(')') => {
            // Tuple type encoded as Named("(A, B)") — render idiomatically per language
            let inner = &name[1..name.len() - 1];
            let rendered: Vec<String> = inner
                .split(',')
                .map(|part| {
                    let trimmed = part.trim();
                    match trimmed {
                        "usize" | "u64" | "u32" | "u16" | "u8" | "i64" | "i32" | "i16" | "i8" | "isize" => match lang {
                            Language::Python => "int".to_string(),
                            Language::Node | Language::Wasm => "number".to_string(),
                            Language::Go => "int".to_string(),
                            Language::Java => "long".to_string(),
                            Language::Csharp => "long".to_string(),
                            Language::Ruby => "Integer".to_string(),
                            Language::Php => "int".to_string(),
                            Language::Elixir => "integer()".to_string(),
                            Language::R => "integer".to_string(),
                            Language::Rust => trimmed.to_string(),
                            Language::Ffi => "uint64_t".to_string(),
                        },
                        s @ ("str" | "&str" | "String" | "&'static str" | "&'staticstr") => match lang {
                            Language::Python => "str".to_string(),
                            Language::Node | Language::Wasm => "string".to_string(),
                            Language::Go => "string".to_string(),
                            Language::Java => "String".to_string(),
                            Language::Csharp => "string".to_string(),
                            Language::Ruby => "String".to_string(),
                            Language::Php => "string".to_string(),
                            Language::Elixir => "String.t()".to_string(),
                            Language::R => "character".to_string(),
                            Language::Rust => s.to_string(),
                            Language::Ffi => "const char*".to_string(),
                        },
                        // Slice of strings — &[&str], &'static [&'static str], Vec<String>, etc.
                        // Also covers compacted IR forms like &'static[&'staticstr]
                        s if s.contains("[&")
                            || s.contains("[String")
                            || s.contains("Vec<&")
                            || s.contains("Vec<String")
                            || s.contains("staticstr") =>
                        {
                            match lang {
                                Language::Python => "list[str]".to_string(),
                                Language::Node | Language::Wasm => "string[]".to_string(),
                                Language::Go => "[]string".to_string(),
                                Language::Java => "List<String>".to_string(),
                                Language::Csharp => "List<string>".to_string(),
                                Language::Ruby => "Array<String>".to_string(),
                                Language::Php => "array<string>".to_string(),
                                Language::Elixir => "list(String.t())".to_string(),
                                Language::R => "list".to_string(),
                                Language::Rust => s.to_string(),
                                Language::Ffi => "const char**".to_string(),
                            }
                        }
                        other => {
                            // For Rust, preserve the raw type token rather than
                            // PascalCasing it — Rust type names are already correct.
                            if lang == Language::Rust {
                                other.to_string()
                            } else {
                                type_name(other, lang, ffi_prefix)
                            }
                        }
                    }
                })
                .collect();
            match lang {
                Language::Python => format!("tuple[{}]", rendered.join(", ")),
                Language::Node | Language::Wasm => format!("[{}]", rendered.join(", ")),
                Language::Go => format!("({})", rendered.join(", ")),
                Language::Java => format!("Tuple<{}>", rendered.join(", ")),
                Language::Csharp => format!("({})", rendered.join(", ")),
                Language::Ruby => format!("[{}]", rendered.join(", ")),
                Language::Php => format!("array{{{}}}", rendered.join(", ")),
                Language::Elixir => format!("{{{}}}", rendered.join(", ")),
                Language::R => "list".to_string(),
                Language::Rust => format!("({})", rendered.join(", ")),
                Language::Ffi => "void*".to_string(),
            }
        }
        TypeRef::Named(name) => type_name(name, lang, ffi_prefix),
        TypeRef::Path => match lang {
            Language::Python => "str".to_string(),
            Language::Node | Language::Wasm => "string".to_string(),
            Language::Go => "string".to_string(),
            Language::Java => "String".to_string(),
            Language::Csharp => "string".to_string(),
            Language::Ruby => "String".to_string(),
            Language::Php => "string".to_string(),
            Language::Elixir => "String.t()".to_string(),
            Language::R => "character".to_string(),
            Language::Rust => "PathBuf".to_string(),
            Language::Ffi => "const char*".to_string(),
        },
        TypeRef::Unit => match lang {
            Language::Python => "None".to_string(),
            Language::Node | Language::Wasm => "void".to_string(),
            Language::Go => "".to_string(),
            Language::Java => "void".to_string(),
            Language::Csharp => "void".to_string(),
            Language::Ruby => "nil".to_string(),
            Language::Php => "void".to_string(),
            Language::Elixir => ":ok".to_string(),
            Language::R => "NULL".to_string(),
            Language::Rust => "()".to_string(),
            Language::Ffi => "void".to_string(),
        },
        TypeRef::Json => match lang {
            Language::Python => "dict[str, Any]".to_string(),
            Language::Node | Language::Wasm => "unknown".to_string(),
            Language::Go => "interface{}".to_string(),
            Language::Java => "Object".to_string(),
            Language::Csharp => "object".to_string(),
            Language::Ruby => "Object".to_string(),
            Language::Php => "mixed".to_string(),
            Language::Elixir => "term()".to_string(),
            Language::R => "list".to_string(),
            Language::Rust => "serde_json::Value".to_string(),
            Language::Ffi => "void*".to_string(),
        },
        TypeRef::Duration => match lang {
            Language::Python => "float".to_string(),
            Language::Node | Language::Wasm => "number".to_string(),
            Language::Go => "time.Duration".to_string(),
            Language::Java => "Duration".to_string(),
            Language::Csharp => "TimeSpan".to_string(),
            Language::Ruby => "Float".to_string(),
            Language::Php => "float".to_string(),
            Language::Elixir => "integer()".to_string(),
            Language::R => "numeric".to_string(),
            Language::Rust => "std::time::Duration".to_string(),
            Language::Ffi => "uint64_t".to_string(),
        },
    }
}

pub fn doc_primitive(p: &PrimitiveType, lang: Language) -> String {
    match lang {
        Language::Python => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "float".to_string(),
            _ => "int".to_string(),
        },
        Language::Node | Language::Wasm => match p {
            PrimitiveType::Bool => "boolean".to_string(),
            _ => "number".to_string(),
        },
        Language::Go => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "uint8".to_string(),
            PrimitiveType::U16 => "uint16".to_string(),
            PrimitiveType::U32 => "uint32".to_string(),
            PrimitiveType::U64 => "uint64".to_string(),
            PrimitiveType::I8 => "int8".to_string(),
            PrimitiveType::I16 => "int16".to_string(),
            PrimitiveType::I32 => "int32".to_string(),
            PrimitiveType::I64 => "int64".to_string(),
            PrimitiveType::F32 => "float32".to_string(),
            PrimitiveType::F64 => "float64".to_string(),
            PrimitiveType::Usize | PrimitiveType::Isize => "int".to_string(),
        },
        Language::Java => match p {
            PrimitiveType::Bool => "boolean".to_string(),
            PrimitiveType::U8 | PrimitiveType::I8 => "byte".to_string(),
            PrimitiveType::U16 | PrimitiveType::I16 => "short".to_string(),
            PrimitiveType::U32 | PrimitiveType::I32 => "int".to_string(),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => "long".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
        },
        Language::Csharp => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "byte".to_string(),
            PrimitiveType::U16 => "ushort".to_string(),
            PrimitiveType::U32 => "uint".to_string(),
            PrimitiveType::U64 => "ulong".to_string(),
            PrimitiveType::I8 => "sbyte".to_string(),
            PrimitiveType::I16 => "short".to_string(),
            PrimitiveType::I32 => "int".to_string(),
            PrimitiveType::I64 => "long".to_string(),
            PrimitiveType::Usize => "nuint".to_string(),
            PrimitiveType::Isize => "nint".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
        },
        Language::Ruby => match p {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "Float".to_string(),
            _ => "Integer".to_string(),
        },
        Language::Php => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "float".to_string(),
            _ => "int".to_string(),
        },
        Language::Elixir => match p {
            PrimitiveType::Bool => "boolean()".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "float()".to_string(),
            _ => "integer()".to_string(),
        },
        Language::R => match p {
            PrimitiveType::Bool => "logical".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "numeric".to_string(),
            _ => "integer".to_string(),
        },
        Language::Ffi => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "uint8_t".to_string(),
            PrimitiveType::U16 => "uint16_t".to_string(),
            PrimitiveType::U32 => "uint32_t".to_string(),
            PrimitiveType::U64 => "uint64_t".to_string(),
            PrimitiveType::I8 => "int8_t".to_string(),
            PrimitiveType::I16 => "int16_t".to_string(),
            PrimitiveType::I32 => "int32_t".to_string(),
            PrimitiveType::I64 => "int64_t".to_string(),
            PrimitiveType::Usize => "uintptr_t".to_string(),
            PrimitiveType::Isize => "intptr_t".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
        },
        Language::Rust => match p {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "u8".to_string(),
            PrimitiveType::U16 => "u16".to_string(),
            PrimitiveType::U32 => "u32".to_string(),
            PrimitiveType::U64 => "u64".to_string(),
            PrimitiveType::I8 => "i8".to_string(),
            PrimitiveType::I16 => "i16".to_string(),
            PrimitiveType::I32 => "i32".to_string(),
            PrimitiveType::I64 => "i64".to_string(),
            PrimitiveType::Usize => "usize".to_string(),
            PrimitiveType::Isize => "isize".to_string(),
            PrimitiveType::F32 => "f32".to_string(),
            PrimitiveType::F64 => "f64".to_string(),
        },
    }
}

/// Return the boxed (object) type for Java generics.
///
/// Java generics cannot use primitive types (`int`, `long`, etc.); they require
/// the corresponding wrapper classes (`Integer`, `Long`, etc.).
pub fn java_boxed_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(p) => match p {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::U8 | PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U16 | PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U32 | PrimitiveType::I32 => "Integer".to_string(),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => "Long".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        },
        // Non-primitive types are already reference types in Java
        _ => doc_type(ty, Language::Java, ""),
    }
}
