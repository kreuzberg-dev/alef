use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn hook_path() -> PathBuf {
    repo_root().join("hooks/check_backend_naming_helpers.py")
}

fn run_hook(cwd: &Path, files: &[&str]) -> Output {
    let mut command = Command::new("python3");
    command.current_dir(cwd);
    command.arg(hook_path());
    for file in files {
        command.arg(file);
    }
    command.output().expect("hook command must run")
}

#[test]
fn rejects_backend_local_generic_naming_helpers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend_dir = dir.path().join("src/backends/node");
    fs::create_dir_all(&backend_dir).expect("create backend dir");
    fs::write(
        backend_dir.join("helpers.rs"),
        "pub(crate) fn to_snake_case(name: &str) -> String { name.to_string() }\n",
    )
    .expect("write fixture");

    let output = run_hook(dir.path(), &["src/backends/node/helpers.rs"]);

    assert!(!output.status.success(), "hook should reject backend-local helper");
    let stderr = String::from_utf8(output.stderr).expect("stderr must be utf8");
    assert!(
        stderr.contains("backend-local helper `to_snake_case`"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("src/codegen/naming.rs"), "stderr: {stderr}");
}

#[test]
fn accepts_context_specific_backend_wrapper_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend_dir = dir.path().join("src/backends/go");
    fs::create_dir_all(&backend_dir).expect("create backend dir");
    fs::write(
        backend_dir.join("helpers.rs"),
        "fn go_visitor_bridge_function_component(name: &str) -> String { name.to_string() }\n",
    )
    .expect("write fixture");

    let output = run_hook(dir.path(), &["src/backends/go/helpers.rs"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
