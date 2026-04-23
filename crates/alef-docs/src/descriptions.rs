use alef_core::ir::TypeRef;

/// Generate a human-readable description for an enum variant from its name.
///
/// Handles both PascalCase (`SingleColumn` → "Single column") and
/// SCREAMING_CASE (`CODE_BLOCK` → "Code block element") variant names.
pub fn generate_enum_variant_description(variant_name: &str) -> String {
    if variant_name.is_empty() {
        return String::new();
    }

    // Well-known variant names with specific descriptions
    match variant_name {
        "TEXT" => return "Text format".to_string(),
        "MARKDOWN" => return "Markdown format".to_string(),
        "HTML" | "Html" => return "Preserve as HTML `<mark>` tags".to_string(),
        "JSON" => return "JSON format".to_string(),
        "CSV" => return "CSV format".to_string(),
        "XML" => return "XML format".to_string(),
        "PDF" => return "PDF format".to_string(),
        "PLAIN" => return "Plain text format".to_string(),
        _ => {}
    }

    // Detect SCREAMING_CASE: all uppercase/underscores/digits
    let is_screaming = variant_name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit());

    let words: Vec<String> = if is_screaming {
        // SCREAMING_CASE: split on underscores and lowercase
        variant_name
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|w| w.to_lowercase())
            .collect()
    } else {
        // PascalCase: split on uppercase boundaries
        let mut parts = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = variant_name.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c.is_uppercase() && !current.is_empty() {
                // Check for acronym runs (e.g., "OSD" in "AutoOsd" stays together
                // only if next char is also uppercase or we're at the end)
                let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                if next_is_lower && current.len() > 1 && current.chars().all(|ch| ch.is_uppercase()) {
                    // End of acronym run: split off all but last char
                    let last = current.pop().expect("current is non-empty");
                    if !current.is_empty() {
                        parts.push(current);
                    }
                    current = String::new();
                    current.push(last);
                } else {
                    parts.push(current);
                    current = String::new();
                }
            }
            current.push(c);
        }
        if !current.is_empty() {
            parts.push(current);
        }
        parts.into_iter().map(|w| w.to_lowercase()).collect()
    };

    if words.is_empty() {
        return String::new();
    }

    // Determine suffix based on category heuristics
    let joined = words.join(" ");
    let suffix = determine_enum_variant_suffix(&joined, is_screaming);

    // Capitalize first letter
    let mut chars = joined.chars();
    match chars.next() {
        Some(first) => {
            let capitalized = first.to_uppercase().to_string() + chars.as_str();
            if suffix.is_empty() {
                capitalized
            } else {
                format!("{capitalized} {suffix}")
            }
        }
        None => String::new(),
    }
}

/// Determine an appropriate suffix for an enum variant description.
pub fn determine_enum_variant_suffix(readable: &str, is_screaming: bool) -> &'static str {
    // Format-like variants
    let format_words = [
        "text", "markdown", "html", "json", "csv", "xml", "pdf", "yaml", "toml", "docx", "xlsx", "pptx", "rtf",
        "latex", "rst", "asciidoc", "epub",
    ];
    for w in &format_words {
        if readable == *w {
            return "format";
        }
    }

    // Element-like variants (common in SCREAMING_CASE block-level names)
    let element_words = [
        "heading",
        "paragraph",
        "blockquote",
        "table",
        "figure",
        "caption",
        "footnote",
        "header",
        "footer",
        "section",
        "title",
        "subtitle",
        "image",
    ];
    for w in &element_words {
        if readable == *w {
            return "element";
        }
    }

    // If it already ends with a category word, no suffix needed
    let no_suffix_endings = [
        "format", "mode", "type", "level", "style", "strategy", "method", "state", "status", "error", "element",
        "block", "list", "model",
    ];
    for ending in &no_suffix_endings {
        if readable.ends_with(ending) {
            return "";
        }
    }

    // SCREAMING_CASE compound names ending in list/block often describe elements
    if is_screaming && (readable.contains("list") || readable.contains("block") || readable.contains("item")) {
        return "";
    }

    // No category suffix applicable
    ""
}

