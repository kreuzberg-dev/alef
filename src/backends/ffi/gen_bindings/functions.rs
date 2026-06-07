mod orchestration;
mod params;
mod return_handling;
mod signatures;
mod support;

pub(super) use orchestration::{gen_free_function, gen_method_wrapper, gen_streaming_method_wrapper};
pub(super) use return_handling::returns_c_char;
pub(super) use signatures::{gen_free_function_len_companion, should_skip_method_wrapper};

#[cfg(test)]
mod tests;
