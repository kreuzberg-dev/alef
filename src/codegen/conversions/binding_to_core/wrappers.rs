use crate::core::ir::CoreWrapper;

/// Apply CoreWrapper transformations to a binding→core conversion expression.
/// Wraps the value expression with Arc::new(), .into() for Cow, etc.
pub fn apply_core_wrapper_to_core(
    conversion: &str,
    name: &str,
    core_wrapper: &CoreWrapper,
    vec_inner_core_wrapper: &CoreWrapper,
    optional: bool,
) -> String {
    // Handle Vec<Arc<T>>: replace .map(Into::into) with .map(|v| std::sync::Arc::new(v.into()))
    if *vec_inner_core_wrapper == CoreWrapper::Arc {
        return conversion
            .replace(
                ".map(Into::into).collect()",
                ".map(|v| std::sync::Arc::new(v.into())).collect()",
            )
            .replace(
                "map(|v| v.into_iter().map(Into::into)",
                "map(|v| v.into_iter().map(|v| std::sync::Arc::new(v.into()))",
            );
    }

    match core_wrapper {
        CoreWrapper::None => conversion.to_string(),
        CoreWrapper::Cow | CoreWrapper::Box => {
            // Cow<str> / Box<str>: binding String → core wrapper via .into().
            // Both wrappers have the same conversion shape — binding is `String`
            // and core is `Cow<'_, str>` or `Box<str>`, so `String -> wrapper`
            // goes through the same `.into()` path. The field_conversion already
            // emits "name: val.name" for strings; we add .into() to wrap.
            if let Some(expr) = conversion.strip_prefix(&format!("{name}: ")) {
                if optional {
                    format!("{name}: {expr}.map(Into::into)")
                } else if expr == format!("val.{name}") {
                    format!("{name}: val.{name}.into()")
                } else if expr == "Default::default()" {
                    // Sanitized field: Default::default() already resolves to the correct core type
                    // (e.g. Cow<'static, str> — adding .into() breaks type inference).
                    conversion.to_string()
                } else {
                    format!("{name}: ({expr}).into()")
                }
            } else {
                conversion.to_string()
            }
        }
        CoreWrapper::Arc => {
            // Arc<T>: wrap with Arc::new()
            if let Some(expr) = conversion.strip_prefix(&format!("{name}: ")) {
                if expr == "Default::default()" {
                    // Sanitized field: Default::default() resolves to the correct core type;
                    // wrapping in Arc::new() would change the type.
                    conversion.to_string()
                } else if optional {
                    format!("{name}: {expr}.map(|v| std::sync::Arc::new(v))")
                } else {
                    format!("{name}: std::sync::Arc::new({expr})")
                }
            } else {
                conversion.to_string()
            }
        }
        CoreWrapper::Bytes => {
            // Bytes: binding Vec<u8> → core bytes::Bytes via .into().
            // When TypeRef::Bytes already emitted a conversion (e.g. `val.{name}.into()` or
            // `val.{name}.map(Into::into)`), applying another .into() creates an ambiguous
            // double-into chain. Detect and dedup: use the already-generated expression as-is
            // when it fully covers the conversion, or emit a fresh single .into() for bare fields.
            if let Some(expr) = conversion.strip_prefix(&format!("{name}: ")) {
                let already_converted_non_opt =
                    expr == format!("val.{name}.into()") || expr == format!("val.{name}.to_vec().into()");
                let already_converted_opt = expr
                    .strip_prefix(&format!("val.{name}"))
                    .map(|s| s == ".map(Into::into)" || s == ".map(|v| v.to_vec().into())")
                    .unwrap_or(false);
                if already_converted_non_opt || already_converted_opt {
                    // The base conversion already handles Bytes — pass through unchanged.
                    conversion.to_string()
                } else if optional {
                    format!("{name}: {expr}.map(Into::into)")
                } else if expr == format!("val.{name}") {
                    format!("{name}: val.{name}.into()")
                } else if expr == "Default::default()" {
                    // Sanitized field: Default::default() already resolves to the correct core type
                    // (e.g. bytes::Bytes — adding .into() breaks type inference).
                    conversion.to_string()
                } else {
                    format!("{name}: ({expr}).into()")
                }
            } else {
                conversion.to_string()
            }
        }
        CoreWrapper::ArcMutex => {
            // ArcMutex: binding T → core Arc<Mutex<T>> via Arc::new(Mutex::new())
            if let Some(expr) = conversion.strip_prefix(&format!("{name}: ")) {
                if optional {
                    format!("{name}: {expr}.map(|v| std::sync::Arc::new(std::sync::Mutex::new(v.into())))")
                } else if expr == format!("val.{name}") {
                    format!("{name}: std::sync::Arc::new(std::sync::Mutex::new(val.{name}.into()))")
                } else {
                    format!("{name}: std::sync::Arc::new(std::sync::Mutex::new(({expr}).into()))")
                }
            } else {
                conversion.to_string()
            }
        }
    }
}
