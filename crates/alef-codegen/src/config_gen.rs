use alef_core::ir::{DefaultValue, FieldDef, PrimitiveType, TypeDef, TypeRef};
use heck::{ToPascalCase, ToShoutySnakeCase};

/// Returns true if a field is a tuple struct positional field (e.g., `_0`, `_1`, `0`, `1`).
/// These fields have no meaningful name and must be skipped in languages requiring named fields.
fn is_tuple_field(field: &FieldDef) -> bool {
    (field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit()))
        || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
}

/// Generate a PyO3 `#[new]` constructor with kwargs for a type with `has_default`.
/// All fields become keyword args with their defaults in `#[pyo3(signature = (...))]`.
pub fn gen_pyo3_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let mut lines = Vec::new();
    lines.push("#[new]".to_string());

    // Build the signature line with defaults
    let mut sig_parts = Vec::new();
    for field in &typ.fields {
        let default_str = default_value_for_field(field, "python");
        sig_parts.push(format!("{}={}", field.name, default_str));
    }
    let signature = format!("#[pyo3(signature = ({}))]", sig_parts.join(", "));
    lines.push(signature);

    // Function signature
    lines.push("fn new(".to_string());
    for (i, field) in typ.fields.iter().enumerate() {
        let type_str = type_mapper(&field.ty);
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        lines.push(format!("    {}: {}{}", field.name, type_str, comma));
    }
    lines.push(") -> Self {".to_string());

    // Body
    lines.push("    Self {".to_string());
    for field in &typ.fields {
        lines.push(format!("        {},", field.name));
    }
    lines.push("    }".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Generate NAPI constructor that applies defaults for missing optional fields.
pub fn gen_napi_defaults_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let mut lines = Vec::new();
    lines.push("pub fn new(mut env: napi::Env, obj: napi::Object) -> napi::Result<Self> {".to_string());

    // Field assignments with defaults
    for field in &typ.fields {
        let type_str = type_mapper(&field.ty);
        let default_str = default_value_for_field(field, "rust");
        lines.push(format!(
            "    let {}: {} = obj.get(\"{}\").unwrap_or({})?;",
            field.name, type_str, field.name, default_str
        ));
    }

    lines.push("    Ok(Self {".to_string());
    for field in &typ.fields {
        lines.push(format!("        {},", field.name));
    }
    lines.push("    })".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Generate Go functional options pattern for a type with `has_default`.
/// Returns: type definition + Option type + WithField functions + NewConfig constructor
pub fn gen_go_functional_options(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let mut lines = Vec::new();

    // Type definition
    lines.push(format!("// {} is a configuration type.", typ.name));
    lines.push(format!("type {} struct {{", typ.name));
    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }
        let go_type = type_mapper(&field.ty);
        lines.push(format!("    {} {}", field.name.to_pascal_case(), go_type));
    }
    lines.push("}".to_string());
    lines.push("".to_string());

    // Option function type
    lines.push(format!(
        "// {}Option is a functional option for {}.",
        typ.name, typ.name
    ));
    lines.push(format!("type {}Option func(*{})", typ.name, typ.name));
    lines.push("".to_string());

    // WithField functions
    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }
        let option_name = format!("With{}", field.name.to_pascal_case());
        let go_type = type_mapper(&field.ty);
        lines.push(format!("// {} sets the {}.", option_name, field.name));
        lines.push(format!("func {}(val {}) {}Option {{", option_name, go_type, typ.name));
        lines.push(format!("    return func(c *{}) {{", typ.name));
        lines.push(format!("        c.{} = val", field.name.to_pascal_case()));
        lines.push("    }".to_string());
        lines.push("}".to_string());
        lines.push("".to_string());
    }

    // New constructor
    lines.push(format!(
        "// New{} creates a new {} with default values and applies options.",
        typ.name, typ.name
    ));
    lines.push(format!(
        "func New{}(opts ...{}Option) *{} {{",
        typ.name, typ.name, typ.name
    ));
    lines.push(format!("    c := &{} {{", typ.name));
    for field in &typ.fields {
        if is_tuple_field(field) {
            continue;
        }
        let default_str = default_value_for_field(field, "go");
        lines.push(format!("        {}: {},", field.name.to_pascal_case(), default_str));
    }
    lines.push("    }".to_string());
    lines.push("    for _, opt := range opts {".to_string());
    lines.push("        opt(c)".to_string());
    lines.push("    }".to_string());
    lines.push("    return c".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Generate Java builder pattern for a type with `has_default`.
/// Returns: Builder inner class with withField methods + build() method
pub fn gen_java_builder(typ: &TypeDef, package: &str, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "// DO NOT EDIT - auto-generated by alef\npackage {};\n",
        package
    ));
    lines.push("/// Builder for creating instances of {} with sensible defaults".to_string());
    lines.push(format!("public class {}Builder {{", typ.name));

    // Fields
    for field in &typ.fields {
        let java_type = type_mapper(&field.ty);
        lines.push(format!("    private {} {};", java_type, field.name.to_lowercase()));
    }
    lines.push("".to_string());

    // Constructor
    lines.push(format!("    public {}Builder() {{", typ.name));
    for field in &typ.fields {
        let default_str = default_value_for_field(field, "java");
        lines.push(format!("        this.{} = {};", field.name.to_lowercase(), default_str));
    }
    lines.push("    }".to_string());
    lines.push("".to_string());

    // withField methods
    for field in &typ.fields {
        let java_type = type_mapper(&field.ty);
        let method_name = format!("with{}", field.name.to_pascal_case());
        lines.push(format!(
            "    public {}Builder {}({} value) {{",
            typ.name, method_name, java_type
        ));
        lines.push(format!("        this.{} = value;", field.name.to_lowercase()));
        lines.push("        return this;".to_string());
        lines.push("    }".to_string());
        lines.push("".to_string());
    }

    // build() method
    lines.push(format!("    public {} build() {{", typ.name));
    lines.push(format!("        return new {}(", typ.name));
    for (i, field) in typ.fields.iter().enumerate() {
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        lines.push(format!("            this.{}{}", field.name.to_lowercase(), comma));
    }
    lines.push("        );".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Generate C# record with init properties for a type with `has_default`.
pub fn gen_csharp_record(typ: &TypeDef, namespace: &str, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let mut lines = Vec::new();

    lines.push("// This file is auto-generated by alef. DO NOT EDIT.".to_string());
    lines.push("using System;".to_string());
    lines.push("".to_string());
    lines.push(format!("namespace {};\n", namespace));

    lines.push(format!("/// Configuration record: {}", typ.name));
    lines.push(format!("public record {} {{", typ.name));

    for field in &typ.fields {
        // Skip tuple struct internals (e.g., _0, _1, etc.)
        if field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit())
            || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
        {
            continue;
        }

        let cs_type = type_mapper(&field.ty);
        let default_str = default_value_for_field(field, "csharp");
        lines.push(format!(
            "    public {} {} {{ get; init; }} = {};",
            cs_type,
            field.name.to_pascal_case(),
            default_str
        ));
    }

    lines.push("}".to_string());

    lines.join("\n")
}

