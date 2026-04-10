use crate::type_map::{java_boxed_type, java_ffi_type, java_type};
use ahash::AHashSet;
use alef_codegen::naming::{to_class_name, to_java_name};
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FunctionDef, PrimitiveType, TypeDef, TypeRef};
use heck::ToSnakeCase;
use std::fmt::Write;
use std::path::PathBuf;

/// Names that conflict with methods on `java.lang.Object` and are therefore
/// illegal as record component names or method names in generated Java code.
const JAVA_OBJECT_METHOD_NAMES: &[&str] = &[
    "wait",
    "notify",
    "notifyAll",
    "getClass",
    "hashCode",
    "equals",
    "toString",
    "clone",
    "finalize",
];

/// Sanitise a field/parameter name that would conflict with `java.lang.Object`
/// methods.  Conflicting names get a `_` suffix (e.g. `wait` -> `wait_`), which
/// is then converted to camelCase by `to_java_name`.
fn safe_java_field_name(name: &str) -> String {
    let java_name = to_java_name(name);
    if JAVA_OBJECT_METHOD_NAMES.contains(&java_name.as_str()) {
        format!("{}Value", java_name)
    } else {
        java_name
    }
}

pub struct JavaBackend;

impl JavaBackend {
    /// Convert crate name to main class name (PascalCase + "Rs" suffix).
    ///
    /// The "Rs" suffix ensures the raw FFI wrapper class has a distinct name from
    /// the public facade class (which strips the "Rs" suffix). Without this, the
    /// facade would delegate to itself, causing infinite recursion.
    fn resolve_main_class(api: &ApiSurface) -> String {
        let base = to_class_name(&api.crate_name.replace('-', "_"));
        if base.ends_with("Rs") {
            base
        } else {
            format!("{}Rs", base)
        }
    }
}

impl Backend for JavaBackend {
    fn name(&self) -> &str {
        "java"
    }

