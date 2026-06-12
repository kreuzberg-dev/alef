/// Emit a shim for an instance method on an opaque client type.
///
/// `receiver_is_mut` controls whether the handle is cast to `*mut T` (`&mut self`)
/// or `*const T` (`&self`).  `opaque_type_names` is used to identify handle-typed
/// params so they can be received as `jlong` rather than a JSON string.
#[allow(clippy::too_many_arguments)]
fn emit_method_shim(
    out: &mut String,
    symbol: &str,
    type_name: &str,
    method_name: &str,
    params: &[ParamDef],
    return_type: &TypeRef,
    is_async: bool,
    has_error: bool,
    receiver_is_mut: bool,
    receiver_owned: bool,
    opaque_type_names: &std::collections::HashSet<&str>,
) {
    let rust_method = method_name.replace('-', "_");
    let has_params = !params.is_empty();

    // Direct opaque return: `-> NamedType` where the type is opaque.
    let is_opaque_return = matches!(return_type, TypeRef::Named(n) if opaque_type_names.contains(n.as_str()));
    // Optional opaque return: `-> Option<NamedType>` where the inner type is opaque.
    let is_optional_opaque_return = matches!(
        return_type,
        TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Named(n) if opaque_type_names.contains(n.as_str()))
    );

    let ret_decl = if is_opaque_return || is_optional_opaque_return {
        " -> jlong".to_string()
    } else {
        method_return_type_decl(return_type)
    };
    let ret_null = if is_opaque_return || is_optional_opaque_return {
        "0"
    } else {
        method_return_null(return_type)
    };

    // For single-param methods with Vec<u8>/Bytes params: use jbyteArray as the
    // JNI parameter type (param name matches the rust param name, not request_json).
    // All other single-param and all multi-param methods use request_json: JString.
    let request_param = if !has_params {
        String::new()
    } else if params.len() == 1 {
        let p = &params[0];
        let rust_name = p.name.replace('-', "_");
        let base_ty = match &p.ty {
            TypeRef::Optional(inner) => inner.as_ref(),
            other => other,
        };
        match base_ty {
            TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8)) => {
                render_param_decl(&rust_name, "jbyteArray")
            }
            TypeRef::Bytes => render_param_decl(&rust_name, "jbyteArray"),
            _ => "    request_json: JString,\n".to_string(),
        }
    } else {
        "    request_json: JString,\n".to_string()
    };

    // See emit_function_shim for why we use AttachGuard::from_unowned instead
    // of EnvUnowned::with_env.
    out.push_str(&template_env::render(
        "method_shim_open.rs.jinja",
        context! {
            symbol => symbol,
            request_param => request_param,
            ret_decl => ret_decl,
        },
    ));

    // Dereference handle.
    out.push_str(&template_env::render(
        "method_client_handle.rs.jinja",
        context! {
            receiver_owned => receiver_owned,
            receiver_is_mut => receiver_is_mut,
            type_name => type_name,
        },
    ));

    // Unmarshal params and build call_args with is_ref/optional adjustments.
    let call_args: String = if !has_params {
        String::new()
    } else if params.len() == 1 {
        let p = &params[0];
        let rust_name = p.name.replace('-', "_");
        // Unwrap Optional wrapper for the JNI unmarshal type.
        let base_ty = match &p.ty {
            TypeRef::Optional(inner) => inner.as_ref(),
            other => other,
        };
        // Branches that understand the target's optional sentinel produce an
        // `Option<T>` binding directly. Other special cases bind the unwrapped
        // `T` and need `Some(name)` wrapping at the call site.
        let unmarshal_produces_option = p.optional
            && (matches!(base_ty, TypeRef::Bytes)
                || matches!(base_ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::U8)))
                || !matches!(base_ty, TypeRef::Vec(_) | TypeRef::Path | TypeRef::String));
        emit_single_param_unmarshal(out, &rust_name, base_ty, ret_null, unmarshal_produces_option);
        // Apply optional/is_ref at the call site.
        // Special case: Vec<String> with is_ref means the core expects `&[&str]`.
        // emit_single_param_unmarshal already bound `<name>_vec: Vec<String>`.
        // We need to collect `Vec<&str>` refs and pass `&<name>_refs`.
        let is_vec_string_ref =
            p.is_ref && matches!(base_ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::String));
        if is_vec_string_ref {
            let refs_name = format!("{rust_name}_refs");
            out.push_str(&render_vec_string_refs(&refs_name, &format!("{rust_name}_vec")));
            format!("&{refs_name}")
        } else if unmarshal_produces_option {
            // Binding is already `Option<T>` — pass through.
            rust_name
        } else if p.optional {
            format!("Some({rust_name})")
        } else if p.is_ref {
            format!("&{rust_name}")
        } else {
            rust_name
        }
    } else {
        // Multi-param: decode JSON map.
        out.push_str(&template_env::render(
            "request_map_unmarshal.rs.jinja",
            context! {
                ret_null => ret_null,
            },
        ));
        let mut args = Vec::new();
        for p in params {
            let rust_name = p.name.replace('-', "_");
            // Unwrap Optional for the deserialization type.
            let base_ty = match &p.ty {
                TypeRef::Optional(inner) => inner.as_ref(),
                other => other,
            };
            let type_path = type_ref_to_core_path(base_ty, "core_crate");
            out.push_str(&template_env::render(
                "request_map_param_unmarshal.rs.jinja",
                context! {
                    name => rust_name,
                    type_path => type_path,
                    ret_null => ret_null,
                },
            ));
            // Special case: Vec<String> with is_ref means the core expects `&[&str]`.
            let is_vec_string_ref =
                p.is_ref && matches!(base_ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::String));
            let call_arg = if is_vec_string_ref {
                let refs_name = format!("{rust_name}_refs");
                out.push_str(&render_vec_string_refs(&refs_name, &rust_name));
                format!("&{refs_name}")
            } else if p.optional {
                format!("Some({rust_name})")
            } else if p.is_ref {
                format!("&{rust_name}")
            } else {
                rust_name
            };
            args.push(call_arg);
        }
        args.join(", ")
    };

    // Build the call.
    let call_expr = if call_args.is_empty() {
        format!("client.{rust_method}()")
    } else {
        format!("client.{rust_method}({call_args})")
    };

    if has_error {
        let mut ok_body = String::new();
        if is_opaque_return {
            ok_body.push_str("            Box::into_raw(Box::new(v)) as jlong\n");
        } else if is_optional_opaque_return {
            ok_body.push_str("            match v {\n");
            ok_body.push_str("                None => 0i64,\n");
            ok_body.push_str("                Some(inner) => Box::into_raw(Box::new(inner)) as jlong,\n");
            ok_body.push_str("            }\n");
        } else {
            emit_return_marshal(&mut ok_body, return_type, ret_null);
        }
        render_call_result_body(out, &call_expr, is_async, true, ret_null, &ok_body, "");
    } else {
        let mut value_body = String::new();
        if is_opaque_return {
            value_body.push_str("    Box::into_raw(Box::new(v)) as jlong\n");
        } else if is_optional_opaque_return {
            value_body.push_str("    match v {\n");
            value_body.push_str("        None => 0i64,\n");
            value_body.push_str("        Some(inner) => Box::into_raw(Box::new(inner)) as jlong,\n");
            value_body.push_str("    }\n");
        } else {
            emit_return_marshal_with_indent(&mut value_body, return_type, "    ", ret_null);
        }
        render_call_result_body(out, &call_expr, is_async, false, ret_null, "", &value_body);
    }
}

fn render_call_result_body(
    out: &mut String,
    call_expr: &str,
    is_async: bool,
    has_error: bool,
    ret_null: &str,
    ok_body: &str,
    value_body: &str,
) {
    let async_call_expr = format!("runtime().block_on({call_expr})");
    out.push_str(&template_env::render(
        "call_result_body.rs.jinja",
        context! {
            call_expr => call_expr,
            async_call_expr => async_call_expr,
            is_async => is_async,
            has_error => has_error,
            ret_null => ret_null,
            ok_body => ok_body,
            value_body => value_body,
        },
    ));
}
