use ahash::AHashSet;
use alef_codegen::generators::{AdapterBodies, AsyncPattern, RustBindingConfig, gen_enum, gen_function, gen_struct};
use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::{EnumDef, EnumVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef, TypeRef};
use std::borrow::Cow;

/// Minimal TypeMapper using plain Rust type names (no backend-specific overrides).
struct RustMapper;

impl TypeMapper for RustMapper {
    fn string(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}

fn default_cfg<'a>() -> RustBindingConfig<'a> {
    RustBindingConfig {
        struct_attrs: &[],
        field_attrs: &[],
        struct_derives: &["Clone", "Debug"],
        method_block_attr: None,
        constructor_attr: "",
        static_attr: None,
        function_attr: "#[no_mangle]",
        enum_attrs: &[],
        enum_derives: &["Clone", "Debug", "PartialEq"],
        needs_signature: false,
        signature_prefix: "",
        signature_suffix: "",
        core_import: "my_crate",
        async_pattern: AsyncPattern::None,
        has_serde: false,
        type_name_prefix: "",
    }
}

fn simple_type_def() -> TypeDef {
    TypeDef {
        name: "MyConfig".to_string(),
        rust_path: "my_crate::MyConfig".to_string(),
        fields: vec![
            FieldDef {
                name: "name".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                doc: "The config name.".to_string(),
                sanitized: false,
                is_boxed: false,
                type_rust_path: None,
                cfg: None,
                typed_default: None,
            },
            FieldDef {
                name: "count".to_string(),
                ty: TypeRef::Primitive(PrimitiveType::U32),
                optional: true,
                default: None,
                doc: String::new(),
                sanitized: false,
                is_boxed: false,
                type_rust_path: None,
                cfg: None,
                typed_default: None,
            },
        ],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        doc: "A minimal config type.".to_string(),
        cfg: None,
    }
}

fn simple_function_def() -> FunctionDef {
    FunctionDef {
        name: "process".to_string(),
        rust_path: "my_crate::process".to_string(),
        params: vec![ParamDef {
            name: "input".to_string(),
            ty: TypeRef::String,
            optional: false,
            default: None,
            sanitized: false,
            typed_default: None,
        }],
        return_type: TypeRef::Primitive(PrimitiveType::U32),
        is_async: false,
        error_type: None,
        doc: "Process a string input.".to_string(),
        cfg: None,
        sanitized: false,
        returns_ref: false,
    }
}

fn simple_enum_def() -> EnumDef {
    EnumDef {
        name: "OutputFormat".to_string(),
        rust_path: "my_crate::OutputFormat".to_string(),
        variants: vec![
            EnumVariant {
                name: "Json".to_string(),
                fields: vec![],
                doc: String::new(),
                is_default: true,
            },
            EnumVariant {
                name: "Csv".to_string(),
                fields: vec![],
                doc: String::new(),
                is_default: false,
            },
            EnumVariant {
                name: "Plain".to_string(),
                fields: vec![],
                doc: String::new(),
                is_default: false,
            },
        ],
        doc: "Output format options.".to_string(),
        cfg: None,
    }
}

#[test]
fn test_gen_struct_produces_struct_definition() {
    let typ = simple_type_def();
    let mapper = RustMapper;
    let cfg = default_cfg();

    let result = gen_struct(&typ, &mapper, &cfg);

    assert!(
        result.contains("pub struct MyConfig"),
        "should contain struct declaration"
    );
    assert!(result.contains("name: String"), "should contain String field");
    assert!(
        result.contains("count: Option<u32>"),
        "should contain optional u32 field"
    );
    assert!(result.contains("#[derive(Clone, Debug)]"), "should have derives");
}

#[test]
fn test_gen_function_produces_function_signature() {
    let func = simple_function_def();
    let mapper = RustMapper;
    let cfg = default_cfg();
    let adapter_bodies = AdapterBodies::default();
    let opaque_types = AHashSet::new();

    let result = gen_function(&func, &mapper, &cfg, &adapter_bodies, &opaque_types);

    assert!(result.contains("pub fn process"), "should contain function name");
    assert!(result.contains("input: String"), "should contain input param");
    assert!(result.contains("-> u32"), "should contain return type");
}

#[test]
fn test_gen_enum_produces_enum_with_variants() {
    let enum_def = simple_enum_def();
    let cfg = default_cfg();

    let result = gen_enum(&enum_def, &cfg);

    assert!(
        result.contains("pub enum OutputFormat"),
        "should contain enum declaration"
    );
    assert!(
        result.contains("Json = 0"),
        "should contain first variant with discriminant"
    );
    assert!(result.contains("Csv = 1"), "should contain second variant");
    assert!(result.contains("Plain = 2"), "should contain third variant");
    assert!(
        result.contains("#[derive(Clone, Debug, PartialEq)]"),
        "should have derives"
    );
}

#[test]
fn test_gen_enum_produces_default_impl() {
    let enum_def = simple_enum_def();
    let cfg = default_cfg();

    let result = gen_enum(&enum_def, &cfg);

    assert!(
        result.contains("impl Default for OutputFormat"),
        "should have Default impl"
    );
    assert!(result.contains("Self::Json"), "default should be first variant");
}

#[test]
fn test_gen_struct_with_empty_fields() {
    let typ = TypeDef {
        name: "Empty".to_string(),
        rust_path: "my_crate::Empty".to_string(),
        fields: vec![],
        methods: vec![],
        is_opaque: false,
        is_clone: false,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        doc: String::new(),
        cfg: None,
    };
    let mapper = RustMapper;
    let cfg = default_cfg();

    let result = gen_struct(&typ, &mapper, &cfg);

    assert!(result.contains("pub struct Empty"), "should generate empty struct");
}