    fn language(&self) -> Language {
        Language::Java
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: true,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let package = config.java_package();
        let prefix = config.ffi_prefix();
        let main_class = Self::resolve_main_class(api);
        let package_path = package.replace('.', "/");

        let output_dir = resolve_output_dir(
            config.output.java.as_ref(),
            &config.crate_config.name,
            "packages/java/src/main/java/",
        );

        let base_path = PathBuf::from(&output_dir).join(&package_path);

        let mut files = Vec::new();

        // 1. NativeLib.java - FFI method handles
        files.push(GeneratedFile {
            path: base_path.join("NativeLib.java"),
            content: gen_native_lib(api, config, &package, &prefix),
            generated_header: true,
        });

        // 2. Main wrapper class
        files.push(GeneratedFile {
            path: base_path.join(format!("{}.java", main_class)),
            content: gen_main_class(api, config, &package, &main_class, &prefix),
            generated_header: true,
        });

        // 3. Exception class
        files.push(GeneratedFile {
            path: base_path.join(format!("{}Exception.java", main_class)),
            content: gen_exception_class(&package, &main_class),
            generated_header: true,
        });

        // 4. Record types
        for typ in &api.types {
            if !typ.is_opaque && !typ.fields.is_empty() {
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.java", typ.name)),
                    content: gen_record_type(&package, typ),
                    generated_header: true,
                });
                // Generate builder class for types with defaults
                if typ.has_default {
                    files.push(GeneratedFile {
                        path: base_path.join(format!("{}Builder.java", typ.name)),
                        content: gen_builder_class(&package, typ),
                        generated_header: true,
                    });
                }
            }
        }

        // 4b. Opaque handle types
        for typ in &api.types {
            if typ.is_opaque {
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.java", typ.name)),
                    content: gen_opaque_handle_class(&package, typ, &prefix),
                    generated_header: true,
                });
            }
        }

        // 5. Enums
        for enum_def in &api.enums {
            files.push(GeneratedFile {
                path: base_path.join(format!("{}.java", enum_def.name)),
                content: gen_enum_class(&package, enum_def),
                generated_header: true,
            });
        }

        // 6. Error exception classes
        for error in &api.errors {
            for (class_name, content) in alef_codegen::error_gen::gen_java_error_types(error, &package) {
                files.push(GeneratedFile {
                    path: base_path.join(format!("{}.java", class_name)),
                    content,
                    generated_header: true,
                });
            }
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Java)?;

        Ok(files)
    }

    fn generate_public_api(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let package = config.java_package();
        let prefix = config.ffi_prefix();
        let main_class = Self::resolve_main_class(api);
        let package_path = package.replace('.', "/");

        let output_dir = resolve_output_dir(
            config.output.java.as_ref(),
            &config.crate_config.name,
            "packages/java/src/main/java/",
        );

        let base_path = PathBuf::from(&output_dir).join(&package_path);

        // Generate a high-level public API class that wraps the raw FFI class.
        // Class name = main_class without "Rs" suffix (e.g., HtmlToMarkdownRs -> HtmlToMarkdown)
        let public_class = main_class.trim_end_matches("Rs").to_string();
        let facade_content = gen_facade_class(api, &package, &public_class, &main_class, &prefix);

        Ok(vec![GeneratedFile {
            path: base_path.join(format!("{}.java", public_class)),
            content: facade_content,
            generated_header: true,
        }])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "mvn",
            crate_suffix: "",
            depends_on_ffi: true,
            post_build: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// NativeLib.java - FFI method handles
// ---------------------------------------------------------------------------

fn gen_native_lib(api: &ApiSurface, config: &AlefConfig, package: &str, prefix: &str) -> String {
    // Generate the class body first, then scan it to determine which imports are needed.
    let mut body = String::with_capacity(2048);
    // Derive the native library name from the FFI output path (directory name with hyphens replaced
    // by underscores), falling back to `{ffi_prefix}_ffi`.
    let lib_name = config.ffi_lib_name();

    writeln!(body, "final class NativeLib {{").ok();
    writeln!(body, "    private static final Linker LINKER = Linker.nativeLinker();").ok();
    writeln!(body, "    private static final SymbolLookup LIB;").ok();
    writeln!(body).ok();
    writeln!(body, "    static {{").ok();
    writeln!(body, "        System.loadLibrary(\"{}\");", lib_name).ok();
    writeln!(body, "        LIB = SymbolLookup.loaderLookup();").ok();
    writeln!(body, "    }}").ok();
    writeln!(body).ok();

    // Generate method handles for free functions.
    // All functions get handles regardless of is_async — the FFI layer always exposes
    // synchronous C functions, and the Java async wrapper delegates to the sync method.
    for func in &api.functions {
        let ffi_name = format!("{}_{}", prefix, func.name.to_lowercase());
        let return_layout = gen_ffi_layout(&func.return_type);
        let param_layouts: Vec<String> = func.params.iter().map(|p| gen_ffi_layout(&p.ty)).collect();

        let layout_str = gen_function_descriptor(&return_layout, &param_layouts);

        let handle_name = format!("{}_{}", prefix.to_uppercase(), func.name.to_uppercase());

        writeln!(
            body,
            "    static final MethodHandle {} = LINKER.downcallHandle(",
            handle_name
        )
        .ok();
        writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", ffi_name).ok();
        writeln!(body, "        {}", layout_str).ok();
        writeln!(body, "    );").ok();
    }

    // free_string handle for releasing FFI-allocated strings
    {
        let free_name = format!("{}_free_string", prefix);
        let handle_name = format!("{}_FREE_STRING", prefix.to_uppercase());
        writeln!(body).ok();
        writeln!(
            body,
            "    static final MethodHandle {} = LINKER.downcallHandle(",
            handle_name
        )
        .ok();
        writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", free_name).ok();
        writeln!(body, "        FunctionDescriptor.ofVoid(ValueLayout.ADDRESS)").ok();
        writeln!(body, "    );").ok();
    }

    // Error handling — use the FFI's last_error_code and last_error_context symbols
    {
        writeln!(
            body,
            "    static final MethodHandle {}_LAST_ERROR_CODE = LINKER.downcallHandle(",
            prefix.to_uppercase()
        )
        .ok();
        writeln!(body, "        LIB.find(\"{}_last_error_code\").orElseThrow(),", prefix).ok();
        writeln!(body, "        FunctionDescriptor.of(ValueLayout.JAVA_INT)").ok();
        writeln!(body, "    );").ok();

        writeln!(
            body,
            "    static final MethodHandle {}_LAST_ERROR_CONTEXT = LINKER.downcallHandle(",
            prefix.to_uppercase()
        )
        .ok();
        writeln!(
            body,
            "        LIB.find(\"{}_last_error_context\").orElseThrow(),",
            prefix
        )
        .ok();
        writeln!(body, "        FunctionDescriptor.of(ValueLayout.ADDRESS)").ok();
        writeln!(body, "    );").ok();
    }

    // Accessor handles for Named return types (struct pointer → field accessor + free)
    for func in &api.functions {
        if let TypeRef::Named(name) = &func.return_type {
            let type_snake = name.to_snake_case();
            let type_upper = type_snake.to_uppercase();

            // _content accessor: (struct_ptr) -> char*
            let content_handle = format!("{}_{}_CONTENT", prefix.to_uppercase(), type_upper);
            let content_ffi = format!("{}_{}_content", prefix, type_snake);
            writeln!(body).ok();
            writeln!(
                body,
                "    static final MethodHandle {} = LINKER.downcallHandle(",
                content_handle
            )
            .ok();
            writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", content_ffi).ok();
            writeln!(
                body,
                "        FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS)"
            )
            .ok();
            writeln!(body, "    );").ok();

            // _free: (struct_ptr) -> void
            let free_handle = format!("{}_{}_FREE", prefix.to_uppercase(), type_upper);
            let free_ffi = format!("{}_{}_free", prefix, type_snake);
            writeln!(
                body,
                "    static final MethodHandle {} = LINKER.downcallHandle(",
                free_handle
            )
            .ok();
            writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", free_ffi).ok();
            writeln!(body, "        FunctionDescriptor.ofVoid(ValueLayout.ADDRESS)").ok();
            writeln!(body, "    );").ok();
        }
    }

    writeln!(body, "}}").ok();

    // Now assemble the file with only the imports that are actually used in the body.
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();
    if body.contains("Arena") {
        writeln!(out, "import java.lang.foreign.Arena;").ok();
    }
    if body.contains("FunctionDescriptor") {
        writeln!(out, "import java.lang.foreign.FunctionDescriptor;").ok();
    }
    if body.contains("Linker") {
        writeln!(out, "import java.lang.foreign.Linker;").ok();
    }
    if body.contains("MemorySegment") {
        writeln!(out, "import java.lang.foreign.MemorySegment;").ok();
    }
    if body.contains("SymbolLookup") {
        writeln!(out, "import java.lang.foreign.SymbolLookup;").ok();
    }
    if body.contains("ValueLayout") {
        writeln!(out, "import java.lang.foreign.ValueLayout;").ok();
    }
    if body.contains("MethodHandle") {
        writeln!(out, "import java.lang.invoke.MethodHandle;").ok();
    }
    writeln!(out).ok();

    out.push_str(&body);

    out
}

// ---------------------------------------------------------------------------
// Main wrapper class
// ---------------------------------------------------------------------------

fn gen_main_class(api: &ApiSurface, _config: &AlefConfig, package: &str, class_name: &str, prefix: &str) -> String {
    // Build the set of opaque type names so we can distinguish opaque handles from records
    let opaque_types: AHashSet<String> = api
        .types
        .iter()
        .filter(|t| t.is_opaque)
        .map(|t| t.name.clone())
        .collect();

    // Generate the class body first, then scan it to determine which imports are needed.
    let mut body = String::with_capacity(4096);

    writeln!(body, "public final class {} {{", class_name).ok();
    writeln!(body, "    private {}() {{ }}", class_name).ok();
    writeln!(body).ok();

    // Generate static methods for free functions
    for func in &api.functions {
        // Always generate sync method
        gen_sync_function_method(&mut body, func, prefix, class_name, &opaque_types);
        writeln!(body).ok();

        // Also generate async wrapper if marked as async
        if func.is_async {
            gen_async_wrapper_method(&mut body, func);
            writeln!(body).ok();
        }
    }

    // Add helper methods only if they are referenced in the body
    gen_helper_methods(&mut body);

    writeln!(body, "}}").ok();

    // Now assemble the file with only the imports that are actually used in the body.
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();
    if body.contains("Arena") {
        writeln!(out, "import java.lang.foreign.Arena;").ok();
    }
    if body.contains("FunctionDescriptor") {
        writeln!(out, "import java.lang.foreign.FunctionDescriptor;").ok();
    }
    if body.contains("Linker") {
        writeln!(out, "import java.lang.foreign.Linker;").ok();
    }
    if body.contains("MemorySegment") {
        writeln!(out, "import java.lang.foreign.MemorySegment;").ok();
    }
    if body.contains("SymbolLookup") {
        writeln!(out, "import java.lang.foreign.SymbolLookup;").ok();
    }
    if body.contains("ValueLayout") {
        writeln!(out, "import java.lang.foreign.ValueLayout;").ok();
    }
    if body.contains("List<") {
        writeln!(out, "import java.util.List;").ok();
    }
    if body.contains("Map<") {
        writeln!(out, "import java.util.Map;").ok();
    }
    if body.contains("Optional<") {
        writeln!(out, "import java.util.Optional;").ok();
    }
    if body.contains("HashMap<") || body.contains("new HashMap") {
        writeln!(out, "import java.util.HashMap;").ok();
    }
    if body.contains("CompletableFuture") {
        writeln!(out, "import java.util.concurrent.CompletableFuture;").ok();
    }
    if body.contains("CompletionException") {
        writeln!(out, "import java.util.concurrent.CompletionException;").ok();
    }
    if body.contains("ObjectMapper") || body.contains("readValue") {
        writeln!(out, "import com.fasterxml.jackson.databind.ObjectMapper;").ok();
    }
    writeln!(out).ok();

    out.push_str(&body);

    out
}

fn gen_sync_function_method(
    out: &mut String,
    func: &FunctionDef,
    prefix: &str,
    class_name: &str,
    opaque_types: &AHashSet<String>,
) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let ptype = java_type(&p.ty);
            format!("{} {}", ptype, to_java_name(&p.name))
        })
        .collect();

    let return_type = java_type(&func.return_type);

    writeln!(
        out,
        "    public static {} {}({}) throws {}Exception {{",
        return_type,
        to_java_name(&func.name),
        params.join(", "),
        class_name
    )
    .ok();

    writeln!(out, "        try (var arena = Arena.ofConfined()) {{").ok();

    // Marshal parameters (use camelCase Java names)
    for param in &func.params {
        marshal_param_to_ffi(out, &to_java_name(&param.name), &param.ty, opaque_types);
    }

    // Call FFI
    let ffi_handle = format!("NativeLib.{}_{}", prefix.to_uppercase(), func.name.to_uppercase());

    let call_args: Vec<String> = func
        .params
        .iter()
        .map(|p| ffi_param_name(&to_java_name(&p.name), &p.ty, opaque_types))
        .collect();

    if matches!(func.return_type, TypeRef::Unit) {
        writeln!(out, "            {}.invoke({});", ffi_handle, call_args.join(", ")).ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", e);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    } else if is_ffi_string_return(&func.return_type) {
        let free_handle = format!("NativeLib.{}_FREE_STRING", prefix.to_uppercase());
        writeln!(
            out,
            "            var resultPtr = (MemorySegment) {}.invoke({});",
            ffi_handle,
            call_args.join(", ")
        )
        .ok();
        writeln!(out, "            if (resultPtr.equals(MemorySegment.NULL)) {{").ok();
        writeln!(out, "                return null;").ok();
        writeln!(out, "            }}").ok();
        writeln!(
            out,
            "            String result = resultPtr.reinterpret(Long.MAX_VALUE).getString(0);"
        )
        .ok();
        writeln!(out, "            {}.invoke(resultPtr);", free_handle).ok();
        writeln!(out, "            return result;").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", e);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    } else if matches!(func.return_type, TypeRef::Named(_)) {
        // Named return types: FFI returns a struct pointer.
        let return_type_name = match &func.return_type {
            TypeRef::Named(name) => name,
            _ => unreachable!(),
        };
        let is_opaque = opaque_types.contains(return_type_name.as_str());

        writeln!(
            out,
            "            var resultPtr = (MemorySegment) {}.invoke({});",
            ffi_handle,
            call_args.join(", ")
        )
        .ok();
        writeln!(out, "            if (resultPtr.equals(MemorySegment.NULL)) {{").ok();
        writeln!(out, "                return null;").ok();
        writeln!(out, "            }}").ok();

        if is_opaque {
            // Opaque handles: wrap the raw pointer directly, caller owns and will close()
            writeln!(out, "            return new {}(resultPtr);", return_type_name).ok();
        } else {
            // Record types: use content accessor to get JSON, then deserialize
            let type_snake = return_type_name.to_snake_case();
            let free_handle = format!("NativeLib.{}_{}_FREE", prefix.to_uppercase(), type_snake.to_uppercase());
            let content_handle = format!(
                "NativeLib.{}_{}_CONTENT",
                prefix.to_uppercase(),
                type_snake.to_uppercase()
            );
            writeln!(
                out,
                "            var contentPtr = (MemorySegment) {}.invoke(resultPtr);",
                content_handle
            )
            .ok();
            writeln!(
                out,
                "            String content = contentPtr.equals(MemorySegment.NULL) ? null :"
            )
            .ok();
            writeln!(
                out,
                "                contentPtr.reinterpret(Long.MAX_VALUE).getString(0);"
            )
            .ok();
            writeln!(out, "            {}.invoke(resultPtr);", free_handle).ok();
            writeln!(
                out,
                "            return new ObjectMapper().readValue(content, {}.class);",
                return_type_name
            )
            .ok();
        }

        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", e);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    } else if matches!(func.return_type, TypeRef::Vec(_)) {
        // Vec return types: FFI returns a JSON string pointer; deserialize into List<T>.
        let free_handle = format!("NativeLib.{}_FREE_STRING", prefix.to_uppercase());
        writeln!(
            out,
            "            var resultPtr = (MemorySegment) {}.invoke({});",
            ffi_handle,
            call_args.join(", ")
        )
        .ok();
        writeln!(out, "            if (resultPtr.equals(MemorySegment.NULL)) {{").ok();
        writeln!(out, "                return java.util.List.of();").ok();
        writeln!(out, "            }}").ok();
        writeln!(
            out,
            "            String json = resultPtr.reinterpret(Long.MAX_VALUE).getString(0);"
        )
        .ok();
        writeln!(out, "            {}.invoke(resultPtr);", free_handle).ok();
        // Determine the element type for deserialization
        let element_type = match &func.return_type {
            TypeRef::Vec(inner) => java_type(inner),
            _ => unreachable!(),
        };
        writeln!(
            out,
            "            return new ObjectMapper().readValue(json, new com.fasterxml.jackson.core.type.TypeReference<java.util.List<{}>>() {{ }});",
            element_type
        )
        .ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", e);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    } else {
        writeln!(
            out,
            "            return ({}) {}.invoke({});",
            java_ffi_return_cast(&func.return_type),
            ffi_handle,
            call_args.join(", ")
        )
        .ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", e);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    }

    writeln!(out, "    }}").ok();
}