/// Get a language-appropriate default value string for a field.
/// Uses `typed_default` if available, falls back to `default` string, or type-based zero value.
pub fn default_value_for_field(field: &FieldDef, language: &str) -> String {
    // First try typed_default if it exists
    if let Some(typed_default) = &field.typed_default {
        return match typed_default {
            DefaultValue::BoolLiteral(b) => match language {
                "python" => {
                    if *b {
                        "True".to_string()
                    } else {
                        "False".to_string()
                    }
                }
                "ruby" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                "go" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                "java" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                "csharp" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                "php" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                "r" => {
                    if *b {
                        "TRUE".to_string()
                    } else {
                        "FALSE".to_string()
                    }
                }
                "rust" => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                _ => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
            },
            DefaultValue::StringLiteral(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            DefaultValue::IntLiteral(n) => n.to_string(),
            DefaultValue::FloatLiteral(f) => {
                let s = f.to_string();
                if !s.contains('.') { format!("{}.0", s) } else { s }
            }
            DefaultValue::EnumVariant(v) => match language {
                "python" => format!("{}.{}", field.ty.type_name(), v.to_shouty_snake_case()),
                "ruby" => format!("{}::{}", field.ty.type_name(), v.to_pascal_case()),
                "go" => format!("{}{}()", field.ty.type_name(), v.to_pascal_case()),
                "java" => format!("{}.{}", field.ty.type_name(), v.to_shouty_snake_case()),
                "csharp" => format!("{}.{}", field.ty.type_name(), v.to_pascal_case()),
                "php" => format!("{}::{}", field.ty.type_name(), v.to_pascal_case()),
                "r" => format!("{}${}", field.ty.type_name(), v.to_pascal_case()),
                "rust" => format!("{}::{}", field.ty.type_name(), v.to_pascal_case()),
                _ => v.clone(),
            },
            DefaultValue::Empty => {
                // Empty means "type's default" — check field type to pick the right zero value
                match &field.ty {
                    TypeRef::Vec(_) => match language {
                        "python" | "ruby" | "csharp" => "[]".to_string(),
                        "go" => "nil".to_string(),
                        "java" => "List.of()".to_string(),
                        "php" => "[]".to_string(),
                        "r" => "c()".to_string(),
                        "rust" => "vec![]".to_string(),
                        _ => "null".to_string(),
                    },
                    TypeRef::Map(_, _) => match language {
                        "python" => "{}".to_string(),
                        "go" => "nil".to_string(),
                        "java" => "Map.of()".to_string(),
                        "rust" => "Default::default()".to_string(),
                        _ => "null".to_string(),
                    },
                    TypeRef::Primitive(p) => match p {
                        PrimitiveType::Bool => match language {
                            "python" => "False".to_string(),
                            "ruby" => "false".to_string(),
                            _ => "false".to_string(),
                        },
                        PrimitiveType::F32 | PrimitiveType::F64 => "0.0".to_string(),
                        _ => "0".to_string(),
                    },
                    TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => match language {
                        "rust" => "String::new()".to_string(),
                        _ => "\"\"".to_string(),
                    },
                    _ => match language {
                        "python" => "None".to_string(),
                        "ruby" => "nil".to_string(),
                        "go" => "nil".to_string(),
                        "rust" => "Default::default()".to_string(),
                        _ => "null".to_string(),
                    },
                }
            }
            DefaultValue::None => match language {
                "python" => "None".to_string(),
                "ruby" => "nil".to_string(),
                "go" => "nil".to_string(),
                "java" => "null".to_string(),
                "csharp" => "null".to_string(),
                "php" => "null".to_string(),
                "r" => "NULL".to_string(),
                "rust" => "None".to_string(),
                _ => "null".to_string(),
            },
        };
    }

    // Fall back to string default if it exists
    if let Some(default_str) = &field.default {
        return default_str.clone();
    }

    // Final fallback: type-based zero value
    match &field.ty {
        TypeRef::Primitive(p) => match p {
            alef_core::ir::PrimitiveType::Bool => match language {
                "python" => "False".to_string(),
                "ruby" => "false".to_string(),
                "csharp" => "false".to_string(),
                "java" => "false".to_string(),
                "php" => "false".to_string(),
                "r" => "FALSE".to_string(),
                _ => "false".to_string(),
            },
            alef_core::ir::PrimitiveType::U8
            | alef_core::ir::PrimitiveType::U16
            | alef_core::ir::PrimitiveType::U32
            | alef_core::ir::PrimitiveType::U64
            | alef_core::ir::PrimitiveType::I8
            | alef_core::ir::PrimitiveType::I16
            | alef_core::ir::PrimitiveType::I32
            | alef_core::ir::PrimitiveType::I64
            | alef_core::ir::PrimitiveType::Usize
            | alef_core::ir::PrimitiveType::Isize => "0".to_string(),
            alef_core::ir::PrimitiveType::F32 | alef_core::ir::PrimitiveType::F64 => "0.0".to_string(),
        },
        TypeRef::String | TypeRef::Char => match language {
            "python" => "\"\"".to_string(),
            "ruby" => "\"\"".to_string(),
            "go" => "\"\"".to_string(),
            "java" => "\"\"".to_string(),
            "csharp" => "\"\"".to_string(),
            "php" => "\"\"".to_string(),
            "r" => "\"\"".to_string(),
            "rust" => "String::new()".to_string(),
            _ => "\"\"".to_string(),
        },
        TypeRef::Bytes => match language {
            "python" => "b\"\"".to_string(),
            "ruby" => "\"\"".to_string(),
            "go" => "[]byte{}".to_string(),
            "java" => "new byte[]{}".to_string(),
            "csharp" => "new byte[]{}".to_string(),
            "php" => "\"\"".to_string(),
            "r" => "raw()".to_string(),
            "rust" => "vec![]".to_string(),
            _ => "[]".to_string(),
        },
        TypeRef::Optional(_) => match language {
            "python" => "None".to_string(),
            "ruby" => "nil".to_string(),
            "go" => "nil".to_string(),
            "java" => "null".to_string(),
            "csharp" => "null".to_string(),
            "php" => "null".to_string(),
            "r" => "NULL".to_string(),
            "rust" => "None".to_string(),
            _ => "null".to_string(),
        },
        TypeRef::Vec(_) => match language {
            "python" => "[]".to_string(),
            "ruby" => "[]".to_string(),
            "go" => "[]interface{}{}".to_string(),
            "java" => "new java.util.ArrayList<>()".to_string(),
            "csharp" => "[]".to_string(),
            "php" => "[]".to_string(),
            "r" => "c()".to_string(),
            "rust" => "vec![]".to_string(),
            _ => "[]".to_string(),
        },
        TypeRef::Map(_, _) => match language {
            "python" => "{}".to_string(),
            "ruby" => "{}".to_string(),
            "go" => "make(map[string]interface{})".to_string(),
            "java" => "new java.util.HashMap<>()".to_string(),
            "csharp" => "new Dictionary<string, object>()".to_string(),
            "php" => "[]".to_string(),
            "r" => "list()".to_string(),
            "rust" => "std::collections::HashMap::new()".to_string(),
            _ => "{}".to_string(),
        },
        TypeRef::Json => match language {
            "python" => "{}".to_string(),
            "ruby" => "{}".to_string(),
            "go" => "make(map[string]interface{})".to_string(),
            "java" => "new com.fasterxml.jackson.databind.JsonNode()".to_string(),
            "csharp" => "JObject.Parse(\"{}\")".to_string(),
            "php" => "[]".to_string(),
            "r" => "list()".to_string(),
            "rust" => "serde_json::json!({})".to_string(),
            _ => "{}".to_string(),
        },
        _ => match language {
            "python" => "None".to_string(),
            "ruby" => "nil".to_string(),
            "go" => "nil".to_string(),
            "java" => "null".to_string(),
            "csharp" => "null".to_string(),
            "php" => "null".to_string(),
            "r" => "NULL".to_string(),
            "rust" => "Default::default()".to_string(),
            _ => "null".to_string(),
        },
    }
}

// Helper trait extension for TypeRef to get type name
trait TypeRefExt {
    fn type_name(&self) -> String;
}

impl TypeRefExt for TypeRef {
    fn type_name(&self) -> String {
        match self {
            TypeRef::Named(n) => n.clone(),
            TypeRef::Primitive(p) => format!("{:?}", p),
            TypeRef::String | TypeRef::Char => "String".to_string(),
            TypeRef::Bytes => "Bytes".to_string(),
            TypeRef::Optional(inner) => format!("Option<{}>", inner.type_name()),
            TypeRef::Vec(inner) => format!("Vec<{}>", inner.type_name()),
            TypeRef::Map(k, v) => format!("Map<{}, {}>", k.type_name(), v.type_name()),
            TypeRef::Path => "Path".to_string(),
            TypeRef::Unit => "()".to_string(),
            TypeRef::Json => "Json".to_string(),
            TypeRef::Duration => "Duration".to_string(),
        }
    }
}

/// Generate a Magnus (Ruby) kwargs constructor for a type with `has_default`.
/// Generates a `new` method that accepts keyword arguments with defaults.
pub fn gen_magnus_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);

    // Start with fn new(
    writeln!(out, "fn new(").ok();

    // Add all fields as keyword parameters with defaults
    for (i, field) in typ.fields.iter().enumerate() {
        let field_type = type_mapper(&field.ty);
        let default_str = default_value_for_field(field, "ruby");
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        writeln!(out, "    {}: {} = {}{}", field.name, field_type, default_str, comma).ok();
    }

    writeln!(out, ") -> Self {{").ok();
    writeln!(out, "    Self {{").ok();

    // Field assignments
    for field in &typ.fields {
        writeln!(out, "        {},", field.name).ok();
    }

    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

