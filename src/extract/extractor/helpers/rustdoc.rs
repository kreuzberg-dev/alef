use syn;

/// Extract doc comments from attributes.
///
/// Output is post-processed by [`normalize_rustdoc`] so binding emitters
/// never see rustdoc-hidden setup lines (`# tokio_test::block_on(async {`)
/// or unresolved intra-doc-link syntax (`[\`crate::Foo\`]`).
pub(crate) fn extract_doc_comments(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        let val = lit_str.value();
                        // Doc comments typically have a leading space
                        let trimmed = val.strip_prefix(' ').unwrap_or(&val);
                        lines.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    let raw = lines.join("\n");
    normalize_rustdoc(&raw)
}

/// Pre-process raw rustdoc so binding emitters can treat it as plain prose.
///
/// 1. Inside ```rust / ```rust,no_run fences, drops lines starting with `# `
///    (rustdoc's "hidden" syntax used to inject test scaffolding such as
///    `# tokio_test::block_on(async {` or `# Ok::<(), Error>(())`).
/// 2. Converts intra-doc-link syntax `` [`crate::Foo`] `` and
///    `` [`super::Bar`] `` to plain `` `Foo` `` / `` `Bar` `` so unresolved
///    paths don't leak into JS / Java / dart output.
///
/// Any other content is preserved verbatim (existing per-host renderers
/// continue to translate `# Errors` / `# Returns` / etc).
pub fn normalize_rustdoc(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    // Step 1: walk lines, track ```rust fence depth, drop `# ` lines inside.
    let mut filtered = String::with_capacity(raw.len());
    let mut in_rust_fence = false;
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            // Toggle fence state. We only treat fences with NO language tag
            // OR an explicit `rust` / `rust,...` tag as Rust-doc fences.
            if in_rust_fence {
                in_rust_fence = false;
            } else {
                let lang = rest.split(',').next().unwrap_or("").trim();
                if lang.is_empty() || lang.eq_ignore_ascii_case("rust") {
                    in_rust_fence = true;
                }
            }
            filtered.push_str(line);
            filtered.push('\n');
            continue;
        }
        if in_rust_fence {
            // Rustdoc-hidden lines start with `# ` (a hash followed by a space)
            // or are exactly `#` (rare). Drop them entirely.
            let after_hash = trimmed.strip_prefix('#');
            if let Some(suffix) = after_hash {
                if suffix.is_empty() || suffix.starts_with(' ') {
                    continue;
                }
            }
        }
        filtered.push_str(line);
        filtered.push('\n');
    }

    // Step 2: rewrite `[` `crate::Foo` `]` / `[` `super::Bar` `]` into
    // `` `Foo` `` / `` `Bar` ``. We only touch the `[\`...\`]` form so
    // legitimate Markdown links are preserved.
    let mut out = String::with_capacity(filtered.len());
    let chars: Vec<char> = filtered.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Detect `[` `\`` opening of an intra-doc link.
        if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '`' {
            // Find the matching `\`]`. The contents are a Rust path.
            let start = i + 2;
            let mut j = start;
            while j + 1 < chars.len() {
                if chars[j] == '`' && chars[j + 1] == ']' {
                    break;
                }
                j += 1;
            }
            if j + 1 < chars.len() && chars[j] == '`' && chars[j + 1] == ']' {
                let inner: String = chars[start..j].iter().collect();
                // Only rewrite paths that begin with `crate::` or `super::`
                // (or `self::`). Leave other intra-doc-link forms to the
                // existing per-host renderer.
                let stripped = inner
                    .strip_prefix("crate::")
                    .or_else(|| inner.strip_prefix("super::"))
                    .or_else(|| inner.strip_prefix("self::"));
                if let Some(rest) = stripped {
                    // Take the last path segment as the human-friendly name.
                    let last = rest.rsplit("::").next().unwrap_or(rest);
                    out.push('`');
                    out.push_str(last);
                    out.push('`');
                    i = j + 2;
                    // Skip optional trailing `(...)` link target.
                    if i < chars.len() && chars[i] == '(' {
                        let mut depth = 1;
                        i += 1;
                        while i < chars.len() && depth > 0 {
                            match chars[i] {
                                '(' => depth += 1,
                                ')' => depth -= 1,
                                _ => {}
                            }
                            i += 1;
                        }
                    }
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // Drop the trailing newline introduced by the line-walk step so the
    // output round-trips with raw input that has no trailing newline.
    if out.ends_with('\n') {
        out.pop();
    }
    out
}
