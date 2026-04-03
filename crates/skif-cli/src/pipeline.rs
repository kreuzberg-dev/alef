use ahash::{AHashMap, AHashSet};
use skif_core::backend::GeneratedFile;
use skif_core::config::{Language, SkifConfig};
use skif_core::ir::{ApiSurface, TypeDef, TypeRef};
use std::collections::HashMap;
use std::path::Path;

use crate::cache;
use crate::registry;
use tracing::{debug, info};

/// Ensure required entries are in `.gitignore` — creates the file if absent.
/// Adds `.skif/` (cache) and language-specific build artifacts based on config.
pub fn ensure_gitignore(base_dir: &Path, config: &SkifConfig) {
    let gitignore_path = base_dir.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let existing_lines: AHashSet<&str> = existing.lines().map(str::trim).collect();

    let mut entries: Vec<&str> = vec![".skif/"];

    for lang in &config.languages {
        match lang {
            Language::Python => {
                entries.extend_from_slice(&["__pycache__/", "*.so", "*.pyd", ".venv/", "*.egg-info/", "dist/"])
            }
            Language::Node => entries.extend_from_slice(&["node_modules/", "*.node"]),
            Language::Ruby => entries.extend_from_slice(&[".gems/", "vendor/bundle/"]),
            Language::Php => entries.extend_from_slice(&["vendor/"]),
            Language::Ffi => entries.push("*.h.bak"),
            Language::Go => entries.push("*.test"),
            Language::Java => entries.extend_from_slice(&["target/", "*.class"]),
            Language::Csharp => entries.extend_from_slice(&["bin/", "obj/", "*.nupkg"]),
            Language::Wasm => entries.push("pkg/"),
            _ => {}
        }
    }

    let mut to_add = Vec::new();
    for entry in &entries {
        if !existing_lines.contains(entry) {
            to_add.push(*entry);
        }
    }

    if to_add.is_empty() {
        return;
    }

    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let additions = to_add.join("\n");
    let new_content = format!("{existing}{separator}{additions}\n");

    if let Err(e) = std::fs::write(&gitignore_path, new_content) {
        debug!("Could not update .gitignore: {e}");
    } else {
        debug!("Updated .gitignore with {} entries", to_add.len());
    }
}

/// Run extraction, with caching.
pub fn extract(config: &SkifConfig, config_path: &Path, clean: bool) -> anyhow::Result<ApiSurface> {
    // Ensure .gitignore has required entries
    if let Some(parent) = config_path.parent() {
        ensure_gitignore(parent, config);
    }

    let source_hash = cache::compute_source_hash(&config.crate_config.sources, config_path)?;

    if !clean && cache::is_ir_cached(&source_hash) {
        info!("Using cached IR");
        return cache::read_cached_ir();
    }

    info!("Extracting API surface from Rust source...");
    let sources: Vec<&Path> = config.crate_config.sources.iter().map(|p| p.as_path()).collect();

    // Read version from Cargo.toml
    let version = read_version(&config.crate_config.version_from)?;

    let workspace_root = config.crate_config.workspace_root.as_deref();
    let api = skif_extract::extractor::extract(&sources, &config.crate_config.name, &version, workspace_root)?;

    // Apply global filters (includes and excludes)
    let mut api = apply_filters(api, config);

    // Inject declared opaque types from config (external crate types skif can't extract)
    inject_declared_opaque_types(&mut api, config);

    // Replace references to types not in the API surface with String
    sanitize_unknown_types(&mut api);

    // Deduplicate types, enums, and functions by name
    dedup_api_surface(&mut api);

    // Apply path mappings to rewrite rust_path fields
    apply_path_mappings(&mut api, config);

    cache::write_ir_cache(&api, &source_hash)?;
    info!(
        "Extracted {} types, {} functions, {} enums",
        api.types.len(),
        api.functions.len(),
        api.enums.len()
    );

    Ok(api)
}