/// Generate a PHP kwargs constructor for a type with `has_default`.
/// All fields become `Option<T>` parameters so PHP users can omit any field.
/// Assignments wrap non-Optional fields in `Some()` and apply defaults.
pub fn gen_php_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);

    writeln!(out, "pub fn __construct(").ok();

    // All params are Option<MappedType> — PHP users can omit any field
    for (i, field) in typ.fields.iter().enumerate() {
        let mapped = type_mapper(&field.ty);
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        writeln!(out, "    {}: Option<{}>{}", field.name, mapped, comma).ok();
    }

    writeln!(out, ") -> Self {{").ok();
    writeln!(out, "    Self {{").ok();

    for field in &typ.fields {
        let is_optional_field = field.optional || matches!(&field.ty, TypeRef::Optional(_));
        if is_optional_field {
            // Struct field is Option<T>, param is Option<T> — pass through directly
            writeln!(out, "        {},", field.name).ok();
        } else {
            // Struct field is T, param is Option<T> — unwrap with default
            let default_str = default_value_for_field(field, "rust");
            writeln!(
                out,
                "        {}: {}.unwrap_or({}),",
                field.name, field.name, default_str
            )
            .ok();
        }
    }

    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

/// Generate a Rustler (Elixir) kwargs constructor for a type with `has_default`.
/// Accepts keyword list or map, applies defaults for missing fields.
pub fn gen_rustler_kwargs_constructor(typ: &TypeDef, _type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);

    // NifStruct already handles keyword list conversion, but we generate
    // an explicit constructor wrapper that applies defaults.
    writeln!(
        out,
        "pub fn new(opts: std::collections::HashMap<String, rustler::Term>) -> Self {{"
    )
    .ok();
    writeln!(out, "    Self {{").ok();

    // Field assignments with defaults from opts
    for field in &typ.fields {
        let default_str = default_value_for_field(field, "rust");
        writeln!(
            out,
            "        {}: opts.get(\"{}\").and_then(|t| t.decode()).unwrap_or({}),",
            field.name, field.name, default_str
        )
        .ok();
    }

    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

