use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

pub(crate) fn scaffold_swift(_api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let module = config.swift_module();
    // Strip the minor version component: "13.0" → "13", "16.0" → "16".
    // Swift PackageDescription uses e.g. `.v13` and `.v16`.
    let min_macos_major = config
        .swift_min_macos()
        .split('.')
        .next()
        .unwrap_or("13")
        .to_string();
    let min_ios_major = config
        .swift_min_ios()
        .split('.')
        .next()
        .unwrap_or("16")
        .to_string();

    let package_swift = format!(
        r#"// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "{module}",
    platforms: [
        .macOS(.v{min_macos}),
        .iOS(.v{min_ios}),
    ],
    products: [
        .library(name: "{module}", targets: ["{module}"]),
    ],
    targets: [
        .target(name: "{module}", path: "Sources/{module}"),
        .testTarget(name: "{module}Tests", dependencies: ["{module}"], path: "Tests/{module}Tests"),
    ]
)
"#,
        module = module,
        min_macos = min_macos_major,
        min_ios = min_ios_major,
    );

    let gitignore = ".build/\nPackages/\nxcuserdata/\nDerivedData/\n.swiftpm/\n*.xcodeproj\n";

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/swift/Package.swift"),
            content: package_swift,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/swift/.gitignore"),
            content: gitignore.to_string(),
            generated_header: false,
        },
    ])
}
