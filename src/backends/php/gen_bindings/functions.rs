mod async_methods;
mod methods;
mod params;
mod stubs;

pub(crate) use async_methods::{
    gen_async_function_as_static_method, gen_async_instance_method, gen_async_static_method,
};
pub(crate) use methods::{
    gen_function_as_static_method, gen_instance_method, gen_instance_method_non_opaque, gen_static_method,
};
pub(crate) use params::PhpParamTypeSets;
