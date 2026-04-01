use skif_core::ir::{FieldDef, MethodDef, ParamDef, TypeRef};

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
    let params: Vec<String> = fields
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

    let defaults: Vec<String> = fields
        .iter()
        .map(|f| {
            if f.optional {
                format!("{}=None", f.name)
            } else {
                f.name.clone()
            }
        })
        .collect();

    let assignments: Vec<String> = fields
        .iter()
        .map(|f| format!("{name}: {name}", name = f.name))
        .collect();

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
    params
        .iter()
        .map(|p| {
            let ty = if p.optional {
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
    params
        .iter()
        .map(|p| {
            if p.optional {
                format!("{}=None", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
