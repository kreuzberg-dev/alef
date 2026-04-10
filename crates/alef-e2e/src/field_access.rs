//! Field path resolution for nested struct/map access in e2e assertions.
//!
//! The `FieldResolver` maps fixture field paths (e.g., "metadata.title") to
//! actual API struct paths (e.g., "metadata.document.title") and generates
//! language-specific accessor expressions.

use heck::{ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use std::collections::{HashMap, HashSet};

/// Resolves fixture field paths to language-specific accessor expressions.
pub struct FieldResolver {
    aliases: HashMap<String, String>,
    optional_fields: HashSet<String>,
}

/// A parsed segment of a field path.
#[derive(Debug, Clone)]
enum PathSegment {
    /// Struct field access: `foo`
    Field(String),
    /// Map/dict key access: `foo[key]`
    MapAccess { field: String, key: String },
}

impl FieldResolver {
    /// Create a new resolver from the e2e config's `fields` aliases and
    /// `fields_optional` set.
    pub fn new(fields: &HashMap<String, String>, optional: &HashSet<String>) -> Self {
        Self {
            aliases: fields.clone(),
            optional_fields: optional.clone(),
        }
    }

    /// Resolve a fixture field path to the actual struct path.
    /// Falls back to the field itself if no alias exists.
    pub fn resolve<'a>(&'a self, fixture_field: &'a str) -> &'a str {
        self.aliases
            .get(fixture_field)
            .map(String::as_str)
            .unwrap_or(fixture_field)
    }

    /// Check if a resolved field path is optional.
    pub fn is_optional(&self, field: &str) -> bool {
        self.optional_fields.contains(field)
    }

    /// Check if a fixture field has an explicit alias mapping.
    pub fn has_alias(&self, fixture_field: &str) -> bool {
        self.aliases.contains_key(fixture_field)
    }

    /// Generate a language-specific accessor expression.
    /// `result_var` is the variable holding the function return value.
    pub fn accessor(&self, fixture_field: &str, language: &str, result_var: &str) -> String {
        let resolved = self.resolve(fixture_field);
        let segments = parse_path(resolved);
        render_accessor(&segments, language, result_var)
    }

    /// Generate a Rust variable binding that unwraps an Optional string field.
    /// Returns `(binding_line, local_var_name)` or `None` if the field is not optional.
    pub fn rust_unwrap_binding(&self, fixture_field: &str, result_var: &str) -> Option<(String, String)> {
        let resolved = self.resolve(fixture_field);
        if !self.is_optional(resolved) {
            return None;
        }
        let segments = parse_path(resolved);
        let local_var = resolved.replace(['.', '['], "_").replace(']', "");
        let accessor = render_accessor(&segments, "rust", result_var);
        // Map access (.get("key").map(|s| s.as_str())) already returns Option<&str>,
        // so skip .as_deref() to avoid borrowing from a temporary.
        let has_map_access = segments.iter().any(|s| matches!(s, PathSegment::MapAccess { .. }));
        let binding = if has_map_access {
            format!("let {local_var} = {accessor}.unwrap_or(\"\");")
        } else {
            format!("let {local_var} = {accessor}.as_deref().unwrap_or(\"\");")
        };
        Some((binding, local_var))
    }
}

/// Parse a dotted field path into segments, handling map access `foo[key]`.
fn parse_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    for part in path.split('.') {
        if let Some(bracket_pos) = part.find('[') {
            let field = part[..bracket_pos].to_string();
            let key = part[bracket_pos + 1..].trim_end_matches(']').to_string();
            segments.push(PathSegment::MapAccess { field, key });
        } else {
            segments.push(PathSegment::Field(part.to_string()));
        }
    }
    segments
}

