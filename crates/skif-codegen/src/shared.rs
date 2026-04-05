use ahash::AHashSet;
use skif_core::ir::{FieldDef, MethodDef, ParamDef, TypeRef};

/// Returns true if this parameter is required but must be promoted to optional
/// because it follows an optional parameter in the list.
/// PyO3 requires that required params come before all optional params.
pub fn is_promoted_optional(params: &[ParamDef], idx: usize) -> bool {
    if params[idx].optional {
        return false; // naturally optional
    }
    // Check if any earlier param is optional
    params[..idx].iter().any(|p| p.optional)
}

/// Check if a free function can be auto-delegated to the core crate.
/// Opaque Named params are allowed (unwrapped via Arc). Non-opaque Named params are not
/// (require From impls that may not exist for types with sanitized fields).
pub fn can_auto_delegate_function(func: &skif_core::ir::FunctionDef, opaque_types: &AHashSet<String>) -> bool {
    !func.sanitized
        && func
            .params
            .iter()
            .all(|p| !p.sanitized && is_delegatable_param(&p.ty, opaque_types))
        && is_delegatable_return(&func.return_type)
}

/// Check if all params and return type are delegatable.
pub fn can_auto_delegate(method: &MethodDef, opaque_types: &AHashSet<String>) -> bool {
    !method.sanitized
        && method
            .params
            .iter()
            .all(|p| !p.sanitized && is_delegatable_param(&p.ty, opaque_types))
        && is_delegatable_return(&method.return_type)
}

/// A param type is delegatable if it's simple, or a Named type (opaque → Arc unwrap, non-opaque → .into()).
pub fn is_delegatable_param(ty: &TypeRef, _opaque_types: &AHashSet<String>) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Named(_) => true, // Opaque: &*param.inner; non-opaque: .into()
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_delegatable_param(inner, _opaque_types),
        TypeRef::Map(k, v) => is_delegatable_param(k, _opaque_types) && is_delegatable_param(v, _opaque_types),
        TypeRef::Json => false,
    }
}

/// Return types are more permissive — Named types work via .into() (core→binding From exists).
pub fn is_delegatable_return(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Named(_) => true, // core→binding From impl generated for all convertible types
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_delegatable_return(inner),
        TypeRef::Map(k, v) => is_delegatable_return(k) && is_delegatable_return(v),
        TypeRef::Json => false,
    }
}

/// A type is delegatable if it can cross the binding boundary without From impls.
/// Named types are NOT delegatable as function params (may lack From impls).
/// For opaque methods, Named types are handled separately via Arc wrap/unwrap.
pub fn is_delegatable_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Named(_) => false, // Requires From impl which may not exist
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_delegatable_type(inner),
        TypeRef::Map(k, v) => is_delegatable_type(k) && is_delegatable_type(v),
        TypeRef::Json => false,
    }
}

/// Check if a type is delegatable in the opaque method context.
/// Opaque methods can handle Named params via Arc unwrap and Named returns via Arc wrap.
pub fn is_opaque_delegatable_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Named(_) => true, // Opaque: Arc unwrap/wrap. Non-opaque: .into()
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_opaque_delegatable_type(inner),
        TypeRef::Map(k, v) => is_opaque_delegatable_type(k) && is_opaque_delegatable_type(v),
        TypeRef::Json => false,
    }
}

/// Check if a type is "simple" — can be passed without any conversion.
pub fn is_simple_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_simple_type(inner),
        TypeRef::Map(k, v) => is_simple_type(k) && is_simple_type(v),
        TypeRef::Named(_) | TypeRef::Json => false,
    }
}

/// Partition methods into (instance, static).
pub fn partition_methods(methods: &[MethodDef]) -> (Vec<&MethodDef>, Vec<&MethodDef>) {
    let instance: Vec<_> = methods.iter().filter(|m| m.receiver.is_some()).collect();
    let statics: Vec<_> = methods.iter().filter(|m| m.receiver.is_none()).collect();
    (instance, statics)
}

/// Build a constructor parameter list string.
/// Returns (param_list, signature_with_defaults, field_assignments).
/// If param_list exceeds 100 chars, uses multiline format with trailing commas.
pub fn constructor_parts(fields: &[FieldDef], type_mapper: &dyn Fn(&TypeRef) -> String) -> (String, String, String) {
    // Sort fields: required first, then optional.
    // Many FFI frameworks (PyO3, NAPI) require required params before optional ones.
    let mut sorted_fields: Vec<&FieldDef> = fields.iter().collect();
    sorted_fields.sort_by_key(|f| f.optional as u8);

    let params: Vec<String> = sorted_fields
        .iter()
        .map(|f| {
            let ty = if f.optional {
                format!("Option<{}>", type_mapper(&f.ty))
            } else {
                type_mapper(&f.ty)
            };
            format!("{}: {}", f.name, ty)
        })
        .collect();

    let defaults: Vec<String> = sorted_fields
        .iter()
        .map(|f| {
            if f.optional {
                format!("{}=None", f.name)
            } else {
                f.name.clone()
            }
        })
        .collect();

    // Assignments keep original field order (for struct literal)
    let assignments: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();

    // Format param_list with line wrapping if needed
    let single_line = params.join(", ");
    let param_list = if single_line.len() > 100 {
        format!("\n        {},\n    ", params.join(",\n        "))
    } else {
        single_line
    };

    (param_list, defaults.join(", "), assignments.join(", "))
}

/// Build a function parameter list.
pub fn function_params(params: &[ParamDef], type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    // After the first optional param, all subsequent params must also be optional
    // to satisfy PyO3's signature constraint (required params can't follow optional ones).
    let mut seen_optional = false;
    params
        .iter()
        .map(|p| {
            if p.optional {
                seen_optional = true;
            }
            let ty = if p.optional || seen_optional {
                format!("Option<{}>", type_mapper(&p.ty))
            } else {
                type_mapper(&p.ty)
            };
            format!("{}: {}", p.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a function signature defaults string (for pyo3 signature etc.).
pub fn function_sig_defaults(params: &[ParamDef]) -> String {
    // After the first optional param, all subsequent params must also use =None
    // to satisfy PyO3's signature constraint (required params can't follow optional ones).
    let mut seen_optional = false;
    params
        .iter()
        .map(|p| {
            if p.optional {
                seen_optional = true;
            }
            if p.optional || seen_optional {
                format!("{}=None", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
