use super::*;

/// `#[cfg(test)]` associated functions (the canonical `HeuristicsConfig::test_config`
/// case) must never reach the binding surface, while normal and feature-gated methods
/// are retained.
#[test]
fn test_cfg_test_method_excluded_feature_method_retained() {
    let source = r#"
        pub struct HeuristicsConfig {
            pub threshold: u32,
        }

        impl HeuristicsConfig {
            pub fn new() -> Self {
                Self { threshold: 100 }
            }

            #[cfg(test)]
            pub fn test_config() -> Self {
                Self { threshold: 1 }
            }

            #[cfg(feature = "x")]
            pub fn feature_config() -> Self {
                Self { threshold: 50 }
            }
        }
    "#;

    let surface = extract_from_source(source);
    let config = surface
        .types
        .iter()
        .find(|t| t.name == "HeuristicsConfig")
        .expect("HeuristicsConfig should be extracted");

    let method_names: Vec<&str> = config.methods.iter().map(|m| m.name.as_str()).collect();

    assert!(
        !method_names.contains(&"test_config"),
        "#[cfg(test)] method must be excluded, got {method_names:?}"
    );
    assert!(
        method_names.contains(&"feature_config"),
        "#[cfg(feature = \"x\")] method must be retained, got {method_names:?}"
    );
}

/// A whole `#[cfg(test)]` impl block must be skipped, while its sibling normal impl
/// block is fully extracted.
#[test]
fn test_cfg_test_impl_block_excluded() {
    let source = r#"
        pub struct Widget {
            pub size: u32,
        }

        impl Widget {
            pub fn real_method(&self) -> u32 {
                self.size
            }
        }

        #[cfg(test)]
        impl Widget {
            pub fn fixture() -> Self {
                Self { size: 7 }
            }
        }
    "#;

    let surface = extract_from_source(source);
    let widget = surface
        .types
        .iter()
        .find(|t| t.name == "Widget")
        .expect("Widget should be extracted");

    let method_names: Vec<&str> = widget.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(
        method_names.contains(&"real_method"),
        "method from normal impl must be retained, got {method_names:?}"
    );
    assert!(
        !method_names.contains(&"fixture"),
        "method from #[cfg(test)] impl must be excluded, got {method_names:?}"
    );
}

/// Top-level `#[cfg(test)]` items (struct, enum, free function) are excluded while a
/// `#[cfg(feature = "x")]` item and a plain item are retained.
#[test]
fn test_cfg_test_top_level_items_excluded() {
    let source = r#"
        pub struct NormalType {
            pub value: u32,
        }

        #[cfg(test)]
        pub struct TestOnlyType {
            pub value: u32,
        }

        #[cfg(feature = "x")]
        pub struct FeatureType {
            pub value: u32,
        }

        pub fn normal_fn() -> u32 {
            1
        }

        #[cfg(test)]
        pub fn test_only_fn() -> u32 {
            2
        }

        #[cfg(all(test, feature = "x"))]
        pub fn nested_test_fn() -> u32 {
            3
        }

        #[cfg(not(test))]
        pub fn non_test_fn() -> u32 {
            4
        }
    "#;

    let surface = extract_from_source(source);

    let type_names: Vec<&str> = surface.types.iter().map(|t| t.name.as_str()).collect();
    assert!(type_names.contains(&"NormalType"), "got {type_names:?}");
    assert!(type_names.contains(&"FeatureType"), "got {type_names:?}");
    assert!(
        !type_names.contains(&"TestOnlyType"),
        "#[cfg(test)] struct must be excluded, got {type_names:?}"
    );

    let fn_names: Vec<&str> = surface.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(fn_names.contains(&"normal_fn"), "got {fn_names:?}");
    assert!(
        fn_names.contains(&"non_test_fn"),
        "#[cfg(not(test))] fn must be retained, got {fn_names:?}"
    );
    assert!(
        !fn_names.contains(&"test_only_fn"),
        "#[cfg(test)] fn must be excluded, got {fn_names:?}"
    );
    assert!(
        !fn_names.contains(&"nested_test_fn"),
        "#[cfg(all(test, ...))] fn must be excluded, got {fn_names:?}"
    );
}
