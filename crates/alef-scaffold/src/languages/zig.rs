use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

pub(crate) fn scaffold_zig(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let version = &api.version;
    let ffi_lib_name = config.ffi_lib_name();

    // build.zig: minimal Zig 0.13 build file linking the FFI library
    let build_zig = format!(
        r#"const std = @import("std");

pub fn build(b: *std.Build) void {{
    const target = b.standardTargetOptions(.{{}});
    const optimize = b.standardOptimizeOption(.{{}});

    const lib = b.addStaticLibrary(.{{
        .name = "zig_bindings",
        .root_source_file = b.path("src/lib.zig"),
        .target = target,
        .optimize = optimize,
    }});

    // Link the FFI library
    lib.linkSystemLibrary("{ffi_lib}");

    b.installArtifact(lib);

    // Test executable
    const exe_tests = b.addTest(.{{
        .root_source_file = b.path("src/lib.zig"),
        .target = target,
        .optimize = optimize,
    }});
    exe_tests.linkSystemLibrary("{ffi_lib}");

    const run_test = b.addRunArtifact(exe_tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_test.step);
}}
"#,
        ffi_lib = ffi_lib_name,
    );

    // build.zig.zon: package manifest
    let build_zig_zon = format!(
        r#".{{
    .name = "zig_bindings",
    .version = "{version}",
}}
"#,
        version = version,
    );

    let gitignore = "zig-cache/\nzig-out/\n";

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/zig/build.zig"),
            content: build_zig,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/zig/build.zig.zon"),
            content: build_zig_zon,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/zig/.gitignore"),
            content: gitignore.to_string(),
            generated_header: false,
        },
    ])
}
