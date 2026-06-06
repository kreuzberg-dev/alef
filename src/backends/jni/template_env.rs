use minijinja::Environment;

static TEMPLATES: &[(&str, &str)] = &[
    ("lib_header.rs.jinja", include_str!("templates/lib_header.rs.jinja")),
    (
        "runtime_helpers.rs.jinja",
        include_str!("templates/runtime_helpers.rs.jinja"),
    ),
    (
        "trait_register_shim.rs.jinja",
        include_str!("templates/trait_register_shim.rs.jinja"),
    ),
    (
        "trait_unregister_shim.rs.jinja",
        include_str!("templates/trait_unregister_shim.rs.jinja"),
    ),
    (
        "trait_clear_shim.rs.jinja",
        include_str!("templates/trait_clear_shim.rs.jinja"),
    ),
    (
        "function_shim_open.rs.jinja",
        include_str!("templates/function_shim_open.rs.jinja"),
    ),
    (
        "constructor_shim.rs.jinja",
        include_str!("templates/constructor_shim.rs.jinja"),
    ),
    (
        "destructor_shim.rs.jinja",
        include_str!("templates/destructor_shim.rs.jinja"),
    ),
    (
        "service_header.rs.jinja",
        include_str!("templates/service_header.rs.jinja"),
    ),
    (
        "service_opaque.rs.jinja",
        include_str!("templates/service_opaque.rs.jinja"),
    ),
    (
        "handler_bridge_struct.rs.jinja",
        include_str!("templates/handler_bridge_struct.rs.jinja"),
    ),
    (
        "handler_bridge_impl.rs.jinja",
        include_str!("templates/handler_bridge_impl.rs.jinja"),
    ),
    (
        "registration_variant.rs.jinja",
        include_str!("templates/registration_variant.rs.jinja"),
    ),
];

pub(crate) fn render(name: &str, context: minijinja::Value) -> String {
    let env = make_env();
    env.get_template(name)
        .unwrap_or_else(|err| panic!("missing JNI template {name}: {err}"))
        .render(context)
        .unwrap_or_else(|err| panic!("failed to render JNI template {name}: {err}"))
}

fn make_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    for (name, source) in TEMPLATES {
        env.add_template(name, source)
            .unwrap_or_else(|err| panic!("failed to register JNI template {name}: {err}"));
    }
    env
}
