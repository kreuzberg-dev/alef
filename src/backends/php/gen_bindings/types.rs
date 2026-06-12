mod enums;
mod structs;

#[allow(unused_imports)]
pub(crate) use enums::{
    gen_enum_constants, gen_flat_data_enum, gen_flat_data_enum_from_impls, gen_flat_data_enum_methods,
    is_tagged_data_enum, is_untagged_data_enum, ty_references_untagged_data_enum,
};
#[allow(unused_imports)]
pub(crate) use structs::{gen_opaque_struct_methods_with_exclude, gen_php_struct, gen_struct_methods};
pub use structs::{gen_struct_methods_with_exclude, is_php_prop_scalar};
