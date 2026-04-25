use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

pub(crate) fn scaffold_kotlin(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let version = &api.version;
    let kotlin_package = config.kotlin_package();

    // build.gradle.kts with Gradle, kotlinx.coroutines, JNA for FFI, and Java 21+
    let build_gradle = format!(
        r#"plugins {{
    `java-library`
    kotlin("jvm") version "2.1.10"
    `maven-publish`
}}

group = "{package}"
version = "{version}"

repositories {{
    mavenCentral()
}}

dependencies {{
    api("net.java.dev.jna:jna:5.14.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.1")
    testImplementation("org.jetbrains.kotlin:kotlin-test:2.1.10")
    testImplementation("junit:junit:4.13.2")
}}

java {{
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {{
    kotlinOptions {{
        jvmTarget = "21"
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

    let settings_gradle = "rootProject.name = \"kotlin\"\n";

    let gitignore = "build/\n.gradle/\n.idea/\n*.iml\n";

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/build.gradle.kts"),
            content: build_gradle,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/settings.gradle.kts"),
            content: settings_gradle.to_string(),
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/kotlin/.gitignore"),
            content: gitignore.to_string(),
            generated_header: false,
        },
    ])
}
