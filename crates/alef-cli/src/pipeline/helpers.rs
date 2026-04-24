use anyhow::Context as _;
use tracing::info;

/// Run a shell command, logging and failing on non-zero exit.
pub(crate) fn run_command(cmd: &str) -> anyhow::Result<()> {
    info!("Running: {cmd}");
    let status = std::process::Command::new("sh").args(["-c", cmd]).status()?;
    if !status.success() {
        anyhow::bail!("Command failed: {cmd}");
    }
    Ok(())
}

/// Run a shell command, capturing stdout and stderr.
///
/// Returns the captured output on success.  On failure the error includes
/// the command string and captured stderr for diagnostics.
pub(crate) fn run_command_captured(cmd: &str) -> anyhow::Result<(String, String)> {
    info!("Running: {cmd}");
    let output = std::process::Command::new("sh")
        .args(["-c", cmd])
        .output()
        .with_context(|| format!("failed to spawn: {cmd}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        anyhow::bail!("Command failed: {cmd}\n{stderr}");
    }
    Ok((stdout, stderr))
}

/// Initialize a new alef.toml config file.
pub fn init(config_path: &std::path::Path, languages: Option<Vec<String>>) -> anyhow::Result<()> {
    // Read crate name and version from Cargo.toml
    let (crate_name, crate_version) = read_crate_metadata()?;

    // Use provided languages or default to ["python", "node", "ffi"]
    let langs = languages.unwrap_or_else(|| vec!["python".to_string(), "node".to_string(), "ffi".to_string()]);

    // Generate config content
    let config_content = generate_init_config(&crate_name, &crate_version, &langs);

    // Write to alef.toml
    std::fs::write(config_path, config_content)
        .with_context(|| format!("failed to write config to {}", config_path.display()))?;
    info!("Created {}", config_path.display());

    Ok(())
}

fn read_crate_metadata() -> anyhow::Result<(String, String)> {
    let content = std::fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?;
    let value: toml::Value = toml::from_str(&content).context("failed to parse Cargo.toml")?;

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
