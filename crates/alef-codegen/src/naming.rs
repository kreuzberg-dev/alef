use heck::{ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};

/// Convert a Rust snake_case name to the target language convention.
pub fn to_python_name(name: &str) -> String {
    name.to_snake_case()
}

pub fn to_node_name(name: &str) -> String {
    name.to_lower_camel_case()
}

pub fn to_ruby_name(name: &str) -> String {
    name.to_snake_case()
}

pub fn to_php_name(name: &str) -> String {
    name.to_lower_camel_case()
}

pub fn to_elixir_name(name: &str) -> String {
    name.to_snake_case()
}

pub fn to_go_name(name: &str) -> String {
    name.to_pascal_case()
}

pub fn to_java_name(name: &str) -> String {
    name.to_lower_camel_case()
}

pub fn to_csharp_name(name: &str) -> String {
    name.to_pascal_case()
}

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
