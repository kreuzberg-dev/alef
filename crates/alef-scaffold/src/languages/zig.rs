use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use alef_core::template_versions::toolchain;
use std::path::PathBuf;

pub(crate) fn scaffold_zig(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let version = &api.version;
    let ffi_lib_name = config.ffi_lib_name();
    let module_name = config.zig_module_name();

    let build_zig = format!(
        r#"const std = @import("std");

pub fn build(b: *std.Build) void {{
    const target = b.standardTargetOptions(.{{}});
    const optimize = b.standardOptimizeOption(.{{}});

    const module = b.addModule("{module_name}", .{{
        .root_source_file = b.path("src/{module_name}.zig"),
        .target = target,
        .optimize = optimize,
    }});
    module.linkSystemLibrary("{ffi_lib}", .{{}});

    const tests = b.addTest(.{{
        .root_source_file = b.path("src/{module_name}.zig"),
        .target = target,
        .optimize = optimize,
    }});
    tests.linkSystemLibrary("{ffi_lib}");

    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_tests.step);
}}
"#,
        module_name = module_name,
        ffi_lib = ffi_lib_name,
    );

    // build.zig.zon — Zig 0.13+ requires `.paths` and `.minimum_zig_version`.
    let build_zig_zon = format!(
        r#".{{
    .name = "{module_name}",
    .version = "{version}",
    .minimum_zig_version = "{min_zig}",
    .dependencies = .{{}},
    .paths = .{{
        "build.zig",
        "build.zig.zon",
        "src",
    }},
}}
"#,
        module_name = module_name,
        version = version,
        min_zig = toolchain::MIN_ZIG_VERSION,
    );

    let gitignore = "zig-cache/\nzig-out/\n.zig-cache/\n";

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
