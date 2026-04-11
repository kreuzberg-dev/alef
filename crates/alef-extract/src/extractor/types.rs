use alef_core::ir::{CoreWrapper, DefaultValue, EnumDef, ErrorDef, ErrorVariant, FieldDef, TypeDef};
use syn;

use super::helpers::{detect_core_wrapper, detect_vec_inner_core_wrapper};
use crate::type_resolver;

use super::helpers::{
    build_rust_path, extract_cfg_condition, extract_doc_comments, extract_enum_variant, extract_error_message_template,
    extract_field, extract_field_type_rust_path, extract_serde_rename_all, has_cfg_attribute, has_derive,
    has_field_attr, is_pub, syn_type_is_boxed, unwrap_optional,
};

/// Extract a public struct into a `TypeDef`.
/// Returns `None` for generic structs — they can't be directly exposed to FFI.
pub(crate) fn extract_struct(item: &syn::ItemStruct, crate_name: &str, module_path: &str) -> Option<TypeDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let cfg = extract_cfg_condition(&item.attrs);
    let name = item.ident.to_string();

    // Detect single-field tuple structs (newtype wrappers like `pub struct Foo(String)`).
    // These get a single field named `_0` so the post-processing pass in `extract()`
    // can identify them and resolve `TypeRef::Named("Foo")` → inner type transparently.
    let mut fields = match &item.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .filter(|f| is_pub(&f.vis))
            .map(extract_field)
            .collect(),
        syn::Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 && is_pub(&unnamed.unnamed[0].vis) => {
            let field = &unnamed.unnamed[0];
            let resolved = type_resolver::resolve_type(&field.ty);
            let (ty, optional) = unwrap_optional(resolved);
            vec![FieldDef {
                name: "_0".to_string(),
                ty,
                optional,
                default: None,
                doc: String::new(),
                sanitized: false,
                is_boxed: syn_type_is_boxed(&field.ty),
                type_rust_path: extract_field_type_rust_path(&field.ty),
                cfg: None,
                typed_default: None,
                core_wrapper: detect_core_wrapper(&field.ty),
                vec_inner_core_wrapper: detect_vec_inner_core_wrapper(&field.ty),
            }]
        }
        _ => vec![],
    };

    let is_clone = has_derive(item.attrs.as_slice(), "Clone");
    let has_default = has_derive(item.attrs.as_slice(), "Default");
    let serde_rename_all = extract_serde_rename_all(&item.attrs);
    let doc = extract_doc_comments(&item.attrs);
    let is_opaque = fields.is_empty();
    let rust_path = build_rust_path(crate_name, module_path, &name);

    // #[derive(Default)] — all fields get DefaultValue::Empty (type's own Default)
    if has_default {
        for field in &mut fields {
            field.typed_default = Some(DefaultValue::Empty);
        }
    }

    let has_stripped_cfg_fields = fields.iter().any(|f| f.cfg.is_some());

    Some(TypeDef {
        rust_path,
        name,
        fields,
        methods: vec![],
        is_opaque,
        is_clone,
        is_trait: false,
        has_default,
        has_stripped_cfg_fields,
        is_return_type: false,
        doc,
        cfg,
        serde_rename_all,
    })
}

/// Extract a public enum into an `EnumDef`.
/// Returns `None` for generic enums — they can't be directly exposed to FFI.
pub(crate) fn extract_enum(item: &syn::ItemEnum, crate_name: &str, module_path: &str) -> Option<EnumDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let cfg = extract_cfg_condition(&item.attrs);
    let name = item.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);

    let variants = item.variants.iter().map(extract_enum_variant).collect();

    let rust_path = build_rust_path(crate_name, module_path, &name);

    Some(EnumDef {
        rust_path,
        name,
        variants,
        doc,
        cfg,
    })
}

/// Extract a `#[derive(thiserror::Error)]` enum into an `ErrorDef`.
/// Returns `None` for generic enums.
pub(crate) fn extract_error_enum(item: &syn::ItemEnum, crate_name: &str, module_path: &str) -> Option<ErrorDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let name = item.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);

    let variants = item
        .variants
        .iter()
        .filter(|v| !has_cfg_attribute(&v.attrs)) // Skip cfg-gated variants
        .map(|v| {
            let message_template = extract_error_message_template(&v.attrs);
            let variant_doc = extract_doc_comments(&v.attrs);

            let (fields, has_source, has_from, is_unit) = match &v.fields {
                syn::Fields::Named(named) => {
                    let mut source = false;
                    let mut from = false;
                    let fields: Vec<FieldDef> = named
                        .named
                        .iter()
                        .map(|f| {
                            if has_field_attr(&f.attrs, "source") {
                                source = true;
                            }
                            if has_field_attr(&f.attrs, "from") {
                                from = true;
                                source = true; // #[from] implies source
                            }
                            extract_field(f)
                        })
                        .collect();
                    (fields, source, from, false)
                }
                syn::Fields::Unnamed(unnamed) => {
                    let mut source = false;
                    let mut from = false;
                    let fields: Vec<FieldDef> = unnamed
                        .unnamed
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            if has_field_attr(&f.attrs, "source") {
                                source = true;
                            }
                            if has_field_attr(&f.attrs, "from") {
                                from = true;
                                source = true;
                            }
                            let ty = type_resolver::resolve_type(&f.ty);
                            let optional = type_resolver::is_option_type(&f.ty).is_some();
                            FieldDef {
                                name: format!("_{i}"),
                                ty,
                                optional,
                                default: None,
                                doc: extract_doc_comments(&f.attrs),
                                sanitized: false,
                                is_boxed: syn_type_is_boxed(&f.ty),
                                type_rust_path: extract_field_type_rust_path(&f.ty),
                                cfg: None,
                                typed_default: None,
                                core_wrapper: CoreWrapper::None,
                                vec_inner_core_wrapper: CoreWrapper::None,
                            }
                        })
                        .collect();
                    (fields, source, from, false)
                }
                syn::Fields::Unit => (vec![], false, false, true),
            };

            ErrorVariant {
                name: v.ident.to_string(),
                message_template,
                fields,
                has_source,
                has_from,
                is_unit,
                doc: variant_doc,
            }
        })
        .collect();

    let rust_path = build_rust_path(crate_name, module_path, &name);

    Some(ErrorDef {
        name,
        rust_path,
        variants,
        doc,
    })
}