fn gen_async_wrapper_method(out: &mut String, func: &FunctionDef) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let ptype = java_type(&p.ty);
            format!("{} {}", ptype, to_java_name(&p.name))
        })
        .collect();

    let return_type = match &func.return_type {
        TypeRef::Unit => "Void".to_string(),
        other => java_boxed_type(other).to_string(),
    };

    let sync_method_name = to_java_name(&func.name);
    let async_method_name = format!("{}Async", sync_method_name);
    let param_names: Vec<String> = func.params.iter().map(|p| to_java_name(&p.name)).collect();

    writeln!(
        out,
        "    public static CompletableFuture<{}> {}({}) {{",
        return_type,
        async_method_name,
        params.join(", ")
    )
    .ok();
    writeln!(out, "        return CompletableFuture.supplyAsync(() -> {{").ok();
    writeln!(out, "            try {{").ok();
    writeln!(
        out,
        "                return {}({});",
        sync_method_name,
        param_names.join(", ")
    )
    .ok();
    writeln!(out, "            }} catch (Throwable e) {{").ok();
    writeln!(out, "                throw new CompletionException(e);").ok();
    writeln!(out, "            }}").ok();
    writeln!(out, "        }});").ok();
    writeln!(out, "    }}").ok();
}

// ---------------------------------------------------------------------------
// Exception class
// ---------------------------------------------------------------------------

