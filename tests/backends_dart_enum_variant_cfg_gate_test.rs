/// Test that enum variants carrying a `#[cfg(feature = "...")]` attribute cause the Dart
/// Rust crate generator to emit matching `#[cfg(...)]` guards in:
///   1. the `#[frb(mirror(...))]` enum body (so the mirror type itself is conditional), and
///   2. the `From<CoreType>` match arm (core → mirror direction), and
///   3. the `From<MirrorType>` match arm (mirror → core direction).
///
/// This is the fix for the kreuzberg regression where `ImageOutputFormat::Heif { quality }`
/// is gated behind the `heic` feature but the generated `packages/dart/rust/src/lib.rs`
/// unconditionally referenced it, causing `cargo check` failures on Android/iOS targets
/// that activate `android-target` (which excludes `heic`).
use alef::backends::dart::DartBackend;
use alef::core::backend::Backend;
use alef::core::config::{ResolvedCrateConfig, new_config::NewAlefConfig};
use alef::core::ir::{ApiSurface, CoreWrapper, EnumDef, EnumVariant, FieldDef, PrimitiveType, TypeRef};

fn make_field(name: &str, ty: TypeRef, optional: bool) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        ty,
        optional,
        default: None,
        doc: String::new(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: None,
        core_wrapper: CoreWrapper::None,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
        serde_rename: None,
        serde_flatten: false,
        binding_excluded: false,
        binding_exclusion_reason: None,
        original_type: None,
    }
}

fn make_basic_config() -> ResolvedCrateConfig {
    let toml = r#"
[workspace]
languages = ["dart"]

[[crates]]
name = "demo"
sources = ["src/lib.rs"]
version_from = "/nonexistent/Cargo.toml"
"#;
    let cfg: NewAlefConfig = toml::from_str(toml).expect("test config must parse");
    cfg.resolve().expect("test config must resolve").remove(0)
}

/// Build an `ImageOutputFormat`-shaped enum with:
///   - `Native`  — no cfg (always present)
///   - `Jpeg { quality: u8 }` — no cfg (always present)
///   - `Heif { quality: u8 }` — gated behind `feature = "heic"`
fn make_image_output_format_enum() -> EnumDef {
    EnumDef {
        name: "ImageOutputFormat".to_string(),
        rust_path: "demo::ImageOutputFormat".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            EnumVariant {
                name: "Native".to_string(),
                fields: vec![],
                doc: "Keep the original image format.".to_string(),
                is_default: true,
                serde_rename: None,
                is_tuple: false,
                binding_excluded: false,
                binding_exclusion_reason: None,
                originally_had_data_fields: false,
                cfg: None,
                version: Default::default(),
            },
            EnumVariant {
                name: "Jpeg".to_string(),
                fields: vec![make_field("quality", TypeRef::Primitive(PrimitiveType::U8), false)],
                doc: "JPEG output.".to_string(),
                is_default: false,
                serde_rename: None,
                is_tuple: false,
                binding_excluded: false,
                binding_exclusion_reason: None,
                originally_had_data_fields: false,
                cfg: None,
                version: Default::default(),
            },
            EnumVariant {
                name: "Heif".to_string(),
                fields: vec![make_field("quality", TypeRef::Primitive(PrimitiveType::U8), false)],
                doc: "HEIF/HEIC output. Requires the `heic` feature.".to_string(),
                is_default: false,
                serde_rename: None,
                is_tuple: false,
                binding_excluded: false,
                binding_exclusion_reason: None,
                originally_had_data_fields: false,
                // This is the key: the upstream variant carries `#[cfg(feature = "heic")]`.
                cfg: Some(r#"feature = "heic""#.to_string()),
                version: Default::default(),
            },
        ],
        excluded_variants: vec![],
        doc: "Output image format for extraction.".to_string(),
        cfg: None,
        is_copy: false,
        has_serde: true,
        serde_tag: None,
        serde_untagged: false,
        serde_rename_all: None,
        binding_excluded: false,
        binding_exclusion_reason: None,
        version: Default::default(),
    }
}

fn generate_lib_rs(enum_def: EnumDef) -> String {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![enum_def],
        errors: vec![],
        excluded_type_paths: ::std::collections::HashMap::new(),
        excluded_trait_names: ::std::collections::HashSet::new(),
        services: vec![],
        handler_contracts: vec![],
        unsupported_public_items: Vec::new(),
        ..Default::default()
    };
    let config = make_basic_config();
    let files = DartBackend.generate_bindings(&api, &config).unwrap();
    files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("lib.rs must be generated")
        .content
        .clone()
}

#[test]
fn cfg_gated_variant_emits_cfg_attribute_in_mirror_enum() {
    let lib_rs = generate_lib_rs(make_image_output_format_enum());

    // The mirror enum body must contain `#[cfg(feature = "heic")]` before `Heif`.
    assert!(
        lib_rs.contains(r#"#[cfg(feature = "heic")]"#),
        "Expected #[cfg(feature = \"heic\")] in generated lib.rs mirror enum, but not found.\n\
         Generated output (enum section):\n{lib_rs}",
    );
}

#[test]
fn cfg_gated_variant_cfg_precedes_variant_in_mirror_body() {
    let lib_rs = generate_lib_rs(make_image_output_format_enum());

    // The `#[cfg(...)]` must appear before the `Heif {` line (i.e. at a lower line index).
    let cfg_line = lib_rs
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(r#"#[cfg(feature = "heic")]"#))
        .map(|(i, _)| i)
        .expect("#[cfg(feature = \"heic\")] line not found");

    let heif_line = lib_rs
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains("Heif"))
        .map(|(i, _)| i)
        .expect("Heif variant line not found");

    assert!(
        cfg_line < heif_line,
        "#[cfg] attribute (line {cfg_line}) must precede the Heif variant (line {heif_line})",
    );
}

#[test]
fn non_cfg_variants_have_no_cfg_attribute() {
    let lib_rs = generate_lib_rs(make_image_output_format_enum());

    // Verify that `#[cfg(feature = "heic")]` appears exactly twice:
    //   1. In the `#[frb(mirror(...))]` enum body before the `Heif` variant.
    //   2. In the `From<CoreType>` match arm for `Heif`.
    // (The mirror-to-core From impl is only emitted for input-side param types, not for
    // output-only enums like ImageOutputFormat — so exactly 2 occurrences is correct.)
    let cfg_count = lib_rs
        .lines()
        .filter(|l| l.contains(r#"#[cfg(feature = "heic")]"#))
        .count();
    assert_eq!(
        cfg_count, 2,
        "Expected exactly 2 occurrences of #[cfg(feature = \"heic\")]: \
         one in the mirror enum body + one in From<Core>. \
         Found {cfg_count}:\n{lib_rs}",
    );
}

#[test]
fn ungated_variants_are_present_without_cfg() {
    let lib_rs = generate_lib_rs(make_image_output_format_enum());

    // Native and Jpeg must be present.
    assert!(lib_rs.contains("Native"), "Native variant missing from lib.rs");
    assert!(lib_rs.contains("Jpeg"), "Jpeg variant missing from lib.rs");
}
