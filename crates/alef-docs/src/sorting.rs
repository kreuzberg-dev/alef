pub(crate) fn type_sort_key(name: &str) -> (u8, &str) {
    match name {
        "ConversionOptions" => (0, name),
        "ConversionResult" => (1, name),
        _ => (2, name),
    }
}

pub(crate) fn is_update_type(name: &str) -> bool {
    name.ends_with("Update")
}
