use minijinja::Environment;

/// Create a minijinja environment with common filters and globals.
pub fn create_env() -> Environment<'static> {
    let mut env = Environment::new();

    // Add common filters
    env.add_filter("snake_case", |s: String| heck::ToSnakeCase::to_snake_case(s.as_str()));
    env.add_filter("camel_case", |s: String| {
        heck::ToLowerCamelCase::to_lower_camel_case(s.as_str())
    });
    env.add_filter("pascal_case", |s: String| {
        heck::ToPascalCase::to_pascal_case(s.as_str())
    });
    env.add_filter("shouty_snake_case", |s: String| {
        heck::ToShoutySnakeCase::to_shouty_snake_case(s.as_str())
    });

    env
}
