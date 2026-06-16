use crate::codegen::shared::binding_fields;
use crate::core::config::{Language, ResolvedCrateConfig};
use crate::core::ir::{ApiSurface, TypeDef};
use crate::docs::descriptions::generate_field_description;
use crate::docs::doc_cleaning::{clean_doc_inline, demote_headings};
use crate::docs::formatting::{doc_type_with_optional, escape_table_cell, format_field_default};
use crate::docs::naming::{field_name, type_name};
use crate::docs::{clean_doc, template_env};

use super::function_render::push_version_annotation;
use super::streaming::{method_visible_in_lang, render_method};

pub(super) fn render_type(
    ty: &TypeDef,
    lang: Language,
    config: &ResolvedCrateConfig,
    api: &ApiSurface,
    ffi_prefix: &str,
) -> String {
    let mut out = String::new();
    let tname = type_name(&ty.name, lang, ffi_prefix);

    out.push_str(&template_env::render(
        "heading.jinja",
        minijinja::context! { marker => "####", title => tname },
    ));

    push_version_annotation(&mut out, &ty.version);

    let doc = clean_doc(&ty.doc, lang);
    // Demote any embedded headings in the type documentation by 2 levels
    // to ensure they stay nested under the type heading (####).
    let doc = demote_headings(&doc, 2);
    if !doc.is_empty() {
        out.push_str(&doc);
        out.push('\n');
        out.push('\n');
    }

    // Fields table (only for non-opaque types or opaque types with documented fields)
    let fields: Vec<_> = if lang == Language::Rust {
        ty.fields.iter().collect()
    } else {
        binding_fields(&ty.fields).collect()
    };
    if !ty.is_opaque && !fields.is_empty() {
        out.push('\n');
        out.push_str("| Field | Type | Default | Description |\n");
        out.push_str("|-------|------|---------|-------------|\n");
        for field in fields {
            let fname = field_name(&field.name, lang);
            let fty = doc_type_with_optional(&field.ty, lang, field.optional, ffi_prefix);
            let fdefault = format_field_default(field, lang, api, ffi_prefix);
            let fdoc = {
                let raw = clean_doc_inline(&field.doc, lang);
                if raw.is_empty() {
                    generate_field_description(&field.name, &field.ty)
                } else {
                    raw
                }
            };
            out.push_str(&template_env::render(
                "field_row.jinja",
                minijinja::context! {
                    name => escape_table_cell(&fname),
                    ty => escape_table_cell(&fty),
                    default => escape_table_cell(&fdefault),
                    doc => escape_table_cell(&fdoc),
                },
            ));
        }
        out.push('\n');
    }

    // Methods (called "Functions" in Elixir)
    let methods: Vec<_> = ty
        .methods
        .iter()
        .filter(|method| method_visible_in_lang(config, method, &ty.name, lang))
        .collect();
    if !methods.is_empty() {
        let methods_heading = if lang == Language::Elixir {
            "Functions"
        } else {
            "Methods"
        };
        out.push_str(&template_env::render(
            "heading.jinja",
            minijinja::context! { marker => "#####", title => methods_heading },
        ));
        for method in methods {
            out.push_str(&render_method(method, &ty.name, lang, config, ffi_prefix));
        }
    }

    out
}
