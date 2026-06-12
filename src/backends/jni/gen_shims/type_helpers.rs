fn internal_class_component(name: &str) -> String {
    to_class_name(name)
}

/// Return the ` -> <JniReturnType>` suffix for a method shim signature.
fn method_return_type_decl(return_type: &TypeRef) -> String {
    match return_type {
        TypeRef::Unit => String::new(),
        TypeRef::Primitive(PrimitiveType::Bool) => " -> jboolean".to_string(),
        TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8)) => {
            " -> jbyteArray".to_string()
        }
        TypeRef::Bytes => " -> jbyteArray".to_string(),
        TypeRef::Optional(inner)
            if matches!(inner.as_ref(), TypeRef::Bytes)
                || matches!(inner.as_ref(), TypeRef::Vec(vec_inner) if matches!(vec_inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8))) =>
        {
            " -> jbyteArray".to_string()
        }
        TypeRef::Primitive(_) => {
            let jni_ty = jni_return_type(return_type);
            format!(" -> {jni_ty}")
        }
        _ => " -> jstring".to_string(),
    }
}

/// Return the "null" / zero value for a method return type (used in error paths).
fn method_return_null(return_type: &TypeRef) -> &'static str {
    match return_type {
        TypeRef::Unit => "()",
        // jni 0.22 + jni-sys 0.4 changed `jboolean` from `u8` to `bool`; the
        // sentinel value for an error-path return therefore needs to be `false`,
        // not the legacy `0u8`.
        TypeRef::Primitive(PrimitiveType::Bool) => "false",
        TypeRef::Primitive(PrimitiveType::F32) => "0.0f32",
        TypeRef::Primitive(PrimitiveType::F64) => "0.0f64",
        TypeRef::Primitive(_) => "0",
        TypeRef::Bytes => "std::ptr::null_mut()",
        TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8)) => {
            "std::ptr::null_mut()"
        }
        _ => "std::ptr::null_mut()",
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a TypeRef to a JNI return type string.
fn jni_return_type(ty: &TypeRef) -> &'static str {
    match ty {
        TypeRef::Unit => "()",
        TypeRef::Primitive(p) => jni_primitive_type(p),
        TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8)) => "jbyteArray",
        TypeRef::Bytes => "jbyteArray",
        TypeRef::Optional(inner)
            if matches!(inner.as_ref(), TypeRef::Bytes)
                || matches!(inner.as_ref(), TypeRef::Vec(vec_inner) if matches!(vec_inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8))) =>
        {
            "jbyteArray"
        }
        // String and complex types cross the boundary as Java objects.
        TypeRef::String | TypeRef::Named(_) | TypeRef::Optional(_) | TypeRef::Vec(_) | TypeRef::Map(_, _) => "jstring",
        // Opaque handles → Long.
        _ => "jlong",
    }
}

fn jni_primitive_type(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Bool => "jboolean",
        PrimitiveType::I8 | PrimitiveType::U8 => "jni::sys::jbyte",
        PrimitiveType::I16 | PrimitiveType::U16 => "jni::sys::jshort",
        PrimitiveType::I32 | PrimitiveType::U32 => "jni::sys::jint",
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize => "jlong",
        PrimitiveType::F32 => "jni::sys::jfloat",
        PrimitiveType::F64 => "jni::sys::jdouble",
    }
}

/// Return the Rust zero-literal for a JNI primitive, used as the null-sentinel
/// for optional primitive parameters.  Returns None for `Bool`, which has no
/// meaningful "absent" sentinel (false is a real value); optional bools cannot
/// be marshalled through plain JNI primitives.
fn primitive_zero_literal(p: &PrimitiveType) -> Option<&'static str> {
    match p {
        PrimitiveType::Bool => None,
        PrimitiveType::I8
        | PrimitiveType::U8
        | PrimitiveType::I16
        | PrimitiveType::U16
        | PrimitiveType::I32
        | PrimitiveType::U32
        | PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::Usize
        | PrimitiveType::Isize => Some("0"),
        PrimitiveType::F32 | PrimitiveType::F64 => Some("0.0"),
    }
}

/// Return a Rust cast target for a JNI primitive → Rust type conversion, or "" if no cast needed.
fn primitive_cast(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Bool => "bool",
        PrimitiveType::I8 => "i8",
        PrimitiveType::U8 => "u8",
        PrimitiveType::I16 => "i16",
        PrimitiveType::U16 => "u16",
        PrimitiveType::I32 => "i32",
        PrimitiveType::U32 => "u32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::U64 => "u64",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
    }
}

/// Map a TypeRef to a Rust type path for serde deserialization.
fn type_ref_to_core_path(ty: &TypeRef, core_prefix: &str) -> String {
    match ty {
        TypeRef::String => "String".to_string(),
        TypeRef::Primitive(p) => primitive_rust_type(p).to_string(),
        TypeRef::Named(n) => format!("{core_prefix}::{n}"),
        TypeRef::Optional(inner) => format!("Option<{}>", type_ref_to_core_path(inner, core_prefix)),
        TypeRef::Vec(inner) => format!("Vec<{}>", type_ref_to_core_path(inner, core_prefix)),
        TypeRef::Map(k, v) => format!(
            "std::collections::HashMap<{}, {}>",
            type_ref_to_core_path(k, core_prefix),
            type_ref_to_core_path(v, core_prefix)
        ),
        _ => "serde_json::Value".to_string(),
    }
}

fn primitive_rust_type(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::Bool => "bool",
        PrimitiveType::I8 => "i8",
        PrimitiveType::U8 => "u8",
        PrimitiveType::I16 => "i16",
        PrimitiveType::U16 => "u16",
        PrimitiveType::I32 => "i32",
        PrimitiveType::U32 => "u32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::U64 => "u64",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
    }
}

/// Resolve the Kotlin package string used when constructing JNI symbols.
///
/// Prefers `[crates.kotlin_android] package`, then `[crates.kotlin] package`,
/// then falls back to `config.kotlin_package()`.
fn jni_kotlin_package(config: &ResolvedCrateConfig) -> String {
    config
        .kotlin_android
        .as_ref()
        .and_then(|a| a.package.clone())
        .or_else(|| config.kotlin.as_ref().and_then(|k| k.package.clone()))
        .unwrap_or_else(|| config.kotlin_package())
}

/// Resolve the fully-qualified error class name for `ERROR_CLASS`.
///
/// Uses `<package_slashed>/<BridgeName>Exception` as default.
fn resolve_error_class(config: &ResolvedCrateConfig, package: &str) -> String {
    let package_slashed = package.replace('.', "/");
    let bridge = bridge_class_name(&config.name);
    format!("{package_slashed}/{bridge}Exception")
}

/// Return the `use` path for the core crate from the JNI shim.
///
/// Uses the `name` field of the config (which is the crate name, e.g.
/// `sample-llm`), converting hyphens to underscores per Rust convention.
fn core_use_path(config: &ResolvedCrateConfig) -> String {
    config.name.replace('-', "_")
}
