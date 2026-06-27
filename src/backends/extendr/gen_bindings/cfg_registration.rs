use crate::core::config::{Language, ResolvedCrateConfig};
use crate::core::ir::ApiSurface;

/// Prepend `#[cfg(<pred>)]` to a code item when the source symbol carries a cfg predicate.
pub(super) fn prepend_cfg(cfg: Option<&str>, item: String) -> String {
    match cfg {
        Some(pred) if !pred.is_empty() => format!("#[cfg({pred})]\n{item}"),
        _ => item,
    }
}

/// Normalise a cfg predicate for structural comparison by stripping all whitespace.
fn strip_cfg_whitespace(pred: &str) -> String {
    pred.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Return true if a function's effective cfg means it is always compiled in the binding crate, so
/// it is safe to register in the `extendr_module!` block.
pub(super) fn always_registered(cfg: Option<&str>) -> bool {
    let Some(pred) = cfg else {
        return true;
    };
    let pred = pred.trim();
    if pred.is_empty() {
        return true;
    }
    let normalized = strip_cfg_whitespace(pred);
    let inner = normalized.strip_prefix("any(").and_then(|s| s.strip_suffix(')'));
    if let Some(inner) = inner {
        if let Some((left, right)) = inner.split_once(',') {
            let complement = |a: &str, b: &str| format!("not({a})") == b;
            return complement(left, right) || complement(right, left);
        }
    }
    false
}

/// Return the cfg features that are effectively enabled for generated R Rust code.
///
/// The R scaffold declares every referenced cfg feature as a passthrough feature. Unless the
/// user explicitly chooses a curated no-default feature set, that scaffold also enables every
/// passthrough feature by default, so codegen must treat those fields as present.
pub(super) fn effective_r_cfg_features(api: &ApiSurface, config: &ResolvedCrateConfig) -> Vec<String> {
    let configured = config.features_for_language(Language::R);
    let default_features = config.r.as_ref().and_then(|r| r.default_features).unwrap_or(true);
    if !configured.is_empty() && !default_features {
        return configured.to_vec();
    }
    crate::codegen::cfg::collect_cfg_features(api).into_iter().collect()
}

/// Apply the R backend's field cfg policy before struct, conversion, and wrapper generation.
///
/// Fields whose cfg is enabled by the R feature set are made unconditional in the generated
/// binding surface. Disabled fields are removed from the binding DTO and marked as stripped so
/// conversion templates keep using core defaults for the missing core slots.
pub(super) fn apply_r_cfg_field_policy(api: &ApiSurface, enabled_features: &[String]) -> ApiSurface {
    let mut filtered = api.clone();
    for typ in &mut filtered.types {
        let mut stripped = false;
        let mut fields = Vec::with_capacity(typ.fields.len());
        for mut field in typ.fields.drain(..) {
            let Some(cfg) = field.cfg.as_deref() else {
                fields.push(field);
                continue;
            };
            if cfg_condition_enabled(cfg, enabled_features) {
                field.cfg = None;
                fields.push(field);
            } else {
                stripped = true;
            }
        }
        if stripped {
            typ.has_stripped_cfg_fields = true;
        }
        typ.fields = fields;
    }
    filtered
}

fn cfg_condition_enabled(cfg_str: &str, enabled_features: &[String]) -> bool {
    let normalized = cfg_str.trim().replace(" (", "(");
    let cfg_str = normalized.as_str();
    let feature_enabled = |feature: &str| {
        enabled_features
            .iter()
            .any(|enabled| enabled == feature || enabled == "full")
    };

    if let Some(feature) = cfg_str.strip_prefix("feature = \"").and_then(|s| s.strip_suffix('"')) {
        return feature_enabled(feature);
    }
    if let Some(inner) = cfg_str.strip_prefix("any(").and_then(|s| s.strip_suffix(')')) {
        return parse_cfg_list(inner)
            .iter()
            .any(|cond| cfg_condition_enabled(cond, enabled_features));
    }
    if let Some(inner) = cfg_str.strip_prefix("all(").and_then(|s| s.strip_suffix(')')) {
        return parse_cfg_list(inner)
            .iter()
            .all(|cond| cfg_condition_enabled(cond, enabled_features));
    }
    if let Some(inner) = cfg_str.strip_prefix("not(").and_then(|s| s.strip_suffix(')')) {
        return !cfg_condition_enabled(inner.trim(), enabled_features);
    }
    true
}

fn parse_cfg_list(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }
    result
}
