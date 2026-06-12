use crate::backends::java::type_map::{java_boxed_type, java_type};
use crate::codegen::naming::to_java_name;
use crate::core::ir::{FunctionDef, TypeRef};
use std::collections::HashSet;

use super::super::helpers::is_bridge_param_java;

pub(super) fn gen_async_wrapper_method(
    out: &mut String,
    func: &FunctionDef,
    bridge_param_names: &HashSet<String>,
    bridge_type_aliases: &HashSet<String>,
) {
    let params: Vec<String> = func
        .params
        .iter()
        .filter(|p| !is_bridge_param_java(p, bridge_param_names, bridge_type_aliases))
        .map(|p| {
            let ptype = java_type(&p.ty);
            format!("final {} {}", ptype, to_java_name(&p.name))
        })
        .collect();

    let return_type = match &func.return_type {
        TypeRef::Unit => "Void".to_string(),
        other => java_boxed_type(other).to_string(),
    };

    let sync_method_name = to_java_name(&func.name);
    let async_method_name = format!("{}Async", sync_method_name);
    let param_names: Vec<String> = func
        .params
        .iter()
        .filter(|p| !is_bridge_param_java(p, bridge_param_names, bridge_type_aliases))
        .map(|p| to_java_name(&p.name))
        .collect();

    out.push_str(&crate::backends::java::template_env::render(
        "ffi_async_method_signature.jinja",
        minijinja::context! {
            return_type => &return_type,
            async_method_name => &async_method_name,
            params => params.join(", "),
        },
    ));
    out.push_str("        return CompletableFuture.supplyAsync(() -> {\n");
    out.push_str("            try {\n");
    if matches!(func.return_type, TypeRef::Unit) {
        out.push_str("                ");
        out.push_str(&sync_method_name);
        out.push('(');
        out.push_str(&param_names.join(", "));
        out.push_str(");\n");
        out.push_str("                return null;\n");
    } else {
        out.push_str("                return ");
        out.push_str(&sync_method_name);
        out.push('(');
        out.push_str(&param_names.join(", "));
        out.push_str(");\n");
    }
    out.push_str("            } catch (Throwable e) {\n");
    out.push_str("                throw new CompletionException(e);\n");
    out.push_str("            }\n");
    out.push_str("        });\n");
    out.push_str("    }\n");
}
