use std::fs;
use std::path::{Path, PathBuf};

const CACHE_DIR: &str = ".skif";

/// Hash a list of files + config to determine if extraction is needed.
pub fn compute_source_hash(sources: &[PathBuf], config_path: &Path) -> anyhow::Result<String> {
    let mut hasher = blake3::Hasher::new();
    for source in sources {
        let content = fs::read(source)?;
        hasher.update(&content);
    }
    let config_content = fs::read(config_path)?;
    hasher.update(&config_content);
    Ok(hasher.finalize().to_hex().to_string())
}

/// Check if cached IR is still valid.
pub fn is_ir_cached(source_hash: &str) -> bool {
    let hash_path = Path::new(CACHE_DIR).join("ir.hash");
    match fs::read_to_string(&hash_path) {
        Ok(cached) => cached.trim() == source_hash,
        Err(_) => false,
    }
}

/// Read cached IR.
pub fn read_cached_ir() -> anyhow::Result<skif_core::ir::ApiSurface> {
    let ir_path = Path::new(CACHE_DIR).join("ir.json");
    let content = fs::read_to_string(&ir_path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Write IR to cache.
pub fn write_ir_cache(api: &skif_core::ir::ApiSurface, source_hash: &str) -> anyhow::Result<()> {
    let cache_dir = Path::new(CACHE_DIR);
    fs::create_dir_all(cache_dir)?;
    fs::write(cache_dir.join("ir.json"), serde_json::to_string_pretty(api)?)?;
    fs::write(cache_dir.join("ir.hash"), source_hash)?;
    Ok(())
}

/// Compute hash for a language's output (IR + language-specific config).
pub fn compute_lang_hash(ir_json: &str, lang: &str, config_toml: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ir_json.as_bytes());
    hasher.update(lang.as_bytes());
    hasher.update(config_toml.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Check if a language's output is cached.
pub fn is_lang_cached(lang: &str, lang_hash: &str) -> bool {
    let hash_path = Path::new(CACHE_DIR).join("hashes").join(format!("{lang}.hash"));
    match fs::read_to_string(&hash_path) {
        Ok(cached) => cached.trim() == lang_hash,
        Err(_) => false,
    }
}

/// Write language hash.
pub fn write_lang_hash(lang: &str, lang_hash: &str) -> anyhow::Result<()> {
    let hashes_dir = Path::new(CACHE_DIR).join("hashes");
    fs::create_dir_all(&hashes_dir)?;
    fs::write(hashes_dir.join(format!("{lang}.hash")), lang_hash)?;
    Ok(())
}

/// Clear cache.
pub fn clear_cache() -> anyhow::Result<()> {
    let cache_dir = Path::new(CACHE_DIR);
    if cache_dir.exists() {
        fs::remove_dir_all(cache_dir)?;
    }
    Ok(())
}

/// Show cache status information.
pub fn show_status() {
    let cache_dir = Path::new(CACHE_DIR);
    if !cache_dir.exists() {
        println!("No cache directory.");
        return;
    }

    println!("Cache directory: .skif/");

    let ir_path = cache_dir.join("ir.json");
    if ir_path.exists() {
        if let Ok(meta) = fs::metadata(&ir_path) {
            println!("  ir.json: {} bytes", meta.len());
        }
    } else {
        println!("  ir.json: not cached");
    }

    let hashes_dir = cache_dir.join("hashes");
    if hashes_dir.exists() {
        if let Ok(entries) = fs::read_dir(&hashes_dir) {
            let langs: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str().map(String::from)))
                .collect();
            if langs.is_empty() {
                println!("  language hashes: none");
            } else {
                println!("  language hashes: {}", langs.join(", "));
            }
        }
    } else {
        println!("  language hashes: none");
    }
}
