use crate::backends::zig::gen_bindings::errors::resolve_zig_error_type;
use crate::backends::zig::gen_bindings::functions::zig_return_type;
use crate::backends::zig::gen_bindings::helpers::emit_cleaned_zig_doc;
use crate::core::ir::{MethodDef, ParamDef, TypeDef, TypeRef};
use heck::AsSnakeCase;
use std::collections::{HashMap, HashSet};

use super::params::{
    emit_method_param_conversion, emit_method_param_free, method_c_arg_names, method_param_needs_alloc,
    method_param_needs_from_json, param_zig_type_with_enums,
};
use super::render;
use super::returns::method_unwrap_return_expr;
use super::streaming::emit_opaque_streaming_method;

/// Emit a single method on an opaque handle wrapper struct.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_opaque_method(
    method: &MethodDef,
    ty: &TypeDef,
    prefix: &str,
    type_snake: &str,
    declared_errors: &[String],
    struct_names: &HashSet<String>,
    streaming_item_types: &HashMap<String, String>,
    enum_names: &HashSet<String>,
    out: &mut String,
) {
    if let Some(item_type) = streaming_item_types.get(&method.name) {
        emit_opaque_streaming_method(method, ty, prefix, type_snake, item_type, declared_errors, out);
        return;
    }

    emit_cleaned_zig_doc(out, &method.doc, "    ");

    let renamed_params = renamed_method_params(method);
    let effective_params: &[ParamDef] = &renamed_params;
    let params_str = method_params_signature(ty, effective_params, struct_names, enum_names);

    let zig_error_type = method
        .error_type
        .as_ref()
        .map(|e| resolve_zig_error_type(e, declared_errors));
    let return_ty = method_return_type(method, effective_params, struct_names, zig_error_type.as_ref());

    out.push_str(&render(
        "opaque_method_signature.jinja",
        minijinja::context! {
            method_name => &method.name,
            params => &params_str,
            return_ty => &return_ty,
        },
    ));

    let json_error_return = zig_error_type
        .as_ref()
        .map_or("return error.InvalidJson;".to_string(), |err| {
            format!("return _first_error({err});")
        });
    for p in effective_params {
        emit_method_param_conversion(p, prefix, struct_names, enum_names, &json_error_return, out);
    }

    let returns_bytes = matches!(method.return_type, TypeRef::Bytes);
    if returns_bytes {
        out.push_str(&render("opaque_bytes_out_vars.jinja", minijinja::context! {}));
    }

    let c_call = method_c_call(
        method,
        ty,
        prefix,
        type_snake,
        effective_params,
        struct_names,
        enum_names,
    );
    emit_method_body(
        method,
        prefix,
        struct_names,
        effective_params,
        returns_bytes,
        &c_call,
        zig_error_type.as_ref(),
        out,
    );

    out.push_str("    }\n");
}

/// Emit a `free()` method that releases the underlying FFI handle by calling
/// `c.{prefix}_{snake_type}_free(self._handle)`. The C destructor is generated
/// by the FFI crate for every opaque handle type.
pub(super) fn emit_opaque_free(ty: &TypeDef, prefix: &str, type_snake: &str, out: &mut String) {
    let upper_prefix = prefix.to_uppercase();
    out.push_str(&render(
        "opaque_free_method.jinja",
        minijinja::context! {
            type_name => &ty.name,
            prefix => prefix,
            type_snake => type_snake,
            upper_prefix => &upper_prefix,
        },
    ));
}

fn renamed_method_params(method: &MethodDef) -> Vec<ParamDef> {
    method
        .params
        .iter()
        .map(|p| {
            if p.name == method.name {
                let mut p2 = p.clone();
                p2.name = "value".to_string();
                p2
            } else {
                p.clone()
            }
        })
        .collect()
}

fn method_params_signature(
    ty: &TypeDef,
    params: &[ParamDef],
    struct_names: &HashSet<String>,
    enum_names: &HashSet<String>,
) -> String {
    let mut param_parts = Vec::new();
    param_parts.push(format!("self: *{}", ty.name));
    for p in params {
        let ty_str = param_zig_type_with_enums(&p.ty, p.optional, struct_names, enum_names);
        param_parts.push(format!("{}: {}", p.name, ty_str));
    }
    param_parts.join(", ")
}

fn method_return_type(
    method: &MethodDef,
    params: &[ParamDef],
    struct_names: &HashSet<String>,
    zig_error_type: Option<&String>,
) -> String {
    let body_needs_try = params.iter().any(method_param_needs_alloc)
        || matches!(
            &method.return_type,
            TypeRef::String | TypeRef::Path | TypeRef::Json | TypeRef::Bytes | TypeRef::Vec(_) | TypeRef::Map(_, _)
        )
        || matches!(&method.return_type, TypeRef::Named(name) if struct_names.contains(name));
    let body_needs_invalid_json = params.iter().any(|p| method_param_needs_from_json(p, struct_names));

    let ret_ty_inner = zig_return_type(&method.return_type, struct_names);
    if let Some(err_ty) = zig_error_type {
        format!("({err_ty}||error{{OutOfMemory}})!{ret_ty_inner}")
    } else if body_needs_try || body_needs_invalid_json {
        let err_set = if body_needs_invalid_json {
            "error{OutOfMemory,InvalidJson}"
        } else {
            "error{OutOfMemory}"
        };
        format!("{err_set}!{ret_ty_inner}")
    } else {
        ret_ty_inner
    }
}

