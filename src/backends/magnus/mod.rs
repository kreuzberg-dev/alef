//! Ruby (Magnus) binding generator backend for alef.

use std::borrow::Cow;

use crate::core::ir::FunctionDef;

mod gen_bindings;
mod gen_stubs;
pub(crate) mod template_env;
pub mod trait_bridge;
mod type_map;

pub use gen_bindings::MagnusBackend;

pub(crate) fn ruby_public_function_name(func: &FunctionDef) -> &str {
    rust_path_leaf(&func.original_rust_path).unwrap_or(func.name.as_str())
}

pub(crate) fn ruby_native_function_name(func: &FunctionDef) -> Cow<'_, str> {
    if !func.is_async {
        return Cow::Borrowed(func.name.as_str());
    }

    if func.name.ends_with("_async") {
        Cow::Borrowed(func.name.as_str())
    } else {
        Cow::Owned(format!("{}_async", func.name))
    }
}

fn rust_path_leaf(path: &str) -> Option<&str> {
    let leaf = path.rsplit("::").next()?;
    let name = leaf.strip_prefix("r#").unwrap_or(leaf);
    if name.is_empty() { None } else { Some(name) }
}