/// Render an accessor expression for the given language.
fn render_accessor(segments: &[PathSegment], language: &str, result_var: &str) -> String {
    match language {
        "rust" => render_rust(segments, result_var),
        "python" => render_dot_access(segments, result_var, false),
        "typescript" | "node" => render_typescript(segments, result_var),
        "go" => render_go(segments, result_var),
        "java" => render_java(segments, result_var),
        "csharp" => render_pascal_dot(segments, result_var),
        "ruby" => render_dot_access(segments, result_var, false),
        "php" => render_php(segments, result_var),
        "elixir" => render_dot_access(segments, result_var, false),
        "r" => render_r(segments, result_var),
        "c" => render_c(segments, result_var),
        _ => render_dot_access(segments, result_var, false),
    }
}

// ---------------------------------------------------------------------------
// Per-language renderers
// ---------------------------------------------------------------------------

/// Rust: `result.foo.bar.baz` or `result.foo.bar.get("key").map(|s| s.as_str())`
fn render_rust(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(&f.to_snake_case());
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(&field.to_snake_case());
                out.push_str(&format!(".get(\"{key}\").map(|s| s.as_str())"));
            }
        }
    }
    out
}

/// Simple dot access (Python, Ruby, Elixir): `result.foo.bar.baz`
fn render_dot_access(segments: &[PathSegment], result_var: &str, _pascal: bool) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(f);
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(field);
                out.push_str(&format!(".get(\"{key}\")"));
            }
        }
    }
    out
}

/// TypeScript/Node: `result.foo.bar.baz` or `result.foo.bar["key"]`
/// NAPI-RS generates camelCase field names, so snake_case segments are converted.
fn render_typescript(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(&f.to_lower_camel_case());
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(&field.to_lower_camel_case());
                out.push_str(&format!("[\"{key}\"]"));
            }
        }
    }
    out
}

/// Go: `result.Foo.Bar.Baz` (PascalCase) or `result.Foo.Bar["key"]`
fn render_go(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(&f.to_pascal_case());
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(&field.to_pascal_case());
                out.push_str(&format!("[\"{key}\"]"));
            }
        }
    }
    out
}

/// Java: `result.foo().bar().baz()` or `result.foo().bar().get("key")`
fn render_java(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(f);
                out.push_str("()");
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(field);
                out.push_str(&format!("().get(\"{key}\")"));
            }
        }
    }
    out
}

/// C#: `result.Foo.Bar.Baz` (PascalCase properties)
fn render_pascal_dot(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('.');
                out.push_str(&f.to_pascal_case());
            }
            PathSegment::MapAccess { field, key } => {
                out.push('.');
                out.push_str(&field.to_pascal_case());
                out.push_str(&format!("[\"{key}\"]"));
            }
        }
    }
    out
}

/// PHP: `$result->foo->bar->baz` or `$result->foo->bar["key"]`
fn render_php(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push_str("->");
                out.push_str(f);
            }
            PathSegment::MapAccess { field, key } => {
                out.push_str("->");
                out.push_str(field);
                out.push_str(&format!("[\"{key}\"]"));
            }
        }
    }
    out
}

/// R: `result$foo$bar$baz` or `result$foo$bar[["key"]]`
fn render_r(segments: &[PathSegment], result_var: &str) -> String {
    let mut out = result_var.to_string();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => {
                out.push('$');
                out.push_str(f);
            }
            PathSegment::MapAccess { field, key } => {
                out.push('$');
                out.push_str(field);
                out.push_str(&format!("[[\"{key}\"]]"));
            }
        }
    }
    out
}

