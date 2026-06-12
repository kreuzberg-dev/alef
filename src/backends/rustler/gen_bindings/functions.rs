mod args;
mod async_functions;
mod async_methods;
mod default_deserialization;
mod shared;
mod sync_functions;
mod sync_methods;

pub(super) use async_functions::gen_nif_async_function;
pub(super) use async_methods::gen_nif_async_method;
pub(super) use sync_functions::gen_nif_function;
pub(super) use sync_methods::gen_nif_method;
