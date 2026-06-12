mod bridge_function;
mod generator;
mod interfaces;
mod visitor;

pub use crate::codegen::generators::trait_bridge::find_bridge_param;
pub use bridge_function::gen_bridge_function;
pub use generator::{PhpBridgeGenerator, gen_trait_bridge};
pub use interfaces::{gen_registration_interface, gen_visitor_interface};