fn gen_exception_class(package: &str, class_name: &str) -> String {
    let mut out = String::with_capacity(512);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();

    writeln!(out, "public class {}Exception extends Exception {{", class_name).ok();
    writeln!(out, "    private final int code;").ok();
    writeln!(out).ok();
    writeln!(out, "    public {}Exception(int code, String message) {{", class_name).ok();
    writeln!(out, "        super(message);").ok();
    writeln!(out, "        this.code = code;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();
    writeln!(
        out,
        "    public {}Exception(String message, Throwable cause) {{",
        class_name
    )
    .ok();
    writeln!(out, "        super(message, cause);").ok();
    writeln!(out, "        this.code = -1;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();
    writeln!(out, "    public int getCode() {{").ok();
    writeln!(out, "        return code;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

// ---------------------------------------------------------------------------
// High-level facade class (public API)
// ---------------------------------------------------------------------------

fn gen_facade_class(api: &ApiSurface, package: &str, public_class: &str, raw_class: &str, _prefix: &str) -> String {
    let mut body = String::with_capacity(4096);

    writeln!(body, "public final class {} {{", public_class).ok();
    writeln!(body, "    private {}() {{ }}", public_class).ok();
    writeln!(body).ok();

    // Generate static methods for free functions
    for func in &api.functions {
        // Sync method
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                let ptype = java_type(&p.ty);
                format!("{} {}", ptype, to_java_name(&p.name))
            })
            .collect();

        let return_type = java_type(&func.return_type);

        if !func.doc.is_empty() {
            writeln!(body, "    /**").ok();
            for line in func.doc.lines() {
                writeln!(body, "     * {}", line).ok();
            }
            writeln!(body, "     */").ok();
        }

        writeln!(
            body,
            "    public static {} {}({}) throws {}Exception {{",
            return_type,
            to_java_name(&func.name),
            params.join(", "),
            raw_class
        )
        .ok();

        // Null checks for required parameters
        for param in &func.params {
            if !param.optional {
                let pname = to_java_name(&param.name);
                writeln!(
                    body,
                    "        java.util.Objects.requireNonNull({}, \"{} must not be null\");",
                    pname, pname
                )
                .ok();
            }
        }

        // Delegate to the raw FFI class
        let call_args: Vec<String> = func.params.iter().map(|p| to_java_name(&p.name)).collect();

        if matches!(func.return_type, TypeRef::Unit) {
            writeln!(
                body,
                "        {}.{}({});",
                raw_class,
                to_java_name(&func.name),
                call_args.join(", ")
            )
            .ok();
        } else {
            writeln!(
                body,
                "        return {}.{}({});",
                raw_class,
                to_java_name(&func.name),
                call_args.join(", ")
            )
            .ok();
        }

        writeln!(body, "    }}").ok();
        writeln!(body).ok();

        // Generate overload without optional params (convenience method)
        let has_optional = func.params.iter().any(|p| p.optional);
        if has_optional {
            let required_params: Vec<String> = func
                .params
                .iter()
                .filter(|p| !p.optional)
                .map(|p| {
                    let ptype = java_type(&p.ty);
                    format!("{} {}", ptype, to_java_name(&p.name))
                })
                .collect();

            writeln!(
                body,
                "    public static {} {}({}) throws {}Exception {{",
                return_type,
                to_java_name(&func.name),
                required_params.join(", "),
                raw_class
            )
            .ok();

            // Build call with null for optional params
            let full_args: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    if p.optional {
                        "null".to_string()
                    } else {
                        to_java_name(&p.name)
                    }
                })
                .collect();

            if matches!(func.return_type, TypeRef::Unit) {
                writeln!(body, "        {}({});", to_java_name(&func.name), full_args.join(", ")).ok();
            } else {
                writeln!(
                    body,
                    "        return {}({});",
                    to_java_name(&func.name),
                    full_args.join(", ")
                )
                .ok();
            }

            writeln!(body, "    }}").ok();
            writeln!(body).ok();
        }
    }

    writeln!(body, "}}").ok();

    // Now assemble the file with imports
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();

    // Check what imports are needed based on content
    if body.contains("List<") {
        writeln!(out, "import java.util.List;").ok();
    }
    if body.contains("Map<") {
        writeln!(out, "import java.util.Map;").ok();
    }
    if body.contains("Optional<") {
        writeln!(out, "import java.util.Optional;").ok();
    }

    writeln!(out).ok();
    out.push_str(&body);

    out
}