/// Generate an extendr (R) kwargs constructor for a type with `has_default`.
/// Generates an R-callable function accepting named parameters with defaults.
pub fn gen_extendr_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);

    writeln!(out, "#[extendr]").ok();
    writeln!(out, "pub fn new_{}(", typ.name.to_lowercase()).ok();

    // Add all fields as named parameters with defaults
    for (i, field) in typ.fields.iter().enumerate() {
        let field_type = type_mapper(&field.ty);
        let default_str = default_value_for_field(field, "r");
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        writeln!(out, "    {}: {} = {}{}", field.name, field_type, default_str, comma).ok();
    }

    writeln!(out, ") -> {} {{", typ.name).ok();
    writeln!(out, "    {} {{", typ.name).ok();

    // Field assignments
    for field in &typ.fields {
        writeln!(out, "        {},", field.name).ok();
    }

    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::{FieldDef, PrimitiveType, TypeRef};

    fn make_test_type() -> TypeDef {
        TypeDef {
            name: "Config".to_string(),
            rust_path: "my_crate::Config".to_string(),
            fields: vec![
                FieldDef {
                    name: "timeout".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::U64),
                    optional: false,
                    default: Some("30".to_string()),
                    doc: "Timeout in seconds".to_string(),
                    sanitized: false,
                    is_boxed: false,
                    type_rust_path: None,
                    cfg: None,
                    typed_default: Some(DefaultValue::IntLiteral(30)),
                },
                FieldDef {
                    name: "enabled".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::Bool),
                    optional: false,
                    default: None,
                    doc: "Enable feature".to_string(),
                    sanitized: false,
                    is_boxed: false,
                    type_rust_path: None,
                    cfg: None,
                    typed_default: Some(DefaultValue::BoolLiteral(true)),
                },
                FieldDef {
                    name: "name".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    doc: "Config name".to_string(),
                    sanitized: false,
                    is_boxed: false,
                    type_rust_path: None,
                    cfg: None,
                    typed_default: Some(DefaultValue::StringLiteral("default".to_string())),
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            doc: "Configuration type".to_string(),
            cfg: None,
            is_trait: false,
            has_default: true,
            has_stripped_cfg_fields: false,
            is_return_type: false,
        }
    }

    #[test]
    fn test_default_value_bool_true_python() {
        let field = FieldDef {
            name: "enabled".to_string(),
            ty: TypeRef::Primitive(PrimitiveType::Bool),
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::BoolLiteral(true)),
        };
        assert_eq!(default_value_for_field(&field, "python"), "True");
    }

    #[test]
    fn test_default_value_bool_false_go() {
        let field = FieldDef {
            name: "enabled".to_string(),
            ty: TypeRef::Primitive(PrimitiveType::Bool),
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::BoolLiteral(false)),
        };
        assert_eq!(default_value_for_field(&field, "go"), "false");
    }

    #[test]
    fn test_default_value_string_literal() {
        let field = FieldDef {
            name: "name".to_string(),
            ty: TypeRef::String,
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::StringLiteral("hello".to_string())),
        };
        assert_eq!(default_value_for_field(&field, "python"), "\"hello\"");
        assert_eq!(default_value_for_field(&field, "java"), "\"hello\"");
    }

    #[test]
    fn test_default_value_int_literal() {
        let field = FieldDef {
            name: "timeout".to_string(),
            ty: TypeRef::Primitive(PrimitiveType::U64),
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::IntLiteral(42)),
        };
        let result = default_value_for_field(&field, "python");
        assert_eq!(result, "42");
    }

    #[test]
    fn test_default_value_none() {
        let field = FieldDef {
            name: "maybe".to_string(),
            ty: TypeRef::Optional(Box::new(TypeRef::String)),
            optional: true,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::None),
        };
        assert_eq!(default_value_for_field(&field, "python"), "None");
        assert_eq!(default_value_for_field(&field, "go"), "nil");
        assert_eq!(default_value_for_field(&field, "java"), "null");
        assert_eq!(default_value_for_field(&field, "csharp"), "null");
    }

    #[test]
    fn test_default_value_fallback_string() {
        let field = FieldDef {
            name: "name".to_string(),
            ty: TypeRef::String,
            optional: false,
            default: Some("\"custom\"".to_string()),
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: None,
        };
        assert_eq!(default_value_for_field(&field, "python"), "\"custom\"");
    }

    #[test]
    fn test_gen_pyo3_kwargs_constructor() {
        let typ = make_test_type();
        let output = gen_pyo3_kwargs_constructor(&typ, &|tr: &TypeRef| match tr {
            TypeRef::Primitive(p) => format!("{:?}", p),
            TypeRef::String | TypeRef::Char => "str".to_string(),
            _ => "Any".to_string(),
        });

        assert!(output.contains("#[new]"));
        assert!(output.contains("#[pyo3(signature = ("));
        assert!(output.contains("timeout=30"));
        assert!(output.contains("enabled=True"));
        assert!(output.contains("name=\"default\""));
        assert!(output.contains("fn new("));
    }

    #[test]
    fn test_gen_napi_defaults_constructor() {
        let typ = make_test_type();
        let output = gen_napi_defaults_constructor(&typ, &|tr: &TypeRef| match tr {
            TypeRef::Primitive(p) => format!("{:?}", p),
            TypeRef::String | TypeRef::Char => "String".to_string(),
            _ => "Value".to_string(),
        });

        assert!(output.contains("pub fn new(mut env: napi::Env, obj: napi::Object)"));
        assert!(output.contains("timeout"));
        assert!(output.contains("enabled"));
        assert!(output.contains("name"));
    }

    #[test]
    fn test_gen_go_functional_options() {
        let typ = make_test_type();
        let output = gen_go_functional_options(&typ, &|tr: &TypeRef| match tr {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::U64 => "uint64".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                _ => "interface{}".to_string(),
            },
            TypeRef::String | TypeRef::Char => "string".to_string(),
            _ => "interface{}".to_string(),
        });

        assert!(output.contains("type Config struct {"));
        assert!(output.contains("type ConfigOption func(*Config)"));
        assert!(output.contains("func WithTimeout(val uint64) ConfigOption"));
        assert!(output.contains("func WithEnabled(val bool) ConfigOption"));
        assert!(output.contains("func WithName(val string) ConfigOption"));
        assert!(output.contains("func NewConfig(opts ...ConfigOption) *Config"));
    }

    #[test]
    fn test_gen_java_builder() {
        let typ = make_test_type();
        let output = gen_java_builder(&typ, "dev.test", &|tr: &TypeRef| match tr {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::U64 => "long".to_string(),
                PrimitiveType::Bool => "boolean".to_string(),
                _ => "int".to_string(),
            },
            TypeRef::String | TypeRef::Char => "String".to_string(),
            _ => "Object".to_string(),
        });

        assert!(output.contains("package dev.test;"));
        assert!(output.contains("public class ConfigBuilder"));
        assert!(output.contains("withTimeout"));
        assert!(output.contains("withEnabled"));
        assert!(output.contains("withName"));
        assert!(output.contains("public Config build()"));
    }

    #[test]
    fn test_gen_csharp_record() {
        let typ = make_test_type();
        let output = gen_csharp_record(&typ, "MyNamespace", &|tr: &TypeRef| match tr {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::U64 => "ulong".to_string(),
                PrimitiveType::Bool => "bool".to_string(),
                _ => "int".to_string(),
            },
            TypeRef::String | TypeRef::Char => "string".to_string(),
            _ => "object".to_string(),
        });

        assert!(output.contains("namespace MyNamespace;"));
        assert!(output.contains("public record Config"));
        assert!(output.contains("public ulong Timeout"));
        assert!(output.contains("public bool Enabled"));
        assert!(output.contains("public string Name"));
        assert!(output.contains("init;"));
    }

    #[test]
    fn test_default_value_float_literal() {
        let field = FieldDef {
            name: "ratio".to_string(),
            ty: TypeRef::Primitive(PrimitiveType::F64),
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: Some(DefaultValue::FloatLiteral(1.5)),
        };
        let result = default_value_for_field(&field, "python");
        assert!(result.contains("1.5"));
    }

    #[test]
    fn test_default_value_no_typed_no_default() {
        let field = FieldDef {
            name: "count".to_string(),
            ty: TypeRef::Primitive(PrimitiveType::U32),
            optional: false,
            default: None,
            doc: String::new(),
            sanitized: false,
            is_boxed: false,
            type_rust_path: None,
            cfg: None,
            typed_default: None,
        };
        // Should fall back to type-based zero value
        assert_eq!(default_value_for_field(&field, "python"), "0");
        assert_eq!(default_value_for_field(&field, "go"), "0");
    }
}
