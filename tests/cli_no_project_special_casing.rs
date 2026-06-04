use std::path::{Path, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

const SNAPSHOT_FORBIDDEN_MARKERS: &[&str] = &[
    "kreuzberg",
    "kreuzberglib",
    "literllmclient",
    "liter-llm",
    "spikard",
    "kreuzcrawl",
    "html-to-markdown",
];

const SNAPSHOT_FORBIDDEN_DOMAIN_TYPES: &[&str] = &[
    "InternalDocument",
    "ExtractionConfig",
    "ExtractionResult",
    "EmbeddingConfig",
    "ChunkingConfig",
    "BatchBytesItem",
    "BatchFileItem",
    "ConversionOptions",
    "ConversionResult",
    "HtmlVisitor",
    "IHtmlVisitor",
    "OcrBackend",
    "VisitorHandle",
];

#[test]
fn no_project_name_special_casing_in_enforced_files() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hook = workspace_root.join("hooks/check_project_mentions.py");
    let files = enforced_files(&workspace_root);

    let output = Command::new("python3")
        .arg(hook)
        .args(&files)
        .output()
        .expect("project mention hook must run");

    assert!(
        output.status.success(),
        "project mention hook failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn strict_hook_scans_snapshots_docs_and_generated_guidance() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let hook = workspace_root.join("hooks/check_project_mentions.py");
    let files = strict_enforced_files(&workspace_root);

    let output = Command::new("python3")
        .arg(hook)
        .arg("--strict")
        .args(&files)
        .output()
        .expect("strict project mention hook must run");

    assert!(
        output.status.success(),
        "strict project mention hook failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn no_downstream_markers_in_snapshot_filenames() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshot_root = workspace_root.join("tests/snapshots");
    let leaks: Vec<String> = WalkDir::new(&snapshot_root)
        .into_iter()
        .filter_map(Result::ok)
        .map(walkdir::DirEntry::into_path)
        .filter(|entry_path| entry_path.is_file())
        .filter_map(|entry_path| {
            let display_path = entry_path
                .strip_prefix(&snapshot_root)
                .unwrap_or(&entry_path)
                .display()
                .to_string();
            let normalized = display_path.to_lowercase().replace(['_', '/'], "-");
            let has_project_marker = SNAPSHOT_FORBIDDEN_MARKERS
                .iter()
                .any(|marker| normalized.contains(marker));
            let has_domain_marker = SNAPSHOT_FORBIDDEN_DOMAIN_TYPES.iter().any(|marker| {
                let normalized_marker = marker.to_lowercase().replace('_', "-");
                normalized.contains(&normalized_marker)
            });
            (has_project_marker || has_domain_marker).then_some(display_path)
        })
        .collect();

    assert!(
        leaks.is_empty(),
        "snapshot filenames must use neutral fixture names:\n{}",
        leaks.join("\n")
    );
}

#[test]
fn no_downstream_markers_in_snapshot_content() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshot_root = workspace_root.join("tests/snapshots");
    let mut leaks = Vec::new();
    for entry_path in WalkDir::new(snapshot_root)
        .into_iter()
        .filter_map(Result::ok)
        .map(walkdir::DirEntry::into_path)
        .filter(|entry_path| entry_path.is_file())
    {
        let Ok(content) = std::fs::read_to_string(&entry_path) else {
            continue;
        };
        let normalized = content.to_lowercase().replace('_', "-");
        for marker in SNAPSHOT_FORBIDDEN_MARKERS {
            if normalized.contains(marker) {
                leaks.push(format!("{} contains {marker}", entry_path.display()));
            }
        }
        for marker in SNAPSHOT_FORBIDDEN_DOMAIN_TYPES {
            if content.contains(marker) {
                leaks.push(format!("{} contains {marker}", entry_path.display()));
            }
        }
    }

    assert!(
        leaks.is_empty(),
        "snapshot content must use neutral fixture names:\n{}",
        leaks.join("\n")
    );
}

fn enforced_files(workspace_root: &Path) -> Vec<PathBuf> {
    [
        "src",
        "tests",
        "alef.toml",
        "Cargo.toml",
        ".pre-commit-config.yaml",
        ".pre-commit-hooks.yaml",
        "hooks",
    ]
    .into_iter()
    .flat_map(|relative| {
        let path = workspace_root.join(relative);
        if path.is_file() {
            vec![path]
        } else {
            WalkDir::new(path)
                .into_iter()
                .filter_map(Result::ok)
                .map(walkdir::DirEntry::into_path)
                .filter(|entry_path| entry_path.is_file())
                .collect()
        }
    })
    .collect()
}

fn strict_enforced_files(workspace_root: &Path) -> Vec<PathBuf> {
    [
        "tests/snapshots",
        "docs",
        ".ai-rulez",
        "AGENTS.md",
        ".pre-commit-config.yaml",
        ".pre-commit-hooks.yaml",
    ]
    .into_iter()
    .flat_map(|relative| {
        let path = workspace_root.join(relative);
        if path.is_file() {
            vec![path]
        } else {
            WalkDir::new(path)
                .into_iter()
                .filter_map(Result::ok)
                .map(walkdir::DirEntry::into_path)
                .filter(|entry_path| entry_path.is_file())
                .collect()
        }
    })
    .collect()
}
