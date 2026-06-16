use crate::codegen::builder::RustFileBuilder;
use crate::core::config::{CapsuleTypeConfig, ResolvedCrateConfig};
use ahash::AHashSet;
use std::collections::HashMap;

pub(crate) fn exclude_capsule_opaque_types(
    py_exclude_types: &mut AHashSet<String>,
    config: &ResolvedCrateConfig,
    capsule_types: &HashMap<String, CapsuleTypeConfig>,
) {
    for name in config.opaque_types.keys() {
        if capsule_types.contains_key(name) {
            py_exclude_types.insert(name.clone());
        }
    }
}

pub(crate) fn emit_wrappers(
    builder: &mut RustFileBuilder,
    config: &ResolvedCrateConfig,
    capsule_types: &HashMap<String, CapsuleTypeConfig>,
    emitted_pyclass_names: &AHashSet<&str>,
    error_type_names: &AHashSet<String>,
    api_opaque_types_empty: bool,
) {
    if config.opaque_types.is_empty() {
        return;
    }

    if api_opaque_types_empty {
        builder.add_import("std::sync::Arc");
    }

    let mut emitted_opaque_wrapper_names: AHashSet<&str> = AHashSet::new();
    for (name, source_path) in &config.opaque_types {
        if capsule_types.contains_key(name)
            || emitted_pyclass_names.contains(name.as_str())
            || error_type_names.contains(name.as_str())
            || !emitted_opaque_wrapper_names.insert(name.as_str())
        {
            continue;
        }

        let rust_path = source_path.replace('-', "_");
        let wrapper = crate::backends::pyo3::template_env::render(
            "config_opaque_wrapper.rs.jinja",
            minijinja::context! {
                name => name,
                rust_path => rust_path,
            },
        );
        builder.add_item(wrapper.trim_end());
    }
}
