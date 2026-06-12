use super::*;

#[test]
fn test_pub_use_self_super_skipped() {
    let source = r#"
        pub use self::inner::Helper;
        pub use super::other::Thing;
        pub use crate::root::Item;

        pub mod inner {
            pub struct Helper {
                pub value: u32,
            }
        }
    "#;

    let surface = extract_from_source(source);
    // self/super/crate use paths are skipped (handled by mod resolution)
    // The inline module should still be extracted
    assert_eq!(surface.types.len(), 1);
    assert_eq!(surface.types[0].name, "Helper");
}

#[test]
fn test_collect_use_names_single() {
    let tree: syn::UseTree = syn::parse_str("Foo").unwrap();
    match collect_use_names(&tree) {
        UseFilter::Names(names) => assert_eq!(names, vec!["Foo"]),
        UseFilter::All => panic!("expected Names"),
    }
}

#[test]
fn test_collect_use_names_group() {
    let tree: syn::UseTree = syn::parse_str("{Foo, Bar, Baz}").unwrap();
    match collect_use_names(&tree) {
        UseFilter::Names(names) => {
            assert_eq!(names.len(), 3);
            assert!(names.contains(&"Foo".to_string()));
            assert!(names.contains(&"Bar".to_string()));
            assert!(names.contains(&"Baz".to_string()));
        }
        UseFilter::All => panic!("expected Names"),
    }
}

#[test]
fn test_collect_use_names_glob() {
    let tree: syn::UseTree = syn::parse_str("*").unwrap();
    assert!(matches!(collect_use_names(&tree), UseFilter::All));
}

#[test]
fn test_collect_use_names_rename() {
    // `use Foo as Bar` should return the alias name "Bar"
    let tree: syn::UseTree = syn::parse_str("Foo as Bar").unwrap();
    match super::reexports::collect_use_names(&tree) {
        super::reexports::UseFilter::Names(names) => {
            assert_eq!(names, vec!["Bar"]);
        }
        super::reexports::UseFilter::All => panic!("expected Names"),
    }
}

#[test]
fn test_collect_use_names_nested_path() {
    // `some::module::Type` — the leaf is Type
    let tree: syn::UseTree = syn::parse_str("some::module::Type").unwrap();
    match super::reexports::collect_use_names(&tree) {
        super::reexports::UseFilter::Names(names) => {
            assert_eq!(names, vec!["Type"]);
        }
        super::reexports::UseFilter::All => panic!("expected Names"),
    }
}

#[test]
fn test_collect_use_names_group_with_glob_returns_all() {
    // `{Foo, *}` — a group containing a glob means All
    let tree: syn::UseTree = syn::parse_str("{Foo, *}").unwrap();
    assert!(matches!(
        super::reexports::collect_use_names(&tree),
        super::reexports::UseFilter::All
    ));
}

#[test]
fn test_resolve_use_tree_group_variant() {
    // `pub use self::inner::{Foo};` — group variant of UseTree going through resolve_use_tree
    // Since these are self-references, they should be skipped without error.
    let source = r#"
        pub use self::{inner::Foo};

        pub mod inner {
            pub struct Foo { pub val: u32 }
        }
    "#;

    // Should not panic, and the inline module is still extracted
    let surface = extract_from_source(source);
    assert_eq!(surface.types.len(), 1);
    assert_eq!(surface.types[0].name, "Foo");
}
