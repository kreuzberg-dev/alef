/// Scan a list of parsed `syn::Item`s and return the set of type names that have **manual**
/// (non-derive) `impl serde::Serialize` AND `impl serde::Deserialize` blocks.
///
/// `#[derive(Serialize, Deserialize)]` is handled by the struct/enum extractor via
/// `has_derive`. This function covers the complementary case where both serde traits are
/// implemented manually — typically needed when a type's Serialize and Deserialize
/// implementations are asymmetric (e.g. `NodeContext<'_>` serialises from borrowed data but
/// deserialises into `NodeContext<'static>`).
///
/// The detection is intentionally permissive about generic parameters: both
/// `impl serde::Serialize for Foo` and `impl serde::Serialize for Foo<'_>` match `Foo`.
/// The function is also cfg-agnostic — a manual impl inside a `#[cfg(feature = "serde")]`
/// block counts the same as an unconditional one.
pub(crate) fn collect_manual_serde_type_names(items: &[syn::Item]) -> ahash::AHashSet<String> {
    let mut has_serialize: ahash::AHashSet<String> = ahash::AHashSet::new();
    let mut has_deserialize: ahash::AHashSet<String> = ahash::AHashSet::new();

    for item in items {
        if let syn::Item::Impl(item_impl) = item {
            let Some((_, trait_path, _)) = &item_impl.trait_ else {
                continue;
            };
            // Extract the base type name from the self type, ignoring any lifetime/generic args.
            // Both `impl Trait for Foo` and `impl Trait for Foo<'_>` give type name "Foo".
            let type_name = match &*item_impl.self_ty {
                syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                _ => None,
            };
            let Some(type_name) = type_name else {
                continue;
            };

            // Determine which serde trait this impl block implements.
            // Acceptable forms:
            //   - `impl Serialize for T`              (single segment)
            //   - `impl serde::Serialize for T`       (two segments)
            //   - `impl<'de> Deserialize<'de> for T`  (single segment, generic on impl)
            //   - `impl<'de> serde::Deserialize<'de> for T` (two segments)
            let trait_last = trait_path.segments.last().map(|s| s.ident.to_string());
            match trait_last.as_deref() {
                Some("Serialize") => {
                    has_serialize.insert(type_name);
                }
                Some("Deserialize") => {
                    has_deserialize.insert(type_name);
                }
                _ => {}
            }
        }
    }

    // Return only names where BOTH impls were found.
    has_serialize
        .into_iter()
        .filter(|name| has_deserialize.contains(name))
        .collect()
}
