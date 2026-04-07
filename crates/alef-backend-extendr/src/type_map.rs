//! Type mapping for extendr (R) bindings.
//!
//! Maps Rust types to R/extendr types with proper nullability handling.
//!
//! Type mapping for R (via extendr):
//! - bool → bool
//! - u32/i32 → i32 (R integers are 32-bit)
//! - u64/i64 → f64 (R doesn't have 64-bit integers, use doubles)
//! - f32/f64 → f64
//! - Option<T> → Nullable<T>
//! - Vec<T> → Vec<T>
//! - String → String (params), String (return)
//! - Path → String
//! - Json → String