fn method_c_call(
    method: &MethodDef,
    ty: &TypeDef,
    prefix: &str,
    type_snake: &str,
    params: &[ParamDef],
    struct_names: &HashSet<String>,
    enum_names: &HashSet<String>,
) -> String {
    let method_snake = AsSnakeCase(&method.name).to_string();
    let upper_prefix = prefix.to_uppercase();
    let c_handle = format!(
        "@as(*c.{upper_prefix}{type_name}, @ptrCast(self._handle))",
        type_name = ty.name,
    );
    let mut c_args = vec![c_handle];
    for p in params {
        c_args.extend(method_c_arg_names(p, struct_names, enum_names));
    }
    if matches!(method.return_type, TypeRef::Bytes) {
        c_args.push("&_out_ptr".to_string());
        c_args.push("&_out_len".to_string());
        c_args.push("&_out_cap".to_string());
    }
    format!(
        "c.{prefix}_{type_snake}_{method_snake}({args})",
        args = c_args.join(", ")
    )
}

fn emit_method_body(
    method: &MethodDef,
    prefix: &str,
    struct_names: &HashSet<String>,
    params: &[ParamDef],
    returns_bytes: bool,
    c_call: &str,
    zig_error_type: Option<&String>,
    out: &mut String,
) {
    if let Some(err_ty) = zig_error_type {
        emit_fallible_method_body(method, prefix, struct_names, params, returns_bytes, c_call, err_ty, out);
    } else {
        emit_infallible_method_body(method, prefix, struct_names, params, returns_bytes, c_call, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_fallible_method_body(
    method: &MethodDef,
    prefix: &str,
    struct_names: &HashSet<String>,
    params: &[ParamDef],
    returns_bytes: bool,
    c_call: &str,
    err_ty: &str,
    out: &mut String,
) {
    let result_is_pointer = !(matches!(method.return_type, TypeRef::Unit) || returns_bytes);
    if !result_is_pointer {
        out.push_str(&render(
            "opaque_method_call_discard.jinja",
            minijinja::context! {
                c_call => c_call,
            },
        ));
    } else {
        out.push_str(&render(
            "opaque_method_call_result.jinja",
            minijinja::context! {
                c_call => c_call,
            },
        ));
    }

    if result_is_pointer {
        out.push_str("        if (_result == null) {\n");
        out.push_str(&format!("            return _first_error({err_ty});\n"));
        out.push_str("        }\n");
    } else {
        out.push_str(&render(
            "opaque_method_error_check.jinja",
            minijinja::context! {
                prefix => prefix,
                error_type => err_ty,
            },
        ));
    }

    for p in params {
        emit_method_param_free(p, struct_names);
    }

    if returns_bytes {
        out.push_str(&render(
            "opaque_bytes_return.jinja",
            minijinja::context! {
                prefix => prefix,
            },
        ));
    } else if !matches!(method.return_type, TypeRef::Unit) {
        let ret_expr = method_unwrap_return_expr("_result", &method.return_type, prefix, struct_names);
        out.push_str(&render(
            "opaque_method_return.jinja",
            minijinja::context! {
                ret_expr => &ret_expr,
            },
        ));
    }
}

fn emit_infallible_method_body(
    method: &MethodDef,
    prefix: &str,
    struct_names: &HashSet<String>,
    params: &[ParamDef],
    returns_bytes: bool,
    c_call: &str,
    out: &mut String,
) {
    for p in params {
        emit_method_param_free(p, struct_names);
    }
    if returns_bytes {
        out.push_str(&render(
            "opaque_method_call_discard.jinja",
            minijinja::context! {
                c_call => c_call,
            },
        ));
        out.push_str(&render(
            "opaque_bytes_return.jinja",
            minijinja::context! {
                prefix => prefix,
            },
        ));
    } else if matches!(method.return_type, TypeRef::Unit) {
        out.push_str(&render(
            "opaque_method_unit_call.jinja",
            minijinja::context! {
                c_call => c_call,
            },
        ));
    } else {
        out.push_str(&render(
            "opaque_method_call_result.jinja",
            minijinja::context! {
                c_call => c_call,
            },
        ));
        let ret_expr = method_unwrap_return_expr("_result", &method.return_type, prefix, struct_names);
        out.push_str(&render(
            "opaque_method_return.jinja",
            minijinja::context! {
                ret_expr => &ret_expr,
            },
        ));
    }
}
