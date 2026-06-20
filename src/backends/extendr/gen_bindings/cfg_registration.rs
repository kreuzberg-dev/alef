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