/// C FFI: `{prefix}_result_foo_bar_baz({result})` accessor function style.
fn render_c(segments: &[PathSegment], result_var: &str) -> String {
    let mut parts = Vec::new();
    for seg in segments {
        match seg {
            PathSegment::Field(f) => parts.push(f.to_snake_case()),
            PathSegment::MapAccess { field, key } => {
                parts.push(field.to_snake_case());
                parts.push(key.clone());
            }
        }
    }
    let suffix = parts.join("_");
    format!("result_{suffix}({result_var})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resolver() -> FieldResolver {
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), "metadata.document.title".to_string());
        fields.insert("tags".to_string(), "metadata.tags[name]".to_string());
        fields.insert("og".to_string(), "metadata.document.open_graph".to_string());
        fields.insert("twitter".to_string(), "metadata.document.twitter_card".to_string());
        fields.insert("canonical".to_string(), "metadata.document.canonical_url".to_string());
        fields.insert("og_tag".to_string(), "metadata.open_graph_tags[og_title]".to_string());

        let mut optional = HashSet::new();
        optional.insert("metadata.document.title".to_string());

        FieldResolver::new(&fields, &optional)
    }

    #[test]
    fn test_resolve_alias() {
        let r = make_resolver();
        assert_eq!(r.resolve("title"), "metadata.document.title");
    }

    #[test]
    fn test_resolve_passthrough() {
        let r = make_resolver();
        assert_eq!(r.resolve("content"), "content");
    }

    #[test]
    fn test_is_optional() {
        let r = make_resolver();
        assert!(r.is_optional("metadata.document.title"));
        assert!(!r.is_optional("content"));
    }

    #[test]
    fn test_accessor_rust_struct() {
        let r = make_resolver();
        assert_eq!(r.accessor("title", "rust", "result"), "result.metadata.document.title");
    }

    #[test]
    fn test_accessor_rust_map() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("tags", "rust", "result"),
            "result.metadata.tags.get(\"name\").map(|s| s.as_str())"
        );
    }

    #[test]
    fn test_accessor_python() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "python", "result"),
            "result.metadata.document.title"
        );
    }

    #[test]
    fn test_accessor_go() {
        let r = make_resolver();
        assert_eq!(r.accessor("title", "go", "result"), "result.Metadata.Document.Title");
    }

    #[test]
    fn test_accessor_typescript() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "typescript", "result"),
            "result.metadata.document.title"
        );
    }

    #[test]
    fn test_accessor_typescript_snake_to_camel() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("og", "typescript", "result"),
            "result.metadata.document.openGraph"
        );
        assert_eq!(
            r.accessor("twitter", "typescript", "result"),
            "result.metadata.document.twitterCard"
        );
        assert_eq!(
            r.accessor("canonical", "typescript", "result"),
            "result.metadata.document.canonicalUrl"
        );
    }

    #[test]
    fn test_accessor_typescript_map_snake_to_camel() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("og_tag", "typescript", "result"),
            "result.metadata.openGraphTags[\"og_title\"]"
        );
    }

    #[test]
    fn test_accessor_node_alias() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("og", "node", "result"),
            "result.metadata.document.openGraph"
        );
    }

    #[test]
    fn test_accessor_java() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "java", "result"),
            "result.metadata().document().title()"
        );
    }

    #[test]
    fn test_accessor_csharp() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "csharp", "result"),
            "result.Metadata.Document.Title"
        );
    }

    #[test]
    fn test_accessor_php() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "php", "$result"),
            "$result->metadata->document->title"
        );
    }

    #[test]
    fn test_accessor_r() {
        let r = make_resolver();
        assert_eq!(r.accessor("title", "r", "result"), "result$metadata$document$title");
    }

    #[test]
    fn test_accessor_c() {
        let r = make_resolver();
        assert_eq!(
            r.accessor("title", "c", "result"),
            "result_metadata_document_title(result)"
        );
    }

    #[test]
    fn test_rust_unwrap_binding() {
        let r = make_resolver();
        let (binding, var) = r.rust_unwrap_binding("title", "result").unwrap();
        assert_eq!(var, "metadata_document_title");
        assert!(binding.contains("as_deref().unwrap_or(\"\")"));
    }

    #[test]
    fn test_rust_unwrap_binding_non_optional() {
        let r = make_resolver();
        assert!(r.rust_unwrap_binding("content", "result").is_none());
    }

    #[test]
    fn test_direct_field_no_alias() {
        let r = make_resolver();
        assert_eq!(r.accessor("content", "rust", "result"), "result.content");
        assert_eq!(r.accessor("content", "go", "result"), "result.Content");
    }
}
