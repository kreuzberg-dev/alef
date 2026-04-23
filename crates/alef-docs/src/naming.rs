use alef_core::config::Language;
use heck::{ToPascalCase, ToShoutySnakeCase, ToSnakeCase, ToUpperCamelCase};

pub fn lang_display_name(lang: Language) -> &'static str {
    match lang {
        Language::Python => "Python",
        Language::Node => "TypeScript",
        Language::Ruby => "Ruby",
        Language::Php => "PHP",
        Language::Elixir => "Elixir",
        Language::Go => "Go",
        Language::Java => "Java",
        Language::Csharp => "C#",
        Language::Ffi => "C",
        Language::Wasm => "WebAssembly",
        Language::R => "R",
        Language::Rust => "Rust",
    }
}

/// Get the slug used in file names (e.g. `typescript` for `Node`).
pub fn lang_slug(lang: Language) -> &'static str {
    match lang {
        Language::Python => "python",
        Language::Node => "typescript",
        Language::Ruby => "ruby",
        Language::Php => "php",
        Language::Elixir => "elixir",
        Language::Go => "go",
        Language::Java => "java",
        Language::Csharp => "csharp",
        Language::Ffi => "c",
        Language::Wasm => "wasm",
        Language::R => "r",
        Language::Rust => "rust",
    }
}

/// Get the code fence language identifier.
pub fn lang_code_fence(lang: Language) -> &'static str {
    match lang {
        Language::Python => "python",
        Language::Node | Language::Wasm => "typescript",
        Language::Ruby => "ruby",
        Language::Php => "php",
        Language::Elixir => "elixir",
        Language::Go => "go",
        Language::Java => "java",
        Language::Csharp => "csharp",
        Language::Ffi => "c",
        Language::R => "r",
        Language::Rust => "rust",
    }
}

/// Convert a Rust type name to the idiomatic name for the target language.
pub fn type_name(name: &str, lang: Language, ffi_prefix: &str) -> String {
    // Strip module path prefix if present
    let short = name.rsplit("::").next().unwrap_or(name);
    match lang {
        Language::Python
        | Language::Node
        | Language::Wasm
        | Language::Ruby
        | Language::Go
        | Language::Java
        | Language::Csharp
        | Language::Php
        | Language::Elixir
        | Language::R
        | Language::Rust => short.to_pascal_case(),
        Language::Ffi => {
            // C: prefix with configured FFI prefix (PascalCase) and PascalCase type name
            format!("{}{}", ffi_prefix, short.to_pascal_case())
        }
    }
}

/// Convert a Rust function name to the idiomatic name for the target language.
pub fn func_name(name: &str, lang: Language, ffi_prefix: &str) -> String {
    let base = match lang {
        Language::Python | Language::Ruby | Language::Elixir | Language::R | Language::Rust => name.to_snake_case(),
        Language::Node | Language::Wasm | Language::Java | Language::Php => to_camel_case(name),
        Language::Csharp | Language::Go => name.to_pascal_case(),
        Language::Ffi => format!("{}_{}", ffi_prefix.to_snake_case(), name.to_snake_case()),
    };
    // Handle reserved keywords
    match (lang, base.as_str()) {
        (Language::Java, "default") => "defaultOptions".to_string(),
        (Language::Csharp, "Default") => "CreateDefault".to_string(),
        _ => base,
    }
}

/// Convert a Rust field name to the idiomatic name for the target language.
pub fn field_name(name: &str, lang: Language) -> String {
    match lang {
        Language::Python | Language::Ruby | Language::Elixir | Language::R | Language::Ffi | Language::Rust => {
            name.to_snake_case()
        }
        // Go and C# exported fields/properties are PascalCase
        Language::Go | Language::Csharp => name.to_pascal_case(),
        Language::Node | Language::Wasm | Language::Java | Language::Php => to_camel_case(name),
    }
}

/// Convert a Rust enum variant name to the idiomatic name for the target language.
pub fn enum_variant_name(name: &str, lang: Language, ffi_prefix: &str) -> String {
    // Special-case acronym variants that don't split cleanly
    if name == "RDFa" {
        return match lang {
            Language::Python | Language::Java => "RDFA".to_string(),
            Language::Ruby | Language::Elixir => "rdfa".to_string(),
            Language::R => "rdfa".to_string(),
            Language::Ffi => format!("{}_{}", ffi_prefix.to_shouty_snake_case(), "RDFA"),
            _ => "RDFa".to_string(),
        };
    }
    match lang {
        Language::Python => {
            // Python: UPPER_SNAKE_CASE
            name.to_shouty_snake_case()
        }
        Language::Java => {
            // Java: UPPER_SNAKE_CASE
            name.to_shouty_snake_case()
        }
        Language::Ruby | Language::Elixir => {
            // Ruby/Elixir: :snake_atom style
            name.to_snake_case()
        }
        Language::Go | Language::Node | Language::Wasm | Language::Csharp | Language::Php => name.to_pascal_case(),
        Language::R => name.to_snake_case(),
        // Rust: PascalCase enum variants
        Language::Rust => name.to_pascal_case(),
        Language::Ffi => format!("{}_{}", ffi_prefix.to_shouty_snake_case(), name.to_shouty_snake_case()),
    }
}

/// Convert snake_case or PascalCase to camelCase.
pub fn to_camel_case(s: &str) -> String {
    let pascal = s.to_upper_camel_case();
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().to_string() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Default value formatting
// ---------------------------------------------------------------------------
