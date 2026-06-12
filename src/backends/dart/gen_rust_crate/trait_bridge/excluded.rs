use crate::codegen::naming;
use crate::core::config::Language;
use crate::core::ir::{ApiSurface, TypeRef};

pub(super) fn excluded_carrier_name(type_name: &str) -> String {
    format!(
        "{}Bridge",
        naming::public_host_identifier(Language::Dart, naming::PublicIdentifierKind::Type, type_name)
    )
}

fn needs_excluded_carrier(ty: &TypeRef, excluded_type_paths: &std::collections::HashMap<String, String>) -> bool {
    match ty {
        TypeRef::Named(name) => excluded_type_paths.contains_key(name),
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => needs_excluded_carrier(inner, excluded_type_paths),
        TypeRef::Map(key, value) => {
            needs_excluded_carrier(key, excluded_type_paths) || needs_excluded_carrier(value, excluded_type_paths)
        }
        _ => false,
    }
}

fn replace_token(input: &str, needle: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(index) = rest.find(needle) {
        let (before, after_start) = rest.split_at(index);
        out.push_str(before);
        let after = &after_start[needle.len()..];
        let before_ok = out.chars().last().is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after_ok = after.chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before_ok && after_ok {
            out.push_str(replacement);
        } else {
            out.push_str(needle);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Substitute excluded concrete types with JSON-backed carrier types in binding-facing
/// closure signatures. The exact type names come from the extracted API, not from
/// downstream project conventions.
pub(super) fn substitute_excluded_carriers_in_rust_type(
    rust_type: &str,
    source_crate_name: &str,
    excluded_type_paths: &std::collections::HashMap<String, String>,
) -> String {
    let mut rendered = rust_type.to_string();
    for (type_name, path) in excluded_type_paths {
        let carrier = excluded_carrier_name(type_name);
        if !path.is_empty() {
            let normalized_path = path.replace('-', "_");
            rendered = rendered.replace(&normalized_path, &carrier);
        }
        let partial_qualified = format!("{source_crate_name}::{type_name}");
        rendered = rendered.replace(&partial_qualified, &carrier);
        rendered = replace_token(&rendered, type_name, &carrier);
    }
    rendered
}

pub(crate) fn needs_excluded_bridge_type(
    ty: &TypeRef,
    excluded_type_paths: &std::collections::HashMap<String, String>,
) -> bool {
    needs_excluded_carrier(ty, excluded_type_paths)
}

pub(crate) fn emit_excluded_bridge_types(out: &mut String, api: &ApiSurface) {
    let mut carriers = std::collections::BTreeSet::new();
    for trait_def in api.types.iter().filter(|t| t.is_trait) {
        for method in &trait_def.methods {
            for param in &method.params {
                collect_excluded_carriers(&param.ty, &api.excluded_type_paths, &mut carriers);
            }
            collect_excluded_carriers(&method.return_type, &api.excluded_type_paths, &mut carriers);
        }
    }
    for (type_name, carrier_name) in carriers {
        out.push_str(&crate::backends::dart::template_env::render(
            "rust_excluded_carrier.rs.jinja",
            minijinja::context! {
                type_name => type_name.as_str(),
                carrier_name => carrier_name.as_str(),
            },
        ));
    }
}

fn collect_excluded_carriers(
    ty: &TypeRef,
    excluded_type_paths: &std::collections::HashMap<String, String>,
    carriers: &mut std::collections::BTreeSet<(String, String)>,
) {
    match ty {
        TypeRef::Named(name) if excluded_type_paths.contains_key(name) => {
            carriers.insert((name.clone(), excluded_carrier_name(name)));
        }
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => {
            collect_excluded_carriers(inner, excluded_type_paths, carriers)
        }
        TypeRef::Map(key, value) => {
            collect_excluded_carriers(key, excluded_type_paths, carriers);
            collect_excluded_carriers(value, excluded_type_paths, carriers);
        }
        _ => {}
    }
}

pub(super) fn excluded_type_core_path(
    name: &str,
    source_crate_name: &str,
    excluded_type_paths: &std::collections::HashMap<String, String>,
) -> String {
    excluded_type_paths
        .get(name)
        .filter(|p| !p.is_empty())
        .map(|p| p.replace('-', "_"))
        .unwrap_or_else(|| format!("{source_crate_name}::{name}"))
}