/// Generate bindings for given languages.
pub fn generate(
    api: &ApiSurface,
    config: &SkifConfig,
    languages: &[Language],
    clean: bool,
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    // Validate that Go/Java/C# have FFI in the languages list
    let has_ffi = languages.contains(&Language::Ffi);
    for &lang in languages {
        if (lang == Language::Go || lang == Language::Java || lang == Language::Csharp) && !has_ffi {
            tracing::warn!(
                "Language {:?} requires FFI to be in the languages list for proper code generation",
                lang
            );
        }
    }

    let ir_json = serde_json::to_string(api)?;
    let config_toml = toml::to_string(config).unwrap_or_default();
    let mut results = vec![];

    for &lang in languages {
        let lang_str = lang.to_string();
        let lang_hash = cache::compute_lang_hash(&ir_json, &lang_str, &config_toml);

        if !clean && cache::is_lang_cached(&lang_str, &lang_hash) {
            debug!("  {}: cached, skipping", lang_str);
            continue;
        }

        let backend = registry::get_backend(lang);
        info!("  {}: generating...", lang_str);

        let files = backend.generate_bindings(api, config)?;
        cache::write_lang_hash(&lang_str, &lang_hash)?;
        results.push((lang, files));
    }

    Ok(results)
}

/// Generate type stubs for given languages.
pub fn generate_stubs(
    api: &ApiSurface,
    config: &SkifConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let mut results = vec![];
    for &lang in languages {
        let backend = registry::get_backend(lang);
        let files = backend.generate_type_stubs(api, config)?;
        if !files.is_empty() {
            results.push((lang, files));
        }
    }
    Ok(results)
}

/// Write generated files to disk.
pub fn write_files(files: &[(Language, Vec<GeneratedFile>)], base_dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for (_lang, lang_files) in files {
        for file in lang_files {
            let full_path = base_dir.join(&file.path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, &file.content)?;
            count += 1;
            debug!("  wrote: {}", full_path.display());
        }
    }
    Ok(count)
}

/// Diff generated files against what's on disk.
pub fn diff_files(files: &[(Language, Vec<GeneratedFile>)], base_dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut diffs = vec![];
    for (lang, lang_files) in files {
        for file in lang_files {
            let full_path = base_dir.join(&file.path);
            let existing = std::fs::read_to_string(&full_path).unwrap_or_default();
            if existing != file.content {
                diffs.push(format!("[{lang}] {}", file.path.display()));
            }
        }
    }
    Ok(diffs)
}

fn read_version(version_from: &str) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(version_from)?;
    let value: toml::Value = toml::from_str(&content)?;
    if let Some(v) = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(v.to_string());
    }
    if let Some(v) = value
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(v.to_string());
    }
    anyhow::bail!("Could not find version in {version_from}")
}

/// Generate scaffold files for given languages.
pub fn scaffold(api: &ApiSurface, config: &SkifConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    skif_scaffold::scaffold(api, config, languages)
}

/// Generate README files for given languages.
pub fn readme(api: &ApiSurface, config: &SkifConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    skif_readme::generate_readmes(api, config, languages)
}

/// Write standalone generated files (not grouped by language) to disk.
pub fn write_scaffold_files(files: &[GeneratedFile], base_dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for file in files {
        let full_path = base_dir.join(&file.path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, &file.content)?;
        count += 1;
        debug!("  wrote: {}", full_path.display());
    }
    Ok(count)
}

