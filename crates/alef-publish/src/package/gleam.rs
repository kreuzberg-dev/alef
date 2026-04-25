//! Gleam Hex package — archives the compiled Rustler NIF for distribution.

use super::PackageArtifact;
use crate::platform::RustTarget;
use alef_core::config::AlefConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Package Gleam Rustler bindings into a Hex tarball.
///
/// Produces: `{name}-{version}.tar` containing the source tree + compiled Rustler NIF,
/// ready for `gleam hex publish`.
pub fn package_gleam(
    config: &AlefConfig,
    _target: &RustTarget,
    workspace_root: &Path,
    output_dir: &Path,
    version: &str,
) -> Result<PackageArtifact> {
    let crate_name = &config.crate_config.name;
    let pkg_dir = config.package_dir(alef_core::config::extras::Language::Gleam);

    let pkg_name = format!("{crate_name}-{version}");
    let staging = output_dir.join(&pkg_name);

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    // Copy the entire gleam package directory into staging.
    let pkg_src = workspace_root.join(&pkg_dir);
    if !pkg_src.exists() {
        anyhow::bail!("Gleam package directory not found: {}", pkg_dir);
    }

    // Copy all files from the Gleam package.
    copy_dir_recursive(&pkg_src, &staging).context("copying Gleam package directory")?;

    // Create tarball (Hex expects .tar, not .tar.gz).
    let archive_name = format!("{pkg_name}.tar");
    let archive_path = output_dir.join(&archive_name);

    let status = std::process::Command::new("tar")
        .arg("cf")
        .arg(&archive_path)
        .arg("-C")
        .arg(output_dir)
        .arg(&pkg_name)
        .status()
        .context("creating Gleam tarball")?;

    if !status.success() {
        anyhow::bail!("tar failed with exit code {}", status.code().unwrap_or(-1));
    }

    // Clean up staging.
    fs::remove_dir_all(&staging).ok();

    Ok(PackageArtifact {
        path: archive_path,
        name: archive_name,
        checksum: None,
    })
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).context("reading source directory")? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(&file_name);

        if path.is_dir() {
            // Skip hidden directories and common ignore patterns.
            if file_name.to_string_lossy().starts_with('.') {
                continue;
            }
            if matches!(
                file_name.to_string_lossy().as_ref(),
                "target" | "node_modules" | "__pycache__" | ".git"
            ) {
                continue;
            }
            fs::create_dir_all(&dest_path)?;
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}
