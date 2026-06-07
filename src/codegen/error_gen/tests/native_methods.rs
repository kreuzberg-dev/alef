use super::*;

#[test]
fn test_gen_ffi_error_codes() {
    let error = sample_error();
    let output = gen_ffi_error_codes(&error);
    assert!(output.contains("CONVERSION_ERROR_NONE = 0"));
    assert!(output.contains("CONVERSION_ERROR_PARSE_ERROR = 1"));
    assert!(output.contains("CONVERSION_ERROR_IO_ERROR = 2"));
    assert!(output.contains("CONVERSION_ERROR_OTHER = 3"));
    assert!(output.contains("conversion_error_t;"));
    assert!(output.contains("conversion_error_error_message(conversion_error_t code)"));
}

// -----------------------------------------------------------------------
// Go tests
// -----------------------------------------------------------------------

#[test]
fn test_gen_go_error_types() {
    let error = sample_error();
    // Package name that does NOT match the error prefix — type name stays unchanged.
    let output = gen_go_error_types(&error, "mylib");
    assert!(output.contains("ErrParseError = errors.New("));
    assert!(output.contains("ErrIoError = errors.New("));
    assert!(output.contains("ErrOther = errors.New("));
    assert!(output.contains("type ConversionError struct {"));
    assert!(output.contains("Code    string"));
    assert!(output.contains("func (e ConversionError) Error() string"));
    // Each sentinel error var should have a doc comment.
    assert!(output.contains("// ErrParseError is returned when"));
    assert!(output.contains("// ErrIoError is returned when"));
    assert!(output.contains("// ErrOther is returned when"));
}

#[test]
fn test_gen_go_error_types_stutter_strip() {
    let error = sample_error();
    // "conversion" package — "ConversionError" starts with "conversion" (case-insensitive)
    // so the exported Go type should be "Error", not "ConversionError".
    let output = gen_go_error_types(&error, "conversion");
    assert!(
        output.contains("type Error struct {"),
        "expected stutter strip, got:\n{output}"
    );
    assert!(
        output.contains("func (e Error) Error() string"),
        "expected stutter strip, got:\n{output}"
    );
    // Sentinel vars are unaffected by stutter stripping.
    assert!(output.contains("ErrParseError = errors.New("));
}

// -----------------------------------------------------------------------
// Java tests
// -----------------------------------------------------------------------

#[test]
fn test_gen_java_error_types() {
    let error = sample_error();
    let files = gen_java_error_types(&error, "dev.sample_crate.test");
    // base + 3 variants
    assert_eq!(files.len(), 4);
    // Base class
    assert_eq!(files[0].0, "ConversionErrorException");
    assert!(
        files[0]
            .1
            .contains("public class ConversionErrorException extends Exception")
    );
    assert!(files[0].1.contains("package dev.sample_crate.test;"));
    // Variant classes
    assert_eq!(files[1].0, "ParseErrorException");
    assert!(
        files[1]
            .1
            .contains("public class ParseErrorException extends ConversionErrorException")
    );
    assert_eq!(files[2].0, "IoErrorException");
    assert_eq!(files[3].0, "OtherException");
}

// -----------------------------------------------------------------------
// C# tests
// -----------------------------------------------------------------------

#[test]
fn test_gen_csharp_error_types() {
    let error = sample_error();
    // Without fallback class: base inherits from Exception.
    let files = gen_csharp_error_types(&error, "SampleCrate.Test", None);
    assert_eq!(files.len(), 4);
    assert_eq!(files[0].0, "ConversionErrorException");
    assert!(files[0].1.contains("public class ConversionErrorException : Exception"));
    assert!(files[0].1.contains("namespace SampleCrate.Test;"));
    assert_eq!(files[1].0, "ParseErrorException");
    assert!(
        files[1]
            .1
            .contains("public class ParseErrorException : ConversionErrorException")
    );
    assert_eq!(files[2].0, "IoErrorException");
    assert_eq!(files[3].0, "OtherException");
}

#[test]
fn test_gen_csharp_error_types_with_fallback() {
    let error = sample_error();
    // With fallback class: base inherits from the generic library exception.
    let files = gen_csharp_error_types(&error, "SampleCrate.Test", Some("TestLibException"));
    assert_eq!(files.len(), 4);
    assert!(
        files[0]
            .1
            .contains("public class ConversionErrorException : TestLibException")
    );
    // Variant classes still inherit from the base error class, not from the fallback directly.
    assert!(
        files[1]
            .1
            .contains("public class ParseErrorException : ConversionErrorException")
    );
}

// -----------------------------------------------------------------------
// python_exception_name tests
// -----------------------------------------------------------------------

#[test]
fn test_python_exception_name_no_conflict() {
    // "ParseError" already ends with "Error" and is not a builtin
    assert_eq!(python_exception_name("ParseError", "ConversionError"), "ParseError");
    // "Other" gets "Error" suffix, "OtherError" is not a builtin
    assert_eq!(python_exception_name("Other", "ConversionError"), "OtherError");
}

