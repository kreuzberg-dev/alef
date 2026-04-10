use crate::generators::RustBindingConfig;
use alef_core::ir::EnumDef;
use std::fmt::Write;

/// Generate an enum.
pub fn gen_enum(enum_def: &EnumDef, cfg: &RustBindingConfig) -> String {
    // All enums are generated as unit-variant-only in the binding layer.
    // Data variants are flattened to unit variants; the From/Into conversions
    // handle the lossy mapping (discarding / providing defaults for field data).
    let mut out = String::with_capacity(512);
    let mut derives: Vec<&str> = cfg.enum_derives.to_vec();
    if cfg.has_serde {
        derives.push("serde::Serialize");
    }
    if !derives.is_empty() {
        writeln!(out, "#[derive({})]", derives.join(", ")).ok();
    }
    for attr in cfg.enum_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    writeln!(out, "pub enum {} {{", enum_def.name).ok();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "    {} = {idx},", variant.name).ok();
    }
    writeln!(out, "}}").ok();

    // Generate Default impl (first variant) so enums can be used with unwrap_or_default()
    // in config constructors for types with has_default.
    if let Some(first) = enum_def.variants.first() {
        writeln!(out).ok();
        writeln!(out, "#[allow(clippy::derivable_impls)]").ok();
        writeln!(out, "impl Default for {} {{", enum_def.name).ok();
        writeln!(out, "    fn default() -> Self {{ Self::{} }}", first.name).ok();
        writeln!(out, "}}").ok();
    }

    out
}
