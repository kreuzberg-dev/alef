use crate::core::ir::TypeRef;
use heck::AsSnakeCase;
use std::collections::HashSet;

/// Produce the Zig return expression for an opaque method result.
pub(super) fn method_unwrap_return_expr(
    raw: &str,
    ty: &TypeRef,
    prefix: &str,
    struct_names: &HashSet<String>,
) -> String {
    match ty {
        TypeRef::String | TypeRef::Path | TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _) => {
            format!(
                "blk: {{\n            const slice = std.mem.span({raw});\n            const owned = try std.heap.c_allocator.dupe(u8, slice);\n            c.{prefix}_free_string({raw});\n            break :blk owned;\n        }}"
            )
        }
        TypeRef::Named(name) if struct_names.contains(name) => {
            let snake = AsSnakeCase(name).to_string();
            format!(
                "blk: {{\n            const _json_ptr = c.{prefix}_{snake}_to_json({raw});\n            const _json_slice = std.mem.span(_json_ptr);\n            const owned = try std.heap.c_allocator.dupe(u8, _json_slice);\n            c.{prefix}_free_string(_json_ptr);\n            c.{prefix}_{snake}_free({raw});\n            break :blk owned;\n        }}"
            )
        }
        TypeRef::Named(name) => {
            format!("{name}{{ ._handle = {raw}.? }}")
        }
        _ => raw.to_string(),
    }
}
