mod constructors;
mod impl_blocks;
mod instance;
mod opaque;
mod static_methods;
mod trait_names;

pub use constructors::{gen_constructor, gen_constructor_with_renames};
pub use impl_blocks::{gen_impl_block, gen_impl_block_with_renames};
pub use instance::gen_method;
pub use opaque::{gen_opaque_constructor, gen_opaque_impl_block};
pub use static_methods::gen_static_method;
pub use trait_names::is_trait_method_name;
