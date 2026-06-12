use crate::cli::registry;
use crate::core::config::{Language, ResolvedCrateConfig};
use crate::core::ir::{ApiSurface, TypeRef};
use crate::core::validation::{ValidatedApiSurface, ValidationCode, ValidationDiagnostic, ValidationSeverity};

pub(super) fn validate_generation_api<'a>(
    api: &'a ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> anyhow::Result<ValidatedApiSurface<'a>> {
    let bridged_trait_names: ahash::AHashSet<&str> = config
        .trait_bridges
        .iter()
        .map(|bridge| bridge.trait_name.as_str())
        .collect();
    let validation_report =
        crate::core::validation::validate_api_surface_with_bridged_traits(api, &bridged_trait_names);
    let language_diagnostics = language_backend_readiness_diagnostics(api, config, languages);
    for diagnostic in validation_report.warnings() {
        tracing::warn!("{diagnostic}");
    }
    for diagnostic in language_diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == ValidationSeverity::Warning)
    {
        tracing::warn!("{diagnostic}");
    }
    let fatal: Vec<_> = validation_report
        .errors()
        .filter(|diagnostic| {
            crate::core::validation::is_critical_unsuppressible(diagnostic.code)
                || !config
                    .suppress_validation_codes
                    .iter()
                    .any(|code| code == &diagnostic.code.to_string())
        })
        .collect();
    let fatal_language_diagnostics: Vec<_> = language_diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == ValidationSeverity::Error)
        .collect();
    for diagnostic in validation_report.errors().filter(|diagnostic| {
        !crate::core::validation::is_critical_unsuppressible(diagnostic.code)
            && config
                .suppress_validation_codes
                .iter()
                .any(|code| code == &diagnostic.code.to_string())
    }) {
        tracing::warn!("[suppressed] {diagnostic}");
    }
    if !fatal.is_empty() || !fatal_language_diagnostics.is_empty() {
        let formatted = fatal
            .iter()
            .copied()
            .chain(fatal_language_diagnostics.iter().copied())
            .map(|diagnostic| {
                let path = diagnostic
                    .item_path
                    .as_deref()
                    .map(|p| format!(" item `{p}`"))
                    .unwrap_or_default();
                format!("- [{}]{path} {}", diagnostic.code, diagnostic.reason)
            })
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("{formatted}");
    }
    ValidatedApiSurface::new_with_bridged_traits(api, &config.suppress_validation_codes, &bridged_trait_names)
        .map_err(|report| anyhow::anyhow!(report.format_errors()))
}

fn language_backend_readiness_diagnostics(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(service_api_capability_diagnostics(api, config, languages));
    diagnostics.extend(ffi_json_return_diagnostics(api, config, languages));
    diagnostics
}

fn service_api_capability_diagnostics(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> Vec<ValidationDiagnostic> {
    if api.services.is_empty() {
        return Vec::new();
    }

    languages
        .iter()
        .filter_map(|&language| {
            if !service_api_requested_for_language(api, config, language) {
                return None;
            }
            let backend = registry::try_get_backend(language)?;
            (!backend.capabilities().supports_service_api).then(|| ValidationDiagnostic {
                severity: ValidationSeverity::Error,
                code: ValidationCode::UnsupportedBackendCapability,
                crate_name: config.name.clone(),
                language: Some(language),
                item_path: Some("service_api".to_string()),
                reason: format!(
                    "configured services require service API generation, but backend `{}` does not support it",
                    backend.name()
                ),
                suggested_fix: "remove the language from this generation run, opt it out in service config, or implement service API support for the backend".to_string(),
            })
        })
        .collect()
}

fn service_api_requested_for_language(api: &ApiSurface, config: &ResolvedCrateConfig, language: Language) -> bool {
    api.services.iter().any(|service| {
        config
            .services
            .iter()
            .find(|service_config| service_config.owner_type == service.name)
            .is_none_or(|service_config| !service_config.skip_languages.contains(&language.to_string()))
    })
}

fn ffi_json_return_diagnostics(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> Vec<ValidationDiagnostic> {
    let readiness_languages: Vec<_> = languages
        .iter()
        .copied()
        .filter(|language| ffi_json_return_readiness_applies(*language))
        .collect();
    if readiness_languages.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for function in &api.functions {
        if function.binding_excluded {
            continue;
        }
        if non_serde_named_in_ffi_json_return(api, &function.return_type) {
            for language in &readiness_languages {
                diagnostics.push(ffi_json_return_diagnostic(
                    config,
                    *language,
                    &format!("function {}", function.name),
                    &function.return_type,
                ));
            }
        }
    }
    for typ in &api.types {
        if typ.binding_excluded {
            continue;
        }
        for method in &typ.methods {
            if method.binding_excluded {
                continue;
            }
            if non_serde_named_in_ffi_json_return(api, &method.return_type) {
                for language in &readiness_languages {
                    diagnostics.push(ffi_json_return_diagnostic(
                        config,
                        *language,
                        &format!("method {}.{}", typ.name, method.name),
                        &method.return_type,
                    ));
                }
            }
        }
    }
    diagnostics
}

fn ffi_json_return_readiness_applies(language: Language) -> bool {
    matches!(
        language,
        Language::Ffi
            | Language::Go
            | Language::Java
            | Language::Jni
            | Language::Csharp
            | Language::KotlinAndroid
            | Language::Swift
            | Language::R
            | Language::Zig
    )
}

fn non_serde_named_in_ffi_json_return(api: &ApiSurface, ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Vec(inner) => named_lacks_serde(api, inner),
        TypeRef::Map(key, value) => named_lacks_serde(api, key) || named_lacks_serde(api, value),
        TypeRef::Optional(inner) => non_serde_named_in_ffi_json_return(api, inner),
        _ => false,
    }
}

fn named_lacks_serde(api: &ApiSurface, ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Named(name) => {
            if let Some(typ) = api.types.iter().find(|typ| typ.name == *name) {
                return !typ.has_serde;
            }
            if let Some(enum_def) = api.enums.iter().find(|enum_def| enum_def.name == *name) {
                return !enum_def.has_serde;
            }
            false
        }
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => named_lacks_serde(api, inner),
        TypeRef::Map(key, value) => named_lacks_serde(api, key) || named_lacks_serde(api, value),
        _ => false,
    }
}

