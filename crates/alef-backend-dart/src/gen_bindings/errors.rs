use alef_codegen::error_gen::strip_thiserror_placeholders;
use alef_core::ir::ErrorDef;
use heck::ToLowerCamelCase;
use std::collections::BTreeSet;

use crate::ident::dart_safe_ident;

use super::render_type::render_type;

/// Escape a string for use inside a Dart single-quoted string literal.
///
/// Dart single-quoted strings interpret `\`, `'`, and `$` specially:
/// - `\` introduces an escape sequence → must be doubled.
/// - `'` terminates the literal → must be escaped as `\'`.
/// - `$` introduces string interpolation → must be escaped as `\$`.
fn escape_dart_string_literal(s: &str) -> String {
    s.replace('\\', r"\\").replace('\'', r"\'").replace('$', r"\$")
}

/// Build the runtime `message` string for a Dart exception variant.
///
/// Strips `thiserror`-style `{name}` placeholders so the host runtime never
/// surfaces literal substitution markers (`Parsing error: {message}` becomes
/// `Parsing error`). When the template is empty (or stripping leaves nothing)
/// falls back to the variant name to preserve some context.
fn build_message(variant_name: &str, template: Option<&str>) -> String {
    let raw = template.unwrap_or(variant_name);
    let stripped = strip_thiserror_placeholders(raw);
    if stripped.is_empty() {
        variant_name.to_string()
    } else {
        stripped
    }
}

pub(super) fn emit_error(error: &ErrorDef, out: &mut String, imports: &mut BTreeSet<String>) {
    if !error.doc.is_empty() {
        for line in error.doc.lines() {
            out.push_str("/// ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(&format!("sealed class {} implements Exception {{\n", error.name));
    out.push_str("  String get message;\n");
    out.push_str("}\n\n");
    for variant in &error.variants {
        if !variant.doc.is_empty() {
            for line in variant.doc.lines() {
                out.push_str("/// ");
                out.push_str(line);
                out.push('\n');
            }
        }
        if variant.is_unit {
            let raw_msg = build_message(&variant.name, variant.message_template.as_deref());
            let msg = escape_dart_string_literal(&raw_msg);
            out.push_str(&format!("final class {} implements {} {{\n", variant.name, error.name));
            out.push_str(&format!("  @override\n  String get message => '{msg}';\n"));
            out.push_str(&format!("  const {}();\n", variant.name));
            out.push_str("}\n");
        } else {
            out.push_str(&format!("final class {} implements {} {{\n", variant.name, error.name));
            for f in &variant.fields {
                let ty_str = render_type(&f.ty, imports);
                let fname = dart_safe_ident(&f.name.to_lower_camel_case());
                out.push_str(&format!("  final {ty_str} {fname};\n"));
            }
            let raw_msg = build_message(&variant.name, variant.message_template.as_deref());
            let msg = escape_dart_string_literal(&raw_msg);
            out.push_str("  @override\n");
            out.push_str(&format!("  String get message => '{msg}';\n"));
            if variant.fields.len() == 1 {
                let fname = dart_safe_ident(&variant.fields[0].name.to_lower_camel_case());
                out.push_str(&format!("  {}(this.{fname});\n", variant.name));
            } else {
                out.push_str(&format!("  {}({{\n", variant.name));
                for f in &variant.fields {
                    let fname = dart_safe_ident(&f.name.to_lower_camel_case());
                    out.push_str(&format!("    required this.{fname},\n"));
                }
                out.push_str("  });\n");
            }
            out.push_str("}\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_message_strips_placeholders() {
        assert_eq!(
            build_message("Parsing", Some("Parsing error: {message}")),
            "Parsing error"
        );
        assert_eq!(build_message("Ocr", Some("OCR error: {message}")), "OCR error");
        assert_eq!(
            build_message("Cancelled", Some("extraction cancelled")),
            "extraction cancelled"
        );
    }

    #[test]
    fn build_message_falls_back_when_stripped_empty() {
        assert_eq!(build_message("Other", Some("{message}")), "Other");
    }

    #[test]
    fn build_message_no_template_uses_variant_name() {
        assert_eq!(build_message("NotFound", None), "NotFound");
    }
}
