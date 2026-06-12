use crate::core::ir::{ApiSurface, TypeRef};

/// Returns true if `ty` references a `Named(name)` at any depth where `name` resolves
/// to a trait — either present in `api.types` or stripped from the binding surface
/// (`api.excluded_trait_names`). Such methods return references to trait objects
/// (`&dyn Trait`, `Option<&dyn Trait>`, `Box<dyn Trait>`) which the Rust IR flattens
/// to `Named(name)`. They cannot be bridged to Dart — the foreign side has no way to
/// construct or return a Rust trait object across FFI — so the trait-bridge generator
/// skips them and falls back to the trait's default impl.
///
/// The `excluded_trait_names` lookup is necessary because traits annotated with
/// `#[cfg_attr(alef, alef(skip))]` (e.g. `SyncExtractor`) are stripped from `api.types`
/// before codegen, but their NAME may still appear in surviving trait method return
/// signatures (e.g. `DocumentExtractor::as_sync_extractor() -> Option<&dyn SyncExtractor>`).
/// Without this fallback, the bridge struct would emit a closure field with the trait
/// path used as a TYPE (`Option<sample_core::extractors::SyncExtractor>`), producing
/// `error[E0782]: expected a type, found a trait`. Restricting the check to trait-shaped
/// excluded items (not all excluded items) keeps methods returning excluded structs
/// (`load -> Result<HiddenDocument>`) emitted, since the excluded item is a
/// concrete struct usable by its qualified core path.
pub(crate) fn return_type_references_trait(ty: &TypeRef, api: &ApiSurface) -> bool {
    match ty {
        TypeRef::Named(name) => {
            api.types.iter().any(|t| t.is_trait && &t.name == name) || api.excluded_trait_names.contains(name)
        }
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => return_type_references_trait(inner, api),
        TypeRef::Map(k, v) => return_type_references_trait(k, api) || return_type_references_trait(v, api),
        _ => false,
    }
}
