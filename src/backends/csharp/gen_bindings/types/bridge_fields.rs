use crate::core::config::TraitBridgeConfig;
use crate::core::ir::TypeRef;

pub(super) fn bridge_config_for_field<'a>(
    field_type: &TypeRef,
    trait_bridges: &'a [TraitBridgeConfig],
) -> Option<&'a TraitBridgeConfig> {
    trait_bridges.iter().find(|bridge| {
        bridge
            .type_alias
            .as_deref()
            .is_some_and(|alias| field_type_matches_alias(field_type, alias))
    })
}

pub(super) fn field_type_matches_alias(field_type: &TypeRef, alias: &str) -> bool {
    match field_type {
        TypeRef::Named(name) => name == alias,
        TypeRef::Optional(inner) => field_type_matches_alias(inner, alias),
        _ => false,
    }
}
