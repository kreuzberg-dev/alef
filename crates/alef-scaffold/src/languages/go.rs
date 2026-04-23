use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

pub(crate) fn scaffold_go(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let go_module = config.go_module();
    let version = &api.version;
    let _ = version; // go.mod doesn't embed the package version

    let content = format!("module {module}\n\ngo 1.26\n", module = go_module,);

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/go/go.mod"),
            content,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/go/.golangci.yml"),
            content: r#"version: "2"

run:
  timeout: 5m
  concurrency: 4

linters:
  enable:
    - errcheck
    - govet
    - ineffassign
    - staticcheck
    - unused
    - revive
    - gocyclo
    - goconst
    - gocritic
    - gosec
    - misspell
    - nakedret

linters-settings:
  errcheck:
    check-type-assertions: true
    check-blank: true
  goconst:
    min-len: 3
    min-occurrences: 3
  gocyclo:
    min-complexity: 25
  revive:
    confidence: 0.8
    severity: warning

issues:
  exclude-rules:
    - path: _test\.go
      linters:
        - goconst
        - gocyclo
        - gosec
"#
            .to_string(),
            generated_header: false,
        },
    ])
}
