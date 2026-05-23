//! `.gitkeep` placeholder files inside `src/main/jniLibs/<abi>/` so the
//! AAR build picks up the directory layout even before any `.so` is copied
//! in. The release pipeline writes the real `lib<crate>.so` files here.

use std::path::Path;

use crate::core::backend::GeneratedFile;
use crate::core::config::ResolvedCrateConfig;

use crate::backends::kotlin_android::naming::abis;

/// Emit one `.gitkeep` per ABI directory.
///
/// The file content is empty. The canonical pre-commit `end-of-file-fixer`
/// hook truncates whitespace-only files (including a lone `"\n"`) to zero
/// bytes and leaves empty files alone, so emitting `String::new()` is the
/// stable resolution. Emitting `"\n"` triggers an infinite ping-pong
/// between alef regen (writes `"\n"`) and prek autofix (truncates to `""`).
pub fn emit(config: &ResolvedCrateConfig, aar_root: &Path) -> Vec<GeneratedFile> {
    abis(config)
        .into_iter()
        .map(|abi| GeneratedFile {
            path: aar_root.join("src/main/jniLibs").join(abi).join(".gitkeep"),
            content: String::new(),
            generated_header: false,
        })
        .collect()
}
