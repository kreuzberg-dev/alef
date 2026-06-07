use super::{TraitBridgeGenerator, TraitBridgeSpec, format_param_type_with_lifetimes, format_return_type};

/// Generate `impl Trait for Wrapper` dispatching each method through the generator.
///
/// Methods with `has_default_impl = true` are NOT emitted — the trait's own default
/// implementation is used instead.  Only required (non-defaulted) own methods get a
/// generated vtable-forwarding body.
pub fn gen_bridge_trait_impl(spec: &TraitBridgeSpec, generator: &dyn TraitBridgeGenerator) -> String {
    let wrapper = spec.wrapper_name();
    let trait_path = spec.trait_path();

    // Check if the trait has async methods (needed for async_trait macro compatibility).
    // Only own, required (non-default) methods need this check — those are the ones we emit.
    let has_async_methods = spec
        .trait_def
        .methods
        .iter()
        .any(|m| m.is_async && m.trait_source.is_none() && !m.has_default_impl);
    let async_trait_is_send = generator.async_trait_is_send();

    // Filter out:
    // - Methods inherited from super-traits (handled by gen_bridge_plugin_impl)
    // - Methods with a default impl (let the trait's own default take effect)
    let own_methods: Vec<_> = spec
        .trait_def
        .methods
        .iter()
        .filter(|m| m.trait_source.is_none() && !m.has_default_impl)
        .collect();

    // Build method code with proper indentation
    let mut methods_code = String::with_capacity(1024);
    for (i, method) in own_methods.iter().enumerate() {
        if i > 0 {
            methods_code.push_str("\n\n");
        }

        // Build the method signature
        let async_kw = if method.is_async { "async " } else { "" };
        let receiver = match &method.receiver {
            Some(crate::core::ir::ReceiverKind::Ref) => "&self",
            Some(crate::core::ir::ReceiverKind::RefMut) => "&mut self",
            Some(crate::core::ir::ReceiverKind::Owned) => "self",
            None => "",
        };

        // Build params (excluding self), using format_param_type_with_lifetimes to respect
        // is_ref/is_mut and emit `<'_>` for core types that carry lifetime parameters.
        let params: Vec<String> = method
            .params
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p.name,
                    format_param_type_with_lifetimes(p, &spec.type_paths, &spec.lifetime_type_names)
                )
            })
            .collect();

        let all_params = if receiver.is_empty() {
            params.join(", ")
        } else if params.is_empty() {
            receiver.to_string()
        } else {
            format!("{}, {}", receiver, params.join(", "))
        };

        // Return type — override the IR's error type with the configured crate error type
        // so the impl matches the actual trait definition (the IR may extract a different
        // error type like anyhow::Error from re-exports or type alias resolution).
        // Pass `returns_ref` so Vec<T> is emitted as `&[elem]` when the trait returns a slice.
        let error_override = method.error_type.as_ref().map(|_| spec.error_path());
        let ret = format_return_type(
            &method.return_type,
            error_override.as_deref(),
            &spec.type_paths,
            method.returns_ref,
        );

        // Generate body: async methods use Box::pin, sync methods call directly
        let raw_body = if method.is_async {
            generator.gen_async_method_body(method, spec)
        } else {
            generator.gen_sync_method_body(method, spec)
        };

        // When the trait method returns `&[&str]` (i.e. Vec<String> + returns_ref), the
        // foreign bridge body returns an owned Vec<String> (via unwrap_or_default or similar).
        // Wrap it with Box::leak so the &'static str slice satisfies the return type.
        // This is correct for the plugin registration pattern: supported_mime_types() is
        // called once per registration and the data is process-global.
        //
        // Exception: when the raw_body is already a reference to a cached
        // `&'static [&'static str]` field (e.g. FFI's `self.{name}_strs` fast-path),
        // there is nothing to leak — return it directly.
        let raw_body_trimmed = raw_body.trim();
        let body_is_static_slice = raw_body_trimmed.starts_with("self.") && raw_body_trimmed.ends_with("_strs");
        let body = if method.returns_ref
            && matches!(&method.return_type, crate::core::ir::TypeRef::Vec(inner) if matches!(inner.as_ref(), crate::core::ir::TypeRef::String))
        {
            if body_is_static_slice {
                raw_body
            } else {
                format!(
                    "let __types: Vec<String> = {{ {raw_body} }};\n\
                     let __strs: Vec<&'static str> = __types.into_iter()\n\
                         .map(|s| -> &'static str {{ Box::leak(s.into_boxed_str()) }})\n\
                         .collect();\n\
                     Box::leak(__strs.into_boxed_slice())"
                )
            }
        } else {
            raw_body
        };

        // Indent body lines
        let indented_body = body
            .lines()
            .map(|line| format!("        {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        methods_code.push_str(&crate::codegen::template_env::render(
            "generators/trait_bridge/trait_method.jinja",
            minijinja::context! {
                async_kw => async_kw,
                method_name => &method.name,
                all_params => all_params,
                ret => ret,
                indented_body => &indented_body,
            },
        ));
    }

    crate::codegen::template_env::render(
        "generators/trait_bridge/trait_impl.jinja",
        minijinja::context! {
            has_async_methods => has_async_methods,
            async_trait_is_send => async_trait_is_send,
            trait_path => trait_path,
            wrapper_name => wrapper,
            methods_code => methods_code,
        },
    )
}