/// Generate a human-readable description for an error variant from its PascalCase name.
///
/// Splits PascalCase into words and forms a sentence like "IO errors" or "Parsing errors".
pub fn generate_error_variant_description(variant_name: &str) -> String {
    // Split PascalCase into words
    let mut words = Vec::new();
    let mut current = String::new();
    for c in variant_name.chars() {
        if c.is_uppercase() && !current.is_empty() {
            words.push(current);
            current = String::new();
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }

    if words.is_empty() {
        return String::new();
    }

    // Join into readable form and add "errors" suffix
    let readable = words.join(" ").to_lowercase();
    // Capitalize first letter
    let mut chars = readable.chars();
    match chars.next() {
        Some(first) => {
            let capitalized = first.to_uppercase().to_string() + chars.as_str();
            format!("{capitalized} errors")
        }
        None => String::new(),
    }
}

/// Generate a human-readable field description from its name and type
/// when no explicit doc comment exists on a struct field.
pub fn generate_field_description(field_name: &str, type_ref: &TypeRef) -> String {
    // Well-known field names with specific descriptions
    match field_name {
        "content" => return "The extracted text content".to_string(),
        "mime_type" => return "The detected MIME type".to_string(),
        "metadata" => return "Document metadata".to_string(),
        "tables" => return "Tables extracted from the document".to_string(),
        "images" => return "Images extracted from the document".to_string(),
        "pages" => return "Per-page content".to_string(),
        "chunks" => return "Text chunks for chunking/embedding".to_string(),
        "elements" => return "Semantic document elements".to_string(),
        "name" => return "The name".to_string(),
        "path" => return "File path".to_string(),
        "description" => return "Human-readable description".to_string(),
        "version" => return "Version string".to_string(),
        "id" => return "Unique identifier".to_string(),
        "enabled" => return "Whether this feature is enabled".to_string(),
        "size" => return "Size in bytes".to_string(),
        "count" => return "Number of items".to_string(),
        _ => {}
    }

    // Prefix-based patterns
    if let Some(rest) = field_name.strip_suffix("_count") {
        let readable = rest.replace('_', " ");
        let pluralized = if readable.ends_with('s') {
            readable
        } else {
            format!("{readable}s")
        };
        return format!("Number of {pluralized}");
    }
    if let Some(rest) = field_name.strip_prefix("is_") {
        let readable = rest.replace('_', " ");
        return format!("Whether {readable}");
    }
    if let Some(rest) = field_name.strip_prefix("has_") {
        let readable = rest.replace('_', " ");
        return format!("Whether {readable}");
    }
    if let Some(rest) = field_name.strip_prefix("max_") {
        let readable = rest.replace('_', " ");
        return format!("Maximum {readable}");
    }
    if let Some(rest) = field_name.strip_prefix("min_") {
        let readable = rest.replace('_', " ");
        return format!("Minimum {readable}");
    }

    // For named types, use the type name for extra context
    if let TypeRef::Named(type_name) = type_ref {
        let readable_type = type_name.chars().enumerate().fold(String::new(), |mut acc, (i, c)| {
            if c.is_uppercase() && i > 0 {
                acc.push(' ');
                acc.push(c.to_ascii_lowercase());
            } else if i == 0 {
                acc.push(c.to_ascii_lowercase());
            } else {
                acc.push(c);
            }
            acc
        });
        // If the field name matches the type (e.g. field "metadata" of type "Metadata"),
        // we already handled it above, so this provides context for other combos.
        let readable_name = snake_to_readable(field_name);
        return format!("{readable_name} ({readable_type})");
    }

    // Default: convert snake_case to readable text
    snake_to_readable(field_name)
}

/// Convert a `snake_case` identifier to `Readable text` (capitalize first letter).
pub fn snake_to_readable(name: &str) -> String {
    let readable = name.replace('_', " ");
    let mut chars = readable.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Generate a human-readable parameter description from its name and type
/// when no explicit doc comment or `# Arguments` entry exists.
pub fn generate_param_description(name: &str, ty: &TypeRef) -> String {
    // Derive a readable noun phrase from the parameter name by splitting on underscores
    // and joining with spaces (e.g. "mime_type" → "MIME type", "config" → "configuration").
    let article = match name {
        // Common names that benefit from a specific description
        "config" | "configuration" => return "The configuration options".to_string(),
        "options" | "opts" => return "The options to use".to_string(),
        "path" | "file_path" => return "Path to the file".to_string(),
        "content" | "contents" => return "The content to process".to_string(),
        "input" => return "The input data".to_string(),
        "output" => return "The output destination".to_string(),
        "url" => return "The URL to fetch".to_string(),
        "timeout" => return "Timeout duration".to_string(),
        "callback" | "cb" => return "Callback function".to_string(),
        _ => "The",
    };

    // For named types, use the type name for context
    let type_hint = match ty {
        TypeRef::Named(type_name) => {
            // Convert PascalCase type name to readable form
            type_name.chars().enumerate().fold(String::new(), |mut acc, (i, c)| {
                if c.is_uppercase() && i > 0 {
                    acc.push(' ');
                    acc.push(c.to_ascii_lowercase());
                } else if i == 0 {
                    acc.push(c.to_ascii_lowercase());
                } else {
                    acc.push(c);
                }
                acc
            })
        }
        _ => {
            // For non-named types, use the param name as description
            name.replace('_', " ")
        }
    };

    format!("{article} {type_hint}")
}