// ---------------------------------------------------------------------------
// Opaque handle classes
// ---------------------------------------------------------------------------

fn gen_opaque_handle_class(package: &str, typ: &TypeDef, prefix: &str) -> String {
    let mut out = String::with_capacity(1024);
    let class_name = &typ.name;
    let type_snake = class_name.to_snake_case();

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();
    writeln!(out, "import java.lang.foreign.MemorySegment;").ok();
    writeln!(out).ok();

    if !typ.doc.is_empty() {
        writeln!(out, "/**").ok();
        for line in typ.doc.lines() {
            writeln!(out, " * {}", line).ok();
        }
        writeln!(out, " */").ok();
    }

    writeln!(out, "public class {} implements AutoCloseable {{", class_name).ok();
    writeln!(out, "    private final MemorySegment handle;").ok();
    writeln!(out).ok();
    writeln!(out, "    {}(MemorySegment handle) {{", class_name).ok();
    writeln!(out, "        this.handle = handle;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();
    writeln!(out, "    MemorySegment handle() {{").ok();
    writeln!(out, "        return this.handle;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();
    writeln!(out, "    @Override").ok();
    writeln!(out, "    public void close() {{").ok();
    writeln!(
        out,
        "        if (handle != null && !handle.equals(MemorySegment.NULL)) {{"
    )
    .ok();
    writeln!(out, "            try {{").ok();
    writeln!(
        out,
        "                NativeLib.{}_{}_FREE.invoke(handle);",
        prefix.to_uppercase(),
        type_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "            }} catch (Throwable e) {{").ok();
    writeln!(
        out,
        "                throw new RuntimeException(\"Failed to free {}: \" + e.getMessage(), e);",
        class_name
    )
    .ok();
    writeln!(out, "            }}").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();

    out
}

// ---------------------------------------------------------------------------
// Record types (Java records)
// ---------------------------------------------------------------------------

/// Maximum line length before splitting record fields across multiple lines.
/// Checkstyle enforces 120 chars; we split at 100 to leave headroom for indentation.
const RECORD_LINE_WRAP_THRESHOLD: usize = 100;

fn gen_record_type(package: &str, typ: &TypeDef) -> String {
    // Generate the record body first, then scan for needed imports
    let field_list: Vec<String> = typ
        .fields
        .iter()
        .map(|f| {
            let ftype = if f.optional {
                format!("Optional<{}>", java_boxed_type(&f.ty))
            } else {
                java_type(&f.ty).to_string()
            };
            format!("{} {}", ftype, safe_java_field_name(&f.name))
        })
        .collect();

    // Build the single-line form to check length and scan for imports.
    let single_line = format!("public record {}({}) {{ }}", typ.name, field_list.join(", "));

    // Build the actual record declaration, splitting across lines if too long.
    let mut record_block = String::new();
    if single_line.len() > RECORD_LINE_WRAP_THRESHOLD && field_list.len() > 1 {
        writeln!(record_block, "public record {}(", typ.name).ok();
        for (i, field) in field_list.iter().enumerate() {
            let comma = if i < field_list.len() - 1 { "," } else { "" };
            writeln!(record_block, "    {}{}", field, comma).ok();
        }
        writeln!(record_block, ") {{").ok();
    } else {
        writeln!(record_block, "public record {}({}) {{", typ.name, field_list.join(", ")).ok();
    }

    // Add builder() factory method if type has defaults
    if typ.has_default {
        writeln!(record_block, "    public static {}Builder builder() {{", typ.name).ok();
        writeln!(record_block, "        return new {}Builder();", typ.name).ok();
        writeln!(record_block, "    }}").ok();
    }

    writeln!(record_block, "}}").ok();

    // Scan the single-line form to determine which imports are needed
    let mut out = String::with_capacity(record_block.len() + 512);
    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();
    if single_line.contains("List<") {
        writeln!(out, "import java.util.List;").ok();
    }
    if single_line.contains("Map<") {
        writeln!(out, "import java.util.Map;").ok();
    }
    if single_line.contains("Optional<") {
        writeln!(out, "import java.util.Optional;").ok();
    }
    writeln!(out).ok();
    write!(out, "{}", record_block).ok();

    out
}

// ---------------------------------------------------------------------------
// Enum classes
// ---------------------------------------------------------------------------

fn gen_enum_class(package: &str, enum_def: &EnumDef) -> String {
    let mut out = String::with_capacity(1024);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();

    writeln!(out, "public enum {} {{", enum_def.name).ok();

    for (i, variant) in enum_def.variants.iter().enumerate() {
        let comma = if i < enum_def.variants.len() - 1 { "," } else { ";" };
        writeln!(out, "    {}{}", variant.name, comma).ok();
    }

    writeln!(out, "}}").ok();

    out
}

// ---------------------------------------------------------------------------
// Helper functions for FFI marshalling
// ---------------------------------------------------------------------------

fn gen_ffi_layout(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => java_ffi_type(prim).to_string(),
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Bytes => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Optional(inner) => gen_ffi_layout(inner),
        TypeRef::Vec(_) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Map(_, _) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Named(_) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Unit => "".to_string(),
        TypeRef::Duration => "ValueLayout.JAVA_LONG".to_string(),
    }
}

fn marshal_param_to_ffi(out: &mut String, name: &str, ty: &TypeRef, opaque_types: &AHashSet<String>) {
    match ty {
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => {
            let cname = "c".to_string() + name;
            writeln!(out, "            var {} = arena.allocateFrom({});", cname, name).ok();
        }
        TypeRef::Named(type_name) => {
            let cname = "c".to_string() + name;
            if opaque_types.contains(type_name.as_str()) {
                // Opaque handles: pass the inner MemorySegment via .handle()
                writeln!(out, "            var {} = {}.handle();", cname, name).ok();
            } else {
                // Non-opaque named types: serialize to JSON and pass as string
                writeln!(out, "            var {} = MemorySegment.NULL;", cname).ok();
            }
        }
        TypeRef::Optional(inner) => {
            // For optional types, marshal the inner type if not null
            match inner.as_ref() {
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => {
                    let cname = "c".to_string() + name;
                    writeln!(
                        out,
                        "            var {} = {} != null ? arena.allocateFrom({}) : MemorySegment.NULL;",
                        cname, name, name
                    )
                    .ok();
                }
                TypeRef::Named(type_name) => {
                    let cname = "c".to_string() + name;
                    if opaque_types.contains(type_name.as_str()) {
                        writeln!(
                            out,
                            "            var {} = {} != null ? {}.handle() : MemorySegment.NULL;",
                            cname, name, name
                        )
                        .ok();
                    } else {
                        writeln!(out, "            var {} = MemorySegment.NULL;", cname).ok();
                    }
                }
                _ => {
                    // Other optional types (primitives) pass through
                }
            }
        }
        _ => {
            // Primitives and others pass through directly
        }
    }
}

fn ffi_param_name(name: &str, ty: &TypeRef, _opaque_types: &AHashSet<String>) -> String {
    match ty {
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "c".to_string() + name,
        TypeRef::Named(_) => "c".to_string() + name,
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Named(_) => {
                "c".to_string() + name
            }
            _ => name.to_string(),
        },
        _ => name.to_string(),
    }
}

