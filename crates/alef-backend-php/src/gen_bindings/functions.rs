use crate::type_map::PhpMapper;
use ahash::AHashSet;
use alef_codegen::generators;
use alef_codegen::shared;
use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::{FunctionDef, MethodDef, TypeDef, TypeRef};

use super::helpers::{
    gen_php_call_args, gen_php_call_args_with_let_bindings, gen_php_function_params,
    gen_php_lossy_binding_to_core_fields, gen_php_named_let_bindings,
};

/// Generate an instance method binding for an opaque struct.
pub(crate) fn gen_instance_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    is_opaque: bool,
    type_name: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let params_str = if params.is_empty() { String::new() } else { params };

    // Exclude methods with non-opaque Named params: ext-php-rs can't pass #[php_class]
    // types by value (no FromZvalMut impl for owned php_class types).
    let has_non_opaque_named_params = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if !opaque_types.contains(n.as_str())));

    let body = if can_delegate && is_opaque && !has_non_opaque_named_params {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let is_owned_receiver = matches!(method.receiver.as_ref(), Some(alef_core::ir::ReceiverKind::Owned));
        let core_call = if is_owned_receiver {
            format!("(*self.inner).clone().{}({})", method.name, call_args)
        } else {
            format!("self.inner.{}({})", method.name, call_args)
        };
        if method.error_type.is_some() {
            if matches!(method.return_type, TypeRef::Unit) {
                format!(
                    "{core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(())"
                )
            } else {
                let wrap = generators::wrap_return(
                    "result",
                    &method.return_type,
                    type_name,
                    opaque_types,
                    true,
                    method.returns_ref,
                );
                format!(
                    "let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
                )
            }
        } else {
            generators::wrap_return(
                &core_call,
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            )
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params_str.is_empty() {
        format!(
            "{trait_allow}pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate an instance method binding for a non-opaque struct (uses gen_lossy_binding_to_core_fields).
pub(crate) fn gen_instance_method_non_opaque(
    method: &MethodDef,
    mapper: &PhpMapper,
    typ: &TypeDef,
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = !method.sanitized
        && method
            .params
            .iter()
            .all(|p| !p.sanitized && generators::is_simple_non_opaque_param(&p.ty))
        && shared::is_delegatable_return(&method.return_type);

    let params_str = if params.is_empty() { String::new() } else { params };

    let body = if can_delegate {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let field_conversions = gen_php_lossy_binding_to_core_fields(typ, core_import);
        let core_call = format!("core_self.{}({})", method.name, call_args);
        let result_wrap = match &method.return_type {
            TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()) => {
                // Enum return type: PHP maps enums to String via format!("{:?}")
                String::new()
            }
            TypeRef::Named(_) | TypeRef::String | TypeRef::Char | TypeRef::Bytes | TypeRef::Path => {
                ".into()".to_string()
            }
            _ => String::new(),
        };
        // For enum return types, wrap with format!("{:?}", ...)
        let format_enum_return =
            matches!(&method.return_type, TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()));
        if method.error_type.is_some() {
            if format_enum_return {
                format!(
                    "{field_conversions}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(format!(\"{{:?}}\", result))"
                )
            } else {
                format!(
                    "{field_conversions}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(result{result_wrap})"
                )
            }
        } else if format_enum_return {
            format!("{field_conversions}format!(\"{{:?}}\", {core_call})")
        } else {
            format!("{field_conversions}{core_call}{result_wrap}")
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params_str.is_empty() {
        format!(
            "{trait_allow}pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate a static method binding.
pub(crate) fn gen_static_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    typ: &TypeDef,
    _core_import: &str,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);
    let core_type_path = typ.rust_path.replace('-', "_");
    let call_args = gen_php_call_args(&method.params, opaque_types);

    // Exclude methods with non-opaque Named params (FromZvalMut issue with php_class)
    let has_non_opaque_named_params = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if !opaque_types.contains(n.as_str())));

    let body = if can_delegate && !has_non_opaque_named_params {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        let is_enum_return = matches!(&method.return_type, TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()));
        if method.error_type.is_some() {
            if is_enum_return {
                format!(
                    "{core_call}.map(|val| format!(\"{{:?}}\", val)).map_err(|e| PhpException::default(e.to_string()))"
                )
            } else {
                let wrap = generators::wrap_return(
                    "val",
                    &method.return_type,
                    &typ.name,
                    opaque_types,
                    typ.is_opaque,
                    method.returns_ref,
                );
                if wrap == "val" {
                    format!("{core_call}.map_err(|e| PhpException::default(e.to_string()))")
                } else {
                    format!("{core_call}.map(|val| {wrap}).map_err(|e| PhpException::default(e.to_string()))")
                }
            }
        } else if is_enum_return {
            format!("format!(\"{{:?}}\", {core_call})")
        } else {
            generators::wrap_return(
                &core_call,
                &method.return_type,
                &typ.name,
                opaque_types,
                typ.is_opaque,
                method.returns_ref,
            )
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params.is_empty() {
        format!(
            "{trait_allow}pub fn {}() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate a free function binding.
pub(crate) fn gen_function(
    func: &FunctionDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let params = gen_php_function_params(&func.params, mapper, opaque_types);
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let let_bindings = gen_php_named_let_bindings(&func.params, opaque_types, core_import);
        let call_args = gen_php_call_args_with_let_bindings(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        if func.error_type.is_some() {
            let wrap = generators::wrap_return("result", &func.return_type, "", opaque_types, false, func.returns_ref);
            format!(
                "{let_bindings}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
            )
        } else {
            format!(
                "{let_bindings}{}",
                generators::wrap_return(&core_call, &func.return_type, "", opaque_types, false, func.returns_ref)
            )
        }
    } else {
        gen_php_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some())
    };

    if params.is_empty() {
        format!(
            "#[php_function]\npub fn {}() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    } else {
        format!(
            "#[php_function]\npub fn {}({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    }
}

/// Generate an async free function binding for PHP (block on runtime).
pub(crate) fn gen_async_function(
    func: &FunctionDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let params = gen_php_function_params(&func.params, mapper, opaque_types);
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let let_bindings = gen_php_named_let_bindings(&func.params, opaque_types, core_import);
        let call_args = gen_php_call_args_with_let_bindings(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let result_wrap =
            generators::wrap_return("result", &func.return_type, "", opaque_types, false, func.returns_ref);
        if func.error_type.is_some() {
            format!(
                "{let_bindings}WORKER_RUNTIME.block_on(async {{\n        \
                 let result = {core_call}.await.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n        \
                 Ok({result_wrap})\n    }})"
            )
        } else {
            format!(
                "{let_bindings}let result = WORKER_RUNTIME.block_on(async {{ {core_call}.await }});\n    {result_wrap}"
            )
        }
    } else {
        gen_php_unimplemented_body(
            &func.return_type,
            &format!("{}_async", func.name),
            func.error_type.is_some(),
        )
    };

    if params.is_empty() {
        format!(
            "#[php_function]\npub fn {}_async() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    } else {
        format!(
            "#[php_function]\npub fn {}_async({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    }
}

/// Generate an async instance method binding for PHP (block on runtime).
pub(crate) fn gen_async_instance_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    is_opaque: bool,
    type_name: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate && is_opaque {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let inner_clone = "let inner = self.inner.clone();\n    ";
        let core_call = format!("inner.{}({})", method.name, call_args);
        let result_wrap = generators::wrap_return(
            "result",
            &method.return_type,
            type_name,
            opaque_types,
            true,
            method.returns_ref,
        );
        if method.error_type.is_some() {
            format!(
                "{inner_clone}WORKER_RUNTIME.block_on(async {{\n        \
                 let result = {core_call}.await.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n        \
                 Ok({result_wrap})\n    }})"
            )
        } else {
            format!(
                "{inner_clone}let result = WORKER_RUNTIME.block_on(async {{ {core_call}.await }});\n    {result_wrap}"
            )
        }
    } else {
        gen_php_unimplemented_body(
            &method.return_type,
            &format!("{}_async", method.name),
            method.error_type.is_some(),
        )
    };

    if params.is_empty() {
        format!(
            "pub fn {}_async(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}_async(&self, {params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate an async static method binding for PHP (block on runtime).
pub(crate) fn gen_async_static_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_php_unimplemented_body(
        &method.return_type,
        &format!("{}_async", method.name),
        method.error_type.is_some(),
    );

    if params.is_empty() {
        format!(
            "pub fn {}_async() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}_async({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate a type-appropriate unimplemented body for PHP (no todo!()).
pub(crate) fn gen_php_unimplemented_body(
    return_type: &alef_core::ir::TypeRef,
    fn_name: &str,
    has_error: bool,
) -> String {
    use alef_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(ext_php_rs::exception::PhpException::default(\"{err_msg}\".to_string()))")
    } else {
        match return_type {
            TypeRef::Unit => "()".to_string(),
            TypeRef::String | TypeRef::Char | TypeRef::Path => format!("String::from(\"[unimplemented: {fn_name}]\")"),
            TypeRef::Bytes => "Vec::new()".to_string(),
            TypeRef::Primitive(p) => match p {
                alef_core::ir::PrimitiveType::Bool => "false".to_string(),
                _ => "0".to_string(),
            },
            TypeRef::Optional(_) => "None".to_string(),
            TypeRef::Vec(_) => "Vec::new()".to_string(),
            TypeRef::Map(_, _) => "Default::default()".to_string(),
            TypeRef::Named(_) | TypeRef::Json => format!("panic!(\"alef: {fn_name} not auto-delegatable\")"),
            TypeRef::Duration => "std::time::Duration::default()".to_string(),
        }
    }
}
