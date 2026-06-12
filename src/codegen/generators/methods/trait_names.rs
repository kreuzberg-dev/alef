/// Returns true when `name` matches a known trait method that would trigger
/// `clippy::should_implement_trait`.
pub fn is_trait_method_name(name: &str) -> bool {
    crate::codegen::generators::TRAIT_METHOD_NAMES.contains(&name)
}