/// Build a `FunctionDescriptor` string for a given return layout and parameter layouts.
/// Handles void returns (ofVoid) and non-void returns (of) correctly.
fn gen_function_descriptor(return_layout: &str, param_layouts: &[String]) -> String {
    if return_layout.is_empty() {
        // Void return
        if param_layouts.is_empty() {
            "FunctionDescriptor.ofVoid()".to_string()
        } else {
            format!("FunctionDescriptor.ofVoid({})", param_layouts.join(", "))
        }
    } else {
        // Non-void return
        if param_layouts.is_empty() {
            format!("FunctionDescriptor.of({})", return_layout)
        } else {
            format!("FunctionDescriptor.of({}, {})", return_layout, param_layouts.join(", "))
        }
    }
}

/// Returns true if the given return type maps to an FFI ADDRESS that represents a string
/// (i.e. the FFI returns `*mut c_char` which must be unmarshaled and freed).
fn is_ffi_string_return(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => true,
        TypeRef::Optional(inner) => is_ffi_string_return(inner),
        _ => false,
    }
}

/// Returns the appropriate Java cast type for non-string FFI return values.
fn java_ffi_return_cast(ty: &TypeRef) -> &'static str {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => "boolean",
            PrimitiveType::U8 | PrimitiveType::I8 => "byte",
            PrimitiveType::U16 | PrimitiveType::I16 => "short",
            PrimitiveType::U32 | PrimitiveType::I32 => "int",
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => "long",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        },
        TypeRef::Bytes | TypeRef::Vec(_) | TypeRef::Map(_, _) | TypeRef::Named(_) => "MemorySegment",
        _ => "MemorySegment",
    }
}

