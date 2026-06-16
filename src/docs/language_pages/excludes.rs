use crate::core::config::{Language, ResolvedCrateConfig};
use std::collections::HashSet;

pub(super) fn language_excludes(config: &ResolvedCrateConfig, lang: Language) -> (HashSet<String>, HashSet<String>) {
    let mut functions: HashSet<String> = config.exclude.functions.iter().cloned().collect();
    let mut types: HashSet<String> = config.exclude.types.iter().cloned().collect();

    match lang {
        Language::Python => {
            if let Some(c) = &config.python {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Node => {
            if let Some(c) = &config.node {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Ruby => {
            if let Some(c) = &config.ruby {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Php => {
            if let Some(c) = &config.php {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Elixir => {
            if let Some(c) = &config.elixir {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Wasm => {
            if let Some(c) = &config.wasm {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Ffi | Language::C => {
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Go => {
            if let Some(c) = &config.go {
                types.extend(c.exclude_types.iter().cloned());
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Java => {
            if let Some(c) = &config.java {
                types.extend(c.exclude_types.iter().cloned());
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Kotlin => {
            if let Some(c) = &config.kotlin {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::KotlinAndroid => {
            if let Some(c) = &config.kotlin_android {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Jni => {
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Swift => {
            if let Some(c) = &config.swift {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Dart => {
            if let Some(c) = &config.dart {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Gleam => {
            if let Some(c) = &config.gleam {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Csharp => {
            if let Some(c) = &config.csharp {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
            if let Some(c) = &config.ffi {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::Zig => {
            if let Some(c) = &config.zig {
                extend_excludes(&mut functions, &mut types, &c.exclude_functions, &c.exclude_types);
            }
        }
        Language::R | Language::Rust => {}
    }

    (functions, types)
}

pub(super) fn extend_excludes(
    functions: &mut HashSet<String>,
    types: &mut HashSet<String>,
    exclude_functions: &[String],
    exclude_types: &[String],
) {
    functions.extend(exclude_functions.iter().cloned());
    types.extend(exclude_types.iter().cloned());
}
