use alef_core::ir::{EnumDef, TypeDef};

use super::conversions::{frb_rust_type, frb_rust_type_inner};

pub(crate) fn emit_mirror_struct(out: &mut String, ty: &TypeDef) {
    use crate::template_env;
    // FRB v2 mirror convention: the mirror struct keeps the same name as the
    // original; the `mirror` attribute argument tells FRB which type this
    // declaration shadows for codegen purposes.
    out.push_str(&template_env::render(
        "rust_mirror_struct_attribute.jinja",
        minijinja::context! {
            name => ty.name.as_str(),
        },
    ));
    out.push_str(&template_env::render(
        "rust_mirror_struct_open.jinja",
        minijinja::context! {
            name => ty.name.as_str(),
        },
    ));
    for field in &ty.fields {
        let rust_ty = frb_rust_type(&field.ty, field.optional);
        out.push_str(&template_env::render(
            "rust_mirror_struct_field.jinja",
            minijinja::context! {
                field_name => field.name.as_str(),
                rust_ty => rust_ty,
            },
        ));
    }
    out.push_str(&template_env::render(
        "rust_mirror_struct_close.jinja",
        minijinja::context! {},
    ));
}

pub(crate) fn emit_mirror_enum(out: &mut String, en: &EnumDef) {
    use crate::template_env;
    let all_unit = en.variants.iter().all(|v| v.fields.is_empty());
    out.push_str(&template_env::render(
        "rust_mirror_enum_attribute.jinja",
        minijinja::context! {
            name => en.name.as_str(),
        },
    ));
    if all_unit {
        out.push_str(&template_env::render(
            "rust_mirror_enum_open.jinja",
            minijinja::context! {
                name => en.name.as_str(),
            },
        ));
        for variant in &en.variants {
            out.push_str(&template_env::render(
                "rust_mirror_enum_unit_variant.jinja",
                minijinja::context! {
                    variant_name => variant.name.as_str(),
                },
            ));
        }
        out.push_str("}\n");
    } else {
        out.push_str(&template_env::render(
            "rust_mirror_enum_open.jinja",
            minijinja::context! {
                name => en.name.as_str(),
            },
        ));
        for variant in &en.variants {
            if variant.fields.is_empty() {
                out.push_str(&template_env::render(
                    "rust_mirror_enum_unit_variant.jinja",
                    minijinja::context! {
                        variant_name => variant.name.as_str(),
                    },
                ));
            } else {
                out.push_str(&template_env::render(
                    "rust_mirror_enum_data_variant_open.jinja",
                    minijinja::context! {
                        variant_name => variant.name.as_str(),
                    },
                ));
                for (idx, f) in variant.fields.iter().enumerate() {
                    // Tuple-variant fields land in the IR as "_0", "_1", ... but
                    // flutter_rust_bridge strips a leading underscore from Dart
                    // field names — leaving an invalid bare digit. Rename any
                    // empty or "_N"-style field to a Dart-safe "fieldN".
                    let fname = if f.name.is_empty() || f.name.starts_with('_') {
                        format!("field{idx}")
                    } else {
                        f.name.clone()
                    };
                    let rust_ty = frb_rust_type_inner(&f.ty);
                    out.push_str(&template_env::render(
                        "rust_mirror_enum_data_variant_field.jinja",
                        minijinja::context! {
                            field_name => fname,
                            rust_ty => rust_ty,
                        },
                    ));
                }
                out.push_str(&template_env::render(
                    "rust_mirror_enum_data_close.jinja",
                    minijinja::context! {},
                ));
            }
        }
        out.push_str("}\n");
    }
}
