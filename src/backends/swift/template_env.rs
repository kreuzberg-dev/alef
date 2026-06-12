use minijinja::Environment;

mod core_templates;
mod enum_error_templates;
mod rust_crate_templates;
mod streaming_templates;
mod swift_templates;
mod wrapper_templates;

static TEMPLATE_GROUPS: &[&[(&str, &str)]] = &[
    core_templates::TEMPLATES,
    enum_error_templates::TEMPLATES,
    wrapper_templates::TEMPLATES,
    rust_crate_templates::TEMPLATES,
    swift_templates::TEMPLATES,
    streaming_templates::TEMPLATES,
];

pub(crate) fn make_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    for templates in TEMPLATE_GROUPS {
        for (name, src) in *templates {
            env.add_template(name, src).expect("built-in template is valid");
        }
    }
    env
}

pub(crate) fn render(template_name: &str, ctx: minijinja::Value) -> String {
    make_env()
        .get_template(template_name)
        .unwrap_or_else(|_| panic!("template {template_name} not found"))
        .render(ctx)
        .unwrap_or_else(|e| panic!("template {template_name} failed to render: {e}"))
}