fn ffi_json_return_diagnostic(
    config: &ResolvedCrateConfig,
    language: Language,
    item_path: &str,
    return_type: &TypeRef,
) -> ValidationDiagnostic {
    ValidationDiagnostic {
        severity: ValidationSeverity::Error,
        code: ValidationCode::BackendStubPath,
        crate_name: config.name.clone(),
        language: Some(language),
        item_path: Some(item_path.to_string()),
        reason: format!(
            "FFI-dependent generation cannot safely marshal return type `{}` because a nested named type lacks serde metadata",
            type_ref_label(return_type)
        ),
        suggested_fix: "derive Serialize/Deserialize on the named return type, expose a binding-safe DTO, or exclude/bridge the item explicitly".to_string(),
    }
}

fn type_ref_label(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(name) => name.clone(),
        TypeRef::Vec(inner) => format!("Vec<{}>", type_ref_label(inner)),
        TypeRef::Optional(inner) => format!("Option<{}>", type_ref_label(inner)),
        TypeRef::Map(key, value) => format!("Map<{}, {}>", type_ref_label(key), type_ref_label(value)),
        _ => format!("{ty:?}"),
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::core::config::service::ServiceConfig;
    use crate::core::ir::{MethodDef, ServiceDef, TypeDef};

    fn method_def(name: &str, return_type: TypeRef) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            params: Vec::new(),
            return_type,
            is_async: false,
            is_static: true,
            error_type: None,
            doc: String::new(),
            receiver: None,
            sanitized: false,
            trait_source: None,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
            has_default_impl: false,
            binding_excluded: false,
            binding_exclusion_reason: None,
            version: Default::default(),
        }
    }

    #[test]
    fn ffi_dependent_generation_rejects_vec_named_return_without_serde_metadata() {
        let api = ApiSurface {
            crate_name: "sample-lib".to_string(),
            types: vec![TypeDef {
                name: "Payload".to_string(),
                rust_path: "sample_lib::Payload".to_string(),
                has_serde: false,
                ..TypeDef::default()
            }],
            functions: vec![crate::core::ir::FunctionDef {
                name: "list_payloads".to_string(),
                rust_path: "sample_lib::list_payloads".to_string(),
                original_rust_path: String::new(),
                params: Vec::new(),
                return_type: TypeRef::Vec(Box::new(TypeRef::Named("Payload".to_string()))),
                is_async: false,
                error_type: None,
                doc: String::new(),
                cfg: None,
                sanitized: false,
                return_sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
                binding_excluded: false,
                binding_exclusion_reason: None,
                version: Default::default(),
            }],
            ..ApiSurface::default()
        };
        let config = ResolvedCrateConfig {
            name: "sample-lib".to_string(),
            ..ResolvedCrateConfig::default()
        };

        let error = validate_generation_api(&api, &config, &[Language::Ffi]).expect_err("missing serde must fail");

        assert!(
            error.to_string().contains("backend_stub_path") && error.to_string().contains("function list_payloads"),
            "expected FFI backend-readiness error, got {error}"
        );
    }

    #[test]
    fn service_api_generation_rejects_selected_backend_without_capability() {
        let api = ApiSurface {
            crate_name: "sample-lib".to_string(),
            types: vec![TypeDef {
                name: "App".to_string(),
                rust_path: "sample_lib::App".to_string(),
                ..TypeDef::default()
            }],
            services: vec![ServiceDef {
                name: "App".to_string(),
                rust_path: "sample_lib::App".to_string(),
                constructor: method_def("new", TypeRef::Named("App".to_string())),
                configurators: Vec::new(),
                registrations: Vec::new(),
                entrypoints: Vec::new(),
                doc: String::new(),
                cfg: None,
            }],
            ..ApiSurface::default()
        };
        let config = ResolvedCrateConfig {
            name: "sample-lib".to_string(),
            services: vec![ServiceConfig {
                owner_type: "App".to_string(),
                constructor: Some("new".to_string()),
                configurators: Vec::new(),
                registrations: Vec::new(),
                entrypoints: Vec::new(),
                skip_languages: Vec::new(),
                host_app_inner_accessor: None,
            }],
            ..ResolvedCrateConfig::default()
        };

        let error = validate_generation_api(&api, &config, &[Language::KotlinAndroid])
            .expect_err("unsupported service backend must fail");

        assert!(
            error.to_string().contains("unsupported_backend_capability")
                && error.to_string().contains("kotlin_android"),
            "expected unsupported backend capability error, got {error}"
        );
    }
}
