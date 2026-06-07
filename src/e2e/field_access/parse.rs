use super::types::PathSegment;

pub(super) fn strip_numeric_indices(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut key = String::new();
            let mut closed = false;
            for inner in chars.by_ref() {
                if inner == ']' {
                    closed = true;
                    break;
                }
                key.push(inner);
            }
            if closed && !key.is_empty() && key.chars().all(|k| k.is_ascii_digit()) {
                // Numeric index — drop it entirely (including any trailing dot).
            } else {
                result.push('[');
                result.push_str(&key);
                if closed {
                    result.push(']');
                }
            }
        } else {
            result.push(c);
        }
    }
    // Collapse any double-dots introduced by dropping `[N].` sequences.
    while result.contains("..") {
        result = result.replace("..", ".");
    }
    if result.starts_with('.') {
        result.remove(0);
    }
    result
}

pub(super) fn normalize_numeric_indices(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut key = String::new();
            let mut closed = false;
            for inner in chars.by_ref() {
                if inner == ']' {
                    closed = true;
                    break;
                }
                key.push(inner);
            }
            if closed && !key.is_empty() && key.chars().all(|k| k.is_ascii_digit()) {
                result.push_str("[0]");
            } else {
                result.push('[');
                result.push_str(&key);
                if closed {
                    result.push(']');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub(super) fn parse_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    for part in path.split('.') {
        if part == "length" || part == "count" || part == "size" {
            segments.push(PathSegment::Length);
        } else if let Some(bracket_pos) = part.find('[') {
            let name = part[..bracket_pos].to_string();
            let key = part[bracket_pos + 1..].trim_end_matches(']').to_string();
            if key.is_empty() {
                // `foo[]` — bare array bracket, index defaults to 0 (upgraded by inject_array_indexing).
                segments.push(PathSegment::ArrayField { name, index: 0 });
            } else if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
                // `foo[N]` — user-typed explicit numeric index.
                let index: usize = key.parse().unwrap_or(0);
                segments.push(PathSegment::ArrayField { name, index });
            } else {
                // `foo[key]` — string-keyed map access.
                segments.push(PathSegment::MapAccess { field: name, key });
            }
        } else {
            segments.push(PathSegment::Field(part.to_string()));
        }
    }
    segments
}