#[test]
fn test_python_exception_name_shadows_builtin() {
    // "Connection" -> "ConnectionError" shadows builtin -> prefix with "Crawl"
    assert_eq!(
        python_exception_name("Connection", "CrawlError"),
        "CrawlConnectionError"
    );
    // "Timeout" -> "TimeoutError" shadows builtin -> prefix with "Crawl"
    assert_eq!(python_exception_name("Timeout", "CrawlError"), "CrawlTimeoutError");
    // "ConnectionError" already ends with "Error", still shadows -> prefix
    assert_eq!(
        python_exception_name("ConnectionError", "CrawlError"),
        "CrawlConnectionError"
    );
}

#[test]
fn test_python_exception_name_no_double_prefix() {
    // If variant is already prefixed with the error base, don't double-prefix
    assert_eq!(
        python_exception_name("CrawlConnectionError", "CrawlError"),
        "CrawlConnectionError"
    );
}

// -----------------------------------------------------------------------
// WASM error methods tests
// -----------------------------------------------------------------------

#[test]
fn test_gen_wasm_error_methods_empty_when_no_methods() {
    let error = sample_error(); // methods: vec![]
    let output = gen_wasm_error_methods(&error, "sample_markdown_rs", "");
    assert!(output.is_empty(), "should produce no output when methods is empty");
}

#[test]
fn test_gen_wasm_error_methods_struct_and_impl() {
    let error = error_with_methods();
    // wasm_prefix is the full type prefix, e.g. "Wasm" — the struct name is
    // {wasm_prefix}{ErrorName} = "WasmSampleLlmError".
    let output = gen_wasm_error_methods(&error, "sample_llm", "Wasm");
    // Struct definition
    assert!(
        output.contains("pub struct WasmSampleLlmError"),
        "must emit opaque struct: {output}"
    );
    assert!(
        output.contains("pub(crate) inner: sample_llm::error::SampleLlmError"),
        "{output}"
    );
    // Impl block
    assert!(output.contains("#[wasm_bindgen]\nimpl WasmSampleLlmError"), "{output}");
    // Methods with camelCase js_name
    assert!(output.contains("js_name = \"statusCode\""), "{output}");
    assert!(output.contains("pub fn status_code(&self) -> u16"), "{output}");
    assert!(output.contains("self.inner.status_code()"), "{output}");
    assert!(output.contains("js_name = \"isTransient\""), "{output}");
    assert!(output.contains("pub fn is_transient(&self) -> bool"), "{output}");
    assert!(output.contains("self.inner.is_transient()"), "{output}");
    assert!(output.contains("js_name = \"errorType\""), "{output}");
    assert!(output.contains("pub fn error_type(&self) -> String"), "{output}");
    assert!(output.contains("self.inner.error_type().to_string()"), "{output}");
}

// -----------------------------------------------------------------------
// FFI error methods tests
// -----------------------------------------------------------------------

#[test]
fn test_gen_ffi_error_methods_empty_when_no_methods() {
    let error = sample_error(); // methods: vec![]
    let output = gen_ffi_error_methods(&error, "sample_markdown_rs", "sample_markup");
    assert!(output.is_empty(), "should produce no output when methods is empty");
}

#[test]
fn test_gen_ffi_error_methods_status_code() {
    let error = error_with_methods();
    let output = gen_ffi_error_methods(&error, "sample_llm", "samplellm");
    assert!(
        output.contains("pub unsafe extern \"C\" fn samplellm_sample_llm_error_status_code("),
        "must emit status_code fn: {output}"
    );
    assert!(
        output.contains("err: *const sample_llm::error::SampleLlmError"),
        "{output}"
    );
    assert!(output.contains("-> u16"), "{output}");
    assert!(output.contains("(*err).status_code()"), "{output}");
    assert!(output.contains("if err.is_null()"), "{output}");
    assert!(output.contains("return 0;"), "{output}");
}

#[test]
fn test_gen_ffi_error_methods_is_transient() {
    let error = error_with_methods();
    let output = gen_ffi_error_methods(&error, "sample_llm", "samplellm");
    assert!(
        output.contains("pub unsafe extern \"C\" fn samplellm_sample_llm_error_is_transient("),
        "must emit is_transient fn: {output}"
    );
    assert!(output.contains("-> bool"), "{output}");
    assert!(output.contains("(*err).is_transient()"), "{output}");
    assert!(output.contains("return false;"), "{output}");
}

#[test]
fn test_gen_ffi_error_methods_error_type_with_free() {
    let error = error_with_methods();
    let output = gen_ffi_error_methods(&error, "sample_llm", "samplellm");
    assert!(
        output.contains("pub unsafe extern \"C\" fn samplellm_sample_llm_error_error_type("),
        "must emit error_type fn: {output}"
    );
    assert!(output.contains("-> *mut std::ffi::c_char"), "{output}");
    assert!(output.contains("(*err).error_type()"), "{output}");
    assert!(output.contains("CString::new(s)"), "{output}");
    assert!(output.contains(".into_raw()"), "{output}");
    assert!(output.contains("return std::ptr::null_mut();"), "{output}");
    // free companion
    assert!(
        output.contains("pub unsafe extern \"C\" fn samplellm_sample_llm_error_error_type_free("),
        "must emit _free companion: {output}"
    );
    assert!(output.contains("drop(std::ffi::CString::from_raw(ptr))"), "{output}");
}

#[test]
fn test_gen_ffi_error_methods_safety_comments() {
    let error = error_with_methods();
    let output = gen_ffi_error_methods(&error, "sample_llm", "samplellm");
    assert!(output.contains("// SAFETY:"), "must include SAFETY comments: {output}");
}
