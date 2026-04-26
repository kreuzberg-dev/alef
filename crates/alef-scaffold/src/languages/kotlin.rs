use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use alef_core::template_versions::{maven, toolchain};
use std::path::PathBuf;

pub(crate) fn scaffold_kotlin(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let version = &api.version;
    let kotlin_package = config.kotlin_package();
    let project_name = config.crate_config.name.replace('-', "_");

    let kotlin_plugin = maven::KOTLIN_JVM_PLUGIN;
    let kotlinx_coroutines = maven::KOTLINX_COROUTINES_CORE;
    let jna = maven::JNA;
    let junit_legacy = maven::JUNIT_LEGACY;
    let jvm_target = toolchain::JVM_TARGET;

    // build.gradle.kts: Kotlin 2.x DSL — `compilerOptions` block replaces the
    // deprecated `kotlinOptions { jvmTarget }` form removed in Kotlin 2.1.
    let build_gradle = format!(
        r#"import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {{
    `java-library`
    kotlin("jvm") version "{kotlin_plugin}"
    `maven-publish`
}}

group = "{package}"
version = "{version}"

repositories {{
    mavenCentral()
}}

dependencies {{
    api("net.java.dev.jna:jna:{jna}")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:{kotlinx_coroutines}")
    testImplementation("org.jetbrains.kotlin:kotlin-test:{kotlin_plugin}")
    testImplementation("junit:junit:{junit_legacy}")
}}

java {{
    sourceCompatibility = JavaVersion.VERSION_{jvm_target}
    targetCompatibility = JavaVersion.VERSION_{jvm_target}
}}

kotlin {{
    compilerOptions {{
        jvmTarget.set(JvmTarget.JVM_{jvm_target})
    }}
}}

publishing {{
    publications {{
        create<MavenPublication>("maven") {{
            from(components["java"])
        }}
    }}
}}
"#,
        package = kotlin_package,
        version = version,
    );

    let settings_gradle = format!("rootProject.name = \"{project_name}\"\n");

    let gitignore = "build/\n.gradle/\n.idea/\n*.iml\n";

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/build.gradle.kts"),
            content: build_gradle,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/settings.gradle.kts"),
            content: settings_gradle,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/.gitignore"),
            content: gitignore.to_string(),
            generated_header: false,
        },
    ])
}
