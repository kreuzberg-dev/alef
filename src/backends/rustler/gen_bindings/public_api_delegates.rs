use crate::backends::rustler::template_env;
use crate::core::config::ResolvedCrateConfig;
use ahash::AHashSet;
use heck::ToSnakeCase;

pub(in crate::backends::rustler::gen_bindings) fn append_trait_bridge_delegates(
    content: &mut String,
    config: &ResolvedCrateConfig,
    api_fn_names: &AHashSet<String>,
    native_mod: &str,
) {
    for bridge_cfg in &config.trait_bridges {
        if bridge_cfg
            .exclude_languages
            .iter()
            .any(|language| language == "elixir" || language == "rustler")
        {
            continue;
        }

        if let Some(register_fn) = bridge_cfg.register_fn.as_deref() {
            let func_name = register_fn.to_snake_case();
            content.push_str(&template_env::render(
                "elixir_trait_register_delegate.ex.jinja",
                minijinja::context! {
                    trait_name => &bridge_cfg.trait_name,
                    func_name => &func_name,
                    native_mod => native_mod,
                },
            ));
        }

        if let Some(unregister_fn) = bridge_cfg.unregister_fn.as_deref() {
            let func_name = unregister_fn.to_snake_case();
            content.push_str(&template_env::render(
                "elixir_trait_unregister_delegate.ex.jinja",
                minijinja::context! {
                    trait_name => &bridge_cfg.trait_name,
                    func_name => &func_name,
                    native_mod => native_mod,
                },
            ));
        }

        if let Some(clear_fn) = bridge_cfg.clear_fn.as_deref() {
            let func_name = clear_fn.to_snake_case();
            if !api_fn_names.contains(func_name.as_str()) {
                content.push_str(&template_env::render(
                    "elixir_trait_clear_delegate.ex.jinja",
                    minijinja::context! {
                        trait_name => &bridge_cfg.trait_name,
                        func_name => &func_name,
                        native_mod => native_mod,
                    },
                ));
            }
        }
    }
}