fn gen_helper_methods(out: &mut String) {
    // Only emit helper methods that are actually called in the generated body.
    let needs_read_cstring = out.contains("readCString(");
    let needs_read_bytes = out.contains("readBytes(");

    if !needs_read_cstring && !needs_read_bytes {
        return;
    }

    writeln!(out, "    // Helper methods for FFI marshalling").ok();
    writeln!(out).ok();

    if needs_read_cstring {
        writeln!(out, "    private static String readCString(MemorySegment ptr) {{").ok();
        writeln!(out, "        if (ptr == null || ptr.address() == 0) {{").ok();
        writeln!(out, "            return null;").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "        return ptr.getUtf8String(0);").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();
    }

    if needs_read_bytes {
        writeln!(
            out,
            "    private static byte[] readBytes(MemorySegment ptr, long len) {{"
        )
        .ok();
        writeln!(out, "        if (ptr == null || ptr.address() == 0) {{").ok();
        writeln!(out, "            return new byte[0];").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "        byte[] bytes = new byte[(int) len];").ok();
        writeln!(
            out,
            "        MemorySegment.copy(ptr, ValueLayout.JAVA_BYTE.byteSize() * 0, bytes, 0, (int) len);"
        )
        .ok();
        writeln!(out, "        return bytes;").ok();
        writeln!(out, "    }}").ok();
    }
}

// ---------------------------------------------------------------------------
// Builder class for types with defaults
// ---------------------------------------------------------------------------

/// Format a default value for an Optional field, wrapping it in Optional.of()
/// with proper Java literal syntax.
fn format_optional_value(ty: &TypeRef, default: &str) -> String {
    // Check if the default is already wrapped (e.g., "Optional.of(...)" or "Optional.empty()")
    if default.contains("Optional.") {
        return default.to_string();
    }

    // Unwrap Optional types to get the inner type
    let inner_ty = match ty {
        TypeRef::Optional(inner) => inner.as_ref(),
        other => other,
    };

    // Determine the proper literal suffix based on type
    let formatted_value = match inner_ty {
        TypeRef::Primitive(p) => match p {
            PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::Isize | PrimitiveType::Usize => {
                // Add 'L' suffix for long values if not already present
                if default.ends_with('L') || default.ends_with('l') {
                    default.to_string()
                } else if default.parse::<i64>().is_ok() {
                    format!("{}L", default)
                } else {
                    default.to_string()
                }
            }
            PrimitiveType::F32 => {
                // Add 'f' suffix for float values if not already present
                if default.ends_with('f') || default.ends_with('F') {
                    default.to_string()
                } else if default.parse::<f32>().is_ok() {
                    format!("{}f", default)
                } else {
                    default.to_string()
                }
            }
            PrimitiveType::F64 => {
                // Double defaults can have optional 'd' suffix, but 0.0 is fine
                default.to_string()
            }
            _ => default.to_string(),
        },
        _ => default.to_string(),
    };

    format!("Optional.of({})", formatted_value)
}

