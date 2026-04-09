use heck::{ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};

/// Convert a Rust snake_case name to the target language convention.
pub fn to_python_name(name: &str) -> String {
    name.to_snake_case()
}

/// Convert a Rust snake_case name to Node.js/TypeScript lowerCamelCase convention.
pub fn to_node_name(name: &str) -> String {
    name.to_lower_camel_case()
}

/// Convert a Rust snake_case name to Ruby snake_case convention.
pub fn to_ruby_name(name: &str) -> String {
    name.to_snake_case()
}

/// Convert a Rust snake_case name to PHP lowerCamelCase convention.
pub fn to_php_name(name: &str) -> String {
    name.to_lower_camel_case()
}

/// Convert a Rust snake_case name to Elixir snake_case convention.
pub fn to_elixir_name(name: &str) -> String {
    name.to_snake_case()
}

/// Convert a Rust snake_case name to Go PascalCase convention.
pub fn to_go_name(name: &str) -> String {
    name.to_pascal_case()
}

/// Convert a Rust snake_case name to Java lowerCamelCase convention.
pub fn to_java_name(name: &str) -> String {
    name.to_lower_camel_case()
}

/// Convert a Rust snake_case name to C# PascalCase convention.
pub fn to_csharp_name(name: &str) -> String {
    name.to_pascal_case()
}

/// Convert a Rust name to a C-style prefixed snake_case identifier (e.g. `prefix_name`).
pub fn to_c_name(prefix: &str, name: &str) -> String {
    format!("{}_{}", prefix, name.to_snake_case())
}

/// Convert a Rust type name to class name convention for target language.
pub fn to_class_name(name: &str) -> String {
    name.to_pascal_case()
}

/// Convert to SCREAMING_SNAKE for constants.
pub fn to_constant_name(name: &str) -> String {
    name.to_shouty_snake_case()
}
