//! C# e2e project and shared test setup rendering.

use crate::core::hash::{self, CommentStyle};
use crate::core::template_versions as tv;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

pub(super) fn render_csproj(
    pkg_name: &str,
    pkg_path: &str,
    pkg_version: &str,
    dep_mode: crate::e2e::config::DependencyMode,
) -> String {
    let pkg_ref = match dep_mode {
        crate::e2e::config::DependencyMode::Registry => {
            format!("    <PackageReference Include=\"{pkg_name}\" Version=\"{pkg_version}\" />")
        }
        crate::e2e::config::DependencyMode::Local => {
            format!("    <ProjectReference Include=\"{pkg_path}\" />")
        }
    };
    crate::e2e::template_env::render(
        "csharp/csproj.jinja",
        minijinja::context! {
            pkg_ref => pkg_ref,
            namespace => pkg_name,
            microsoft_net_test_sdk_version => tv::nuget::MICROSOFT_NET_TEST_SDK,
            xunit_version => tv::nuget::XUNIT,
            xunit_runner_version => tv::nuget::XUNIT_RUNNER_VISUALSTUDIO,
        },
    )
}

pub(super) fn render_test_setup(
    needs_mock_server: bool,
    test_documents_dir: &str,
    namespace: &str,
    env: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    out.push_str(&hash::header(CommentStyle::DoubleSlash));
    out.push_str("using System;\n");
    out.push_str("using System.IO;\n");
    if needs_mock_server {
        out.push_str("using System.Diagnostics;\n");
    }
    out.push_str("using System.Runtime.CompilerServices;\n\n");
    let _ = writeln!(out, "namespace {namespace};\n");
    out.push_str("internal static class TestSetup\n");
    out.push_str("{\n");
    if needs_mock_server {
        out.push_str("    private static Process? _mockServer;\n\n");
    }
    out.push_str("    [ModuleInitializer]\n");
    out.push_str("    internal static void Init()\n");
    out.push_str("    {\n");

    // Emit env vars if present
    if !env.is_empty() {
        let mut sorted_keys: Vec<_> = env.keys().collect();
        sorted_keys.sort();
        for key in sorted_keys {
            let value = &env[key];
            let _ = writeln!(
                out,
                "        if (Environment.GetEnvironmentVariable(\"{key}\") == null) {{"
            );
            let _ = writeln!(
                out,
                "            Environment.SetEnvironmentVariable(\"{key}\", \"{value}\");"
            );
            out.push_str("        }\n");
        }
        out.push('\n');
    }

    let _ = writeln!(
        out,
        "        // Walk up from the assembly directory until we find the repo root."
    );
    let _ = writeln!(
        out,
        "        // Prefer a sibling {test_documents_dir}/ directory (chdir into it so that"
    );
    out.push_str("        // fixture paths like \"docx/fake.docx\" resolve relative to it). If that\n");
    out.push_str("        // is absent (projects with no document fixtures), fall\n");
    out.push_str("        // back to a sibling alef.toml or fixtures/ marker as the repo root.\n");
    out.push_str("        var dir = new DirectoryInfo(AppContext.BaseDirectory);\n");
    out.push_str("        DirectoryInfo? repoRoot = null;\n");
    out.push_str("        while (dir != null)\n");
    out.push_str("        {\n");
    let _ = writeln!(
        out,
        "            var documentsCandidate = Path.Combine(dir.FullName, \"{test_documents_dir}\");"
    );
    out.push_str("            if (Directory.Exists(documentsCandidate))\n");
    out.push_str("            {\n");
    out.push_str("                repoRoot = dir;\n");
    out.push_str("                Directory.SetCurrentDirectory(documentsCandidate);\n");
    out.push_str("                break;\n");
    out.push_str("            }\n");
    out.push_str("            if (File.Exists(Path.Combine(dir.FullName, \"alef.toml\"))\n");
    out.push_str("                || Directory.Exists(Path.Combine(dir.FullName, \"fixtures\")))\n");
    out.push_str("            {\n");
    out.push_str("                repoRoot = dir;\n");
    out.push_str("                break;\n");
    out.push_str("            }\n");
    out.push_str("            dir = dir.Parent;\n");
    out.push_str("        }\n");
    if needs_mock_server {
        out.push('\n');
        let mock_server_code =
            crate::e2e::template_env::render("csharp/test_setup_mock_server.cs.jinja", minijinja::context! {});
        out.push_str(&mock_server_code);
    }
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_test_setup_with_env_vars() {
        let mut env = HashMap::new();
        env.insert("ZEBRA_VAR".to_string(), "z_value".to_string());
        env.insert("ALPHA_VAR".to_string(), "a_value".to_string());
        env.insert("BETA_VAR".to_string(), "b_value".to_string());

        let output = render_test_setup(false, "fixtures", "FixtureE2E", &env);

        assert!(output.contains("ALPHA_VAR"));
        assert!(output.contains("a_value"));
        assert!(output.contains("BETA_VAR"));
        assert!(output.contains("b_value"));
        assert!(output.contains("ZEBRA_VAR"));
        assert!(output.contains("z_value"));

        // Verify alphabetical order
        let alpha_pos = output.find("ALPHA_VAR").unwrap();
        let beta_pos = output.find("BETA_VAR").unwrap();
        let zebra_pos = output.find("ZEBRA_VAR").unwrap();
        assert!(alpha_pos < beta_pos && beta_pos < zebra_pos);

        // Verify SetEnvironmentVariable pattern
        assert!(output.contains("Environment.SetEnvironmentVariable("));
    }

    #[test]
    fn test_render_test_setup_empty_env() {
        let env = HashMap::new();
        let output = render_test_setup(false, "fixtures", "FixtureE2E", &env);

        // Should not contain SetEnvironmentVariable calls for empty env
        assert!(!output.contains("Environment.SetEnvironmentVariable("));
    }

    #[test]
    fn test_render_test_setup_env_null_check() {
        let mut env = HashMap::new();
        env.insert("TEST_VAR".to_string(), "test_value".to_string());

        let output = render_test_setup(false, "fixtures", "FixtureE2E", &env);

        // Verify null-check pattern: if null, set
        assert!(output.contains("if (Environment.GetEnvironmentVariable(\"TEST_VAR\") == null)"));
        assert!(output.contains("Environment.SetEnvironmentVariable(\"TEST_VAR\", \"test_value\");"));
    }
}