fn gen_builder_class(package: &str, typ: &TypeDef) -> String {
    let mut body = String::with_capacity(2048);

    writeln!(body, "public class {}Builder {{", typ.name).ok();
    writeln!(body).ok();

    // Generate field declarations with defaults
    for field in &typ.fields {
        let field_name = safe_java_field_name(&field.name);

        // Skip unnamed tuple fields (name is "_0", "_1", "0", "1", etc.) — Java requires named fields
        if field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit())
            || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
        {
            continue;
        }

        // Duration maps to primitive `long` in the public record, but in builder
        // classes we use boxed `Long` so that `null` can represent "not set".
        let field_type = if field.optional {
            format!("Optional<{}>", java_boxed_type(&field.ty))
        } else if matches!(field.ty, TypeRef::Duration) {
            java_boxed_type(&field.ty).to_string()
        } else {
            java_type(&field.ty).to_string()
        };

        let default_value = if field.optional {
            // For Optional fields, always use Optional.empty() or Optional.of(value)
            if let Some(default) = &field.default {
                // If there's an explicit default, wrap it in Optional.of()
                format_optional_value(&field.ty, default)
            } else {
                // If no default, use Optional.empty()
                "Optional.empty()".to_string()
            }
        } else {
            // For non-Optional fields, use regular defaults
            if let Some(default) = &field.default {
                default.clone()
            } else {
                match &field.ty {
                    TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "\"\"".to_string(),
                    TypeRef::Bytes => "new byte[0]".to_string(),
                    TypeRef::Primitive(p) => match p {
                        PrimitiveType::Bool => "false".to_string(),
                        PrimitiveType::F32 | PrimitiveType::F64 => "0.0".to_string(),
                        _ => "0".to_string(),
                    },
                    TypeRef::Vec(_) => "List.of()".to_string(),
                    TypeRef::Map(_, _) => "Map.of()".to_string(),
                    TypeRef::Optional(_) => "Optional.empty()".to_string(),
                    TypeRef::Duration => "null".to_string(),
                    _ => "null".to_string(),
                }
            }
        };

        writeln!(body, "    private {} {} = {};", field_type, field_name, default_value).ok();
    }

    writeln!(body).ok();

    // Generate withXxx() methods
    for field in &typ.fields {
        // Skip unnamed tuple fields (name is "_0", "_1", "0", "1", etc.) — Java requires named fields
        if field.name.starts_with('_') && field.name[1..].chars().all(|c| c.is_ascii_digit())
            || field.name.chars().next().is_none_or(|c| c.is_ascii_digit())
        {
            continue;
        }

        let field_name = safe_java_field_name(&field.name);
        let field_name_pascal = to_class_name(&field.name);
        let field_type = if field.optional {
            format!("Optional<{}>", java_boxed_type(&field.ty))
        } else if matches!(field.ty, TypeRef::Duration) {
            java_boxed_type(&field.ty).to_string()
        } else {
            java_type(&field.ty).to_string()
        };

        writeln!(
            body,
            "    public {}Builder with{}({} value) {{",
            typ.name, field_name_pascal, field_type
        )
        .ok();
        writeln!(body, "        this.{} = value;", field_name).ok();
        writeln!(body, "        return this;").ok();
        writeln!(body, "    }}").ok();
        writeln!(body).ok();
    }

    // Generate build() method
    writeln!(body, "    public {} build() {{", typ.name).ok();
    writeln!(body, "        return new {}(", typ.name).ok();
    let non_tuple_fields: Vec<_> = typ
        .fields
        .iter()
        .filter(|f| {
            // Include named fields (skip unnamed tuple fields)
            !(f.name.starts_with('_') && f.name[1..].chars().all(|c| c.is_ascii_digit())
                || f.name.chars().next().is_none_or(|c| c.is_ascii_digit()))
        })
        .collect();
    for (i, field) in non_tuple_fields.iter().enumerate() {
        let field_name = safe_java_field_name(&field.name);
        let comma = if i < non_tuple_fields.len() - 1 { "," } else { "" };
        writeln!(body, "            {}{}", field_name, comma).ok();
    }
    writeln!(body, "        );").ok();
    writeln!(body, "    }}").ok();

    writeln!(body, "}}").ok();

    // Now assemble with conditional imports based on what's actually used in the body
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by alef").ok();
    writeln!(out, "package {};", package).ok();
    writeln!(out).ok();

    if body.contains("List<") {
        writeln!(out, "import java.util.List;").ok();
    }
    if body.contains("Map<") {
        writeln!(out, "import java.util.Map;").ok();
    }
    if body.contains("Optional<") {
        writeln!(out, "import java.util.Optional;").ok();
    }

    writeln!(out).ok();
    out.push_str(&body);

    out
}
