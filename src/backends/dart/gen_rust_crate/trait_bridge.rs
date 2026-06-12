mod analysis;
mod callbacks;
mod emit;
mod excluded;
mod forwarders;
mod methods;

#[cfg(test)]
mod tests;

pub(crate) use analysis::return_type_references_trait;
pub(crate) use emit::emit_trait_bridge;
pub(crate) use excluded::{emit_excluded_bridge_types, needs_excluded_bridge_type};