/// Sync version from Cargo.toml to all package manifest files.
pub fn sync_versions(config: &SkifConfig) -> anyhow::Result<()> {
    let version = read_version(&config.crate_config.version_from)?;
    info!("Syncing version {version}");

    let mut updated = vec![];

    // Python: pyproject.toml
    if let Ok(content) = std::fs::read_to_string("packages/python/pyproject.toml") {
        if let Some(new_content) = replace_version_pattern(&content, r#"version = "[^"]*""#, &version) {
            std::fs::write("packages/python/pyproject.toml", &new_content)?;
            updated.push("packages/python/pyproject.toml".to_string());
        }
    }

    // Node: package.json
    if let Ok(content) = std::fs::read_to_string("packages/typescript/package.json") {
        if let Some(new_content) = replace_version_pattern(&content, r#""version": "[^"]*""#, &version) {
            std::fs::write("packages/typescript/package.json", &new_content)?;
            updated.push("packages/typescript/package.json".to_string());
        }
    }

    // Ruby: *.gemspec
    if let Ok(entries) = std::fs::read_dir("packages/ruby") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "gemspec") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(new_content) =
                        replace_version_pattern(&content, r#"spec\.version\s*=\s*"[^"]*""#, &version)
                    {
                        std::fs::write(&path, &new_content)?;
                        updated.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // PHP: composer.json
    if let Ok(content) = std::fs::read_to_string("packages/php/composer.json") {
        if let Some(new_content) = replace_version_pattern(&content, r#""version": "[^"]*""#, &version) {
            std::fs::write("packages/php/composer.json", &new_content)?;
            updated.push("packages/php/composer.json".to_string());
        }
    }

    // Elixir: mix.exs
    if let Ok(content) = std::fs::read_to_string("packages/elixir/mix.exs") {
        if let Some(new_content) = replace_version_pattern(&content, r#"version: "[^"]*""#, &version) {
            std::fs::write("packages/elixir/mix.exs", &new_content)?;
            updated.push("packages/elixir/mix.exs".to_string());
        }
    }

    // Go: go.mod (no version field, skip)

    // Java: pom.xml
    if let Ok(content) = std::fs::read_to_string("packages/java/pom.xml") {
        if let Some(new_content) = replace_version_pattern(&content, r#"<version>[^<]*</version>"#, &version) {
            std::fs::write("packages/java/pom.xml", &new_content)?;
            updated.push("packages/java/pom.xml".to_string());
        }
    }

    // C#: *.csproj
    if let Ok(entries) = std::fs::read_dir("packages/csharp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "csproj") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(new_content) =
                        replace_version_pattern(&content, r#"<Version>[^<]*</Version>"#, &version)
                    {
                        std::fs::write(&path, &new_content)?;
                        updated.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    for file in updated {
        info!("  Updated: {file}");
    }

    Ok(())
}

/// Replace version pattern in content. Returns Some(new_content) if replaced, None if pattern not found.
fn replace_version_pattern(content: &str, pattern: &str, version: &str) -> Option<String> {
    let regex = regex::Regex::new(pattern).ok()?;
    if !regex.is_match(content) {
        return None;
    }

    let replacement = match pattern {
        p if p.contains("version =") && !p.contains("spec") => format!(r#"version = "{version}""#),
        p if p.contains("\"version\"") && p.contains("\"") => format!(r#""version": "{version}""#),
        p if p.contains("spec") => format!(r#"spec.version = "{version}""#),
        p if p.contains("<version>") => format!("<version>{version}</version>"),
        p if p.contains("<Version>") => format!("<Version>{version}</Version>"),
        p if p.contains("version:") => format!(r#"version: "{version}""#),
        _ => return None,
    };

    Some(regex.replace(content, replacement.as_str()).to_string())
}

/// Run configured lint/format commands on generated output.
pub fn lint(config: &SkifConfig, languages: &[Language]) -> anyhow::Result<()> {
    let lint_config = config.lint.as_ref();
    for lang in languages {
        let lang_str = lang.to_string();
        if let Some(lint_map) = lint_config {
            if let Some(lang_lint) = lint_map.get(&lang_str) {
                // Run format command if configured
                if let Some(fmt_cmd) = &lang_lint.format {
                    run_command(fmt_cmd)?;
                }
                // Run check command if configured
                if let Some(check_cmd) = &lang_lint.check {
                    run_command(check_cmd)?;
                }
                // Run typecheck command if configured
                if let Some(typecheck_cmd) = &lang_lint.typecheck {
                    run_command(typecheck_cmd)?;
                }
            }
        }
    }
    Ok(())
}

fn run_command(cmd: &str) -> anyhow::Result<()> {
    info!("Running: {cmd}");
    let status = std::process::Command::new("sh").args(["-c", cmd]).status()?;
    if !status.success() {
        anyhow::bail!("Command failed: {cmd}");
    }
    Ok(())
}

/// Initialize a new skif.toml config file.
pub fn init(config_path: &std::path::Path, languages: Option<Vec<String>>) -> anyhow::Result<()> {
    // Read crate name and version from Cargo.toml
    let (crate_name, crate_version) = read_crate_metadata()?;

    // Use provided languages or default to ["python", "node", "ffi"]
    let langs = languages.unwrap_or_else(|| vec!["python".to_string(), "node".to_string(), "ffi".to_string()]);

    // Generate config content
    let config_content = generate_init_config(&crate_name, &crate_version, &langs);

    // Write to skif.toml
    std::fs::write(config_path, config_content)?;
    info!("Created {}", config_path.display());

    Ok(())
}

fn read_crate_metadata() -> anyhow::Result<(String, String)> {
    let content = std::fs::read_to_string("Cargo.toml")?;
    let value: toml::Value = toml::from_str(&content)?;

    // Try workspace.package first
    if let Some(name) = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
    {
        if let Some(version) = value
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
        {
            return Ok((name.to_string(), version.to_string()));
        }
    }

    // Try package directly
    if let Some(name) = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
    {
        if let Some(version) = value
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
        {
            return Ok((name.to_string(), version.to_string()));
        }
    }

    anyhow::bail!("Could not find package name and version in Cargo.toml")
}

fn generate_init_config(crate_name: &str, _crate_version: &str, languages: &[String]) -> String {
    let source_path = format!("crates/{}/src/lib.rs", crate_name);

    let mut config = String::from("languages = [");

    for (i, lang) in languages.iter().enumerate() {
        if i > 0 {
            config.push_str(", ");
        }
        config.push('"');
        config.push_str(lang);
        config.push('"');
    }
    config.push_str("]\n\n");

    config.push_str(&format!(
        "[crate]\nname = \"{}\"\nsources = [\"{}\"]\nversion_from = \"Cargo.toml\"\n",
        crate_name, source_path
    ));

    // Add language-specific configs
    if languages.contains(&"python".to_string()) {
        config.push_str(&format!(
            "\n[python]\nmodule_name = \"_{}\"\n",
            crate_name.replace('-', "_")
        ));
    }

    if languages.contains(&"node".to_string()) {
        config.push_str(&format!("\n[node]\npackage_name = \"{crate_name}\"\n"));
    }

    if languages.contains(&"ffi".to_string()) {
        config.push_str(&format!("\n[ffi]\nprefix = \"{}\"\n", crate_name.replace('-', "_")));
    }

    if languages.contains(&"go".to_string()) {
        config.push_str(&format!(
            "\n[go]\nmodule = \"github.com/kreuzberg-dev/{}\"\n",
            crate_name
        ));
    }

    if languages.contains(&"ruby".to_string()) {
        config.push_str(&format!("\n[ruby]\ngem_name = \"{}\"\n", crate_name.replace('-', "_")));
    }

    if languages.contains(&"java".to_string()) {
        config.push_str("\n[java]\npackage = \"dev.kreuzberg\"\n");
    }

    if languages.contains(&"csharp".to_string()) {
        config.push_str(&format!("\n[csharp]\nnamespace = \"{}\"\n", to_pascal_case(crate_name)));
    }

    config
}

fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

/// Inject declared opaque types from config into the API surface.
/// These are external crate types that skif can't extract but needs to generate wrappers for.
fn inject_declared_opaque_types(api: &mut ApiSurface, config: &SkifConfig) {
    for (name, rust_path) in &config.opaque_types {
        // Only add if not already in the API surface
        if !api.types.iter().any(|t| t.name == *name) && !api.enums.iter().any(|e| e.name == *name) {
            api.types.push(skif_core::ir::TypeDef {
                name: name.clone(),
                rust_path: rust_path.clone(),
                fields: vec![],
                methods: vec![],
                is_opaque: true,
                is_clone: false,
                doc: String::new(),
                cfg: None,
                is_trait: false,
            });
            debug!("Injected declared opaque type: {name} -> {rust_path}");
        }
    }
}

/// Replace `TypeRef::Named(name)` references that don't exist in the API surface
/// with `TypeRef::String`. This handles trait objects, generic bounds, and other types
/// that were extracted but filtered out or never existed as concrete types.
fn sanitize_unknown_types(api: &mut ApiSurface) {
    let known_types: AHashSet<String> = api.types.iter().map(|t| t.name.clone()).collect();
    let known_enums: AHashSet<String> = api.enums.iter().map(|e| e.name.clone()).collect();

    for typ in &mut api.types {
        for field in &mut typ.fields {
            if sanitize_type_ref(&mut field.ty, &known_types, &known_enums) {
                field.sanitized = true;
            }
        }
        let type_name = typ.name.clone();
        for method in &mut typ.methods {
            let mut method_sanitized = false;
            for param in &mut method.params {
                if sanitize_type_ref(&mut param.ty, &known_types, &known_enums) {
                    param.sanitized = true;
                    method_sanitized = true;
                }
            }
            // Skip sanitizing return type if it's Named(parent_type) — builder/factory pattern.
            // Methods that return their own type (e.g. with_foo(&self) -> Self) should keep
            // the Named return so codegen can delegate them correctly.
            let is_self_return = matches!(&method.return_type, TypeRef::Named(n) if n == &type_name);
            if !is_self_return && sanitize_type_ref(&mut method.return_type, &known_types, &known_enums) {
                method_sanitized = true;
            }
            if method_sanitized {
                method.sanitized = true;
            }
        }
    }
    for func in &mut api.functions {
        let mut func_sanitized = false;
        for param in &mut func.params {
            if sanitize_type_ref(&mut param.ty, &known_types, &known_enums) {
                param.sanitized = true;
                func_sanitized = true;
            }
        }
        if sanitize_type_ref(&mut func.return_type, &known_types, &known_enums) {
            func_sanitized = true;
        }
        if func_sanitized {
            func.sanitized = true;
        }
    }
}

/// Returns true if the type was sanitized (changed from original).
fn sanitize_type_ref(ty: &mut TypeRef, known_types: &AHashSet<String>, known_enums: &AHashSet<String>) -> bool {
    match ty {
        TypeRef::Named(name) => {
            if !known_types.contains(name.as_str()) && !known_enums.contains(name.as_str()) {
                *ty = TypeRef::String;
                true
            } else {
                false
            }
        }
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => sanitize_type_ref(inner, known_types, known_enums),
        TypeRef::Map(k, v) => {
            let a = sanitize_type_ref(k, known_types, known_enums);
            let b = sanitize_type_ref(v, known_types, known_enums);
            a || b
        }
        _ => false,
    }
}

/// Deduplicate API surface items by name to prevent conflicting definitions.
/// This resolves:
/// 1. Type-enum collisions: If a name exists in both types and enums, keep only the enum
/// 2. Duplicate types: Keep only the first occurrence of each type name
/// 3. Duplicate enums: Keep only the first occurrence of each enum name
/// 4. Duplicate functions: Keep only the first occurrence of each function name
fn dedup_api_surface(api: &mut ApiSurface) {
    // Remove types that collide with enums (enums win)
    let enum_names: AHashSet<String> = api.enums.iter().map(|e| e.name.clone()).collect();
    api.types.retain(|t| !enum_names.contains(&t.name));

    // Dedup types by name (keep first)
    let mut seen_types: AHashSet<String> = AHashSet::new();
    api.types.retain(|t| seen_types.insert(t.name.clone()));

    // Dedup enums by name (keep first)
    let mut seen_enums: AHashSet<String> = AHashSet::new();
    api.enums.retain(|e| seen_enums.insert(e.name.clone()));

    // Dedup functions by name (keep first)
    let mut seen_fns: AHashSet<String> = AHashSet::new();
    api.functions.retain(|f| seen_fns.insert(f.name.clone()));
}

fn apply_filters(mut api: ApiSurface, config: &SkifConfig) -> ApiSurface {
    let exclude = &config.exclude;
    let include = &config.include;

    // Apply includes first (whitelist), expanding to transitively referenced types
    if !include.types.is_empty() {
        let expanded = expand_include_list(&api, &include.types);
        api.types.retain(|t| expanded.contains(&t.name));
        api.enums.retain(|e| expanded.contains(&e.name));
        api.errors.retain(|e| expanded.contains(&e.name));
    }
    if !include.functions.is_empty() {
        api.functions.retain(|f| include.functions.contains(&f.name));
    }

    // Then apply excludes (blacklist)
    api.types.retain(|t| !exclude.types.contains(&t.name));
    api.functions.retain(|f| !exclude.functions.contains(&f.name));
    api.enums.retain(|e| !exclude.types.contains(&e.name));
    api.errors.retain(|e| !exclude.types.contains(&e.name));

    // Apply method-level excludes: "TypeName.method_name"
    if !exclude.methods.is_empty() {
        for typ in &mut api.types {
            typ.methods.retain(|m| {
                let key = format!("{}.{}", typ.name, m.name);
                !exclude.methods.contains(&key)
            });
        }
    }

    api
}

/// Expand the include list by transitively discovering all types referenced by fields,
/// method parameters, and return types of the included types.
fn expand_include_list(api: &ApiSurface, include_types: &[String]) -> AHashSet<String> {
    let mut needed: AHashSet<String> = include_types.iter().cloned().collect();
    let mut changed = true;

    // Build a map of all available types for lookup
    let all_types: AHashMap<String, &TypeDef> = api.types.iter().map(|t| (t.name.clone(), t)).collect();
    let all_enums: AHashSet<String> = api.enums.iter().map(|e| e.name.clone()).collect();

    while changed {
        changed = false;
        let current: Vec<String> = needed.iter().cloned().collect();
        for type_name in &current {
            if let Some(typ) = all_types.get(type_name) {
                for field in &typ.fields {
                    collect_named_types(&field.ty, &mut needed, &all_types, &all_enums, &mut changed);
                }
                for method in &typ.methods {
                    collect_named_types(&method.return_type, &mut needed, &all_types, &all_enums, &mut changed);
                    for param in &method.params {
                        collect_named_types(&param.ty, &mut needed, &all_types, &all_enums, &mut changed);
                    }
                }
            }
        }
    }
    needed
}

/// Recursively collect all named type references from a TypeRef into the needed set.
fn collect_named_types(
    ty: &TypeRef,
    needed: &mut AHashSet<String>,
    all_types: &AHashMap<String, &TypeDef>,
    all_enums: &AHashSet<String>,
    changed: &mut bool,
) {
    match ty {
        TypeRef::Named(name) => {
            if (all_types.contains_key(name) || all_enums.contains(name)) && needed.insert(name.clone()) {
                *changed = true;
            }
        }
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => {
            collect_named_types(inner, needed, all_types, all_enums, changed);
        }
        TypeRef::Map(k, v) => {
            collect_named_types(k, needed, all_types, all_enums, changed);
            collect_named_types(v, needed, all_types, all_enums, changed);
        }
        _ => {}
    }
}

/// Rewrite a rust_path using path_mappings.
/// Matches the longest prefix first.
fn rewrite_path(path: &str, mappings: &HashMap<String, String>) -> String {
    let mut sorted: Vec<_> = mappings.iter().collect();
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (from, to) in sorted {
        if path.starts_with(from.as_str()) {
            return format!("{}{}", to, &path[from.len()..]);
        }
    }
    path.to_string()
}

/// Apply path_mappings to rewrite all rust_path fields in the API surface.
fn apply_path_mappings(api: &mut ApiSurface, config: &SkifConfig) {
    if config.crate_config.path_mappings.is_empty() {
        return;
    }
    for typ in &mut api.types {
        typ.rust_path = rewrite_path(&typ.rust_path, &config.crate_config.path_mappings);
    }
    for func in &mut api.functions {
        func.rust_path = rewrite_path(&func.rust_path, &config.crate_config.path_mappings);
    }
    for enum_def in &mut api.enums {
        enum_def.rust_path = rewrite_path(&enum_def.rust_path, &config.crate_config.path_mappings);
    }
    for error_def in &mut api.errors {
        error_def.rust_path = rewrite_path(&error_def.rust_path, &config.crate_config.path_mappings);
    }
}
