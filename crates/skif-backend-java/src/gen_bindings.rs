use crate::type_map::{java_boxed_type, java_ffi_type, java_type};
use skif_codegen::naming::{to_class_name, to_java_name};
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, PrimitiveType, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

pub struct JavaBackend;

impl JavaBackend {
    /// Convert crate name to main class name (PascalCase).
    fn resolve_main_class(api: &ApiSurface) -> String {
        to_class_name(&api.crate_name.replace('-', "_"))
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

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Java)?;

        Ok(files)
    }
}

// ---------------------------------------------------------------------------
// NativeLib.java - FFI method handles
// ---------------------------------------------------------------------------

fn gen_native_lib(api: &ApiSurface, _config: &SkifConfig, package: &str, prefix: &str) -> String {
    // Generate the class body first, then scan it to determine which imports are needed.
    let mut body = String::with_capacity(2048);

    writeln!(body, "final class NativeLib {{").ok();
    writeln!(body, "    private static final Linker LINKER = Linker.nativeLinker();").ok();
    writeln!(body, "    private static final SymbolLookup LIB;").ok();
    writeln!(body).ok();
    writeln!(body, "    static {{").ok();
    writeln!(body, "        System.loadLibrary(\"{}\");", prefix).ok();
    writeln!(body, "        LIB = SymbolLookup.loaderLookup();").ok();
    writeln!(body, "    }}").ok();
    writeln!(body).ok();

    // Generate method handles for free functions
    for func in &api.functions {
        if !func.is_async {
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
            .unwrap();
            writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", ffi_name).ok();
            writeln!(body, "        {}", layout_str).ok();
            writeln!(body, "    );").ok();
        }
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
        .unwrap();
        writeln!(body, "        LIB.find(\"{}\").orElseThrow(),", free_name).ok();
        writeln!(body, "        FunctionDescriptor.ofVoid(ValueLayout.ADDRESS)").ok();
        writeln!(body, "    );").ok();
    }

    // Error handling
    if !api.errors.is_empty() {
        let error_name = &api.errors[0].name;
        let handle_name = format!("{}_GET_{}_MESSAGE", prefix.to_uppercase(), error_name.to_uppercase());
        writeln!(
            body,
            "    static final MethodHandle {} = LINKER.downcallHandle(",
            handle_name
        )
        .unwrap();
        writeln!(
            body,
            "        LIB.find(\"{}_get_{}_message\").orElseThrow(),",
            prefix,
            error_name.to_lowercase()
        )
        .unwrap();
        writeln!(body, "        FunctionDescriptor.of(ValueLayout.ADDRESS)").ok();
        writeln!(body, "    );").ok();
    }

    writeln!(body, "}}").ok();

    // Now assemble the file with only the imports that are actually used in the body.
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by skif").ok();
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

fn gen_main_class(api: &ApiSurface, _config: &SkifConfig, package: &str, class_name: &str, prefix: &str) -> String {
    // Generate the class body first, then scan it to determine which imports are needed.
    let mut body = String::with_capacity(4096);

    writeln!(body, "public final class {} {{", class_name).ok();
    writeln!(body, "    private {}() {{ }}", class_name).ok();
    writeln!(body).ok();

    // Generate static methods for free functions
    for func in &api.functions {
        // Always generate sync method
        gen_sync_function_method(&mut body, func, prefix, class_name);
        writeln!(body).ok();

        // Also generate async wrapper if marked as async
        if func.is_async {
            gen_async_wrapper_method(&mut body, func);
            writeln!(body).ok();
        }
    }

    // Add helper methods
    gen_helper_methods(&mut body);

    writeln!(body, "}}").ok();

    // Now assemble the file with only the imports that are actually used in the body.
    let mut out = String::with_capacity(body.len() + 512);

    writeln!(out, "// DO NOT EDIT - auto-generated by skif").ok();
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
    writeln!(out).ok();

    out.push_str(&body);

    out
}

fn gen_sync_function_method(out: &mut String, func: &FunctionDef, prefix: &str, class_name: &str) {
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
        marshal_param_to_ffi(out, &to_java_name(&param.name), &param.ty);
    }

    // Call FFI
    let ffi_handle = format!("NativeLib.{}_{}", prefix.to_uppercase(), func.name.to_uppercase());

    let call_args: Vec<String> = func
        .params
        .iter()
        .map(|p| ffi_param_name(&to_java_name(&p.name), &p.ty))
        .collect();

    if matches!(func.return_type, TypeRef::Unit) {
        writeln!(out, "            {}.invoke({});", ffi_handle, call_args.join(", ")).ok();
        writeln!(out, "        }} catch (Throwable t) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", t);",
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
        writeln!(out, "        }} catch (Throwable t) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", t);",
            class_name
        )
        .ok();
        writeln!(out, "        }}").ok();
    } else {
        writeln!(
            out,
            "            var result = ({}) {}.invoke({});",
            java_ffi_return_cast(&func.return_type),
            ffi_handle,
            call_args.join(", ")
        )
        .ok();
        writeln!(out, "            return result;").ok();
        writeln!(out, "        }} catch (Throwable t) {{").ok();
        writeln!(
            out,
            "            throw new {}Exception(\"FFI call failed\", t);",
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
    writeln!(out, "            }} catch (Exception e) {{").ok();
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

    writeln!(out, "// DO NOT EDIT - auto-generated by skif").ok();
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
// Record types (Java records)
// ---------------------------------------------------------------------------

/// Maximum line length before splitting record fields across multiple lines.
/// Checkstyle enforces 120 chars; we split at 100 to leave headroom for indentation.
const RECORD_LINE_WRAP_THRESHOLD: usize = 100;

fn gen_record_type(package: &str, typ: &TypeDef) -> String {
    let mut out = String::with_capacity(1024);

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
            format!("{} {}", ftype, to_java_name(&f.name))
        })
        .collect();

    // Build the single-line form to check length and scan for imports.
    let single_line = format!("public record {}({}) {{ }}", typ.name, field_list.join(", "));

    // Build the actual record declaration, splitting across lines if too long.
    let record_block = if single_line.len() > RECORD_LINE_WRAP_THRESHOLD && field_list.len() > 1 {
        let mut buf = String::new();
        writeln!(buf, "public record {}(", typ.name).ok();
        for (i, field) in field_list.iter().enumerate() {
            let comma = if i < field_list.len() - 1 { "," } else { "" };
            writeln!(buf, "    {}{}", field, comma).ok();
        }
        write!(buf, ") {{ }}").ok();
        buf
    } else {
        single_line.clone()
    };

    // Scan the single-line form to determine which imports are needed
    writeln!(out, "// DO NOT EDIT - auto-generated by skif").ok();
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
    writeln!(out, "{record_block}").ok();

    out
}

// ---------------------------------------------------------------------------
// Enum classes
// ---------------------------------------------------------------------------

fn gen_enum_class(package: &str, enum_def: &EnumDef) -> String {
    let mut out = String::with_capacity(1024);

    writeln!(out, "// DO NOT EDIT - auto-generated by skif").ok();
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
        TypeRef::String | TypeRef::Path | TypeRef::Json => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Bytes => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Optional(inner) => gen_ffi_layout(inner),
        TypeRef::Vec(_) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Map(_, _) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Named(_) => "ValueLayout.ADDRESS".to_string(),
        TypeRef::Unit => "".to_string(),
        TypeRef::Duration => "ValueLayout.JAVA_LONG".to_string(),
    }
}

fn marshal_param_to_ffi(out: &mut String, name: &str, ty: &TypeRef) {
    match ty {
        TypeRef::String | TypeRef::Path | TypeRef::Json => {
            let cname = "c".to_string() + name;
            writeln!(out, "            var {} = arena.allocateFrom({});", cname, name).ok();
        }
        TypeRef::Optional(inner) => {
            // For optional types, marshal the inner type if not null
            if matches!(inner.as_ref(), TypeRef::String | TypeRef::Path | TypeRef::Json) {
                let cname = "c".to_string() + name;
                writeln!(
                    out,
                    "            var {} = {} != null ? arena.allocateFrom({}) : MemorySegment.NULL;",
                    cname, name, name
                )
                .ok();
            }
        }
        _ => {
            // Primitives and others pass through directly
        }
    }
}

fn ffi_param_name(name: &str, ty: &TypeRef) -> String {
    match ty {
        TypeRef::String | TypeRef::Path | TypeRef::Json => "c".to_string() + name,
        TypeRef::Optional(inner) => {
            if matches!(inner.as_ref(), TypeRef::String | TypeRef::Path | TypeRef::Json) {
                "c".to_string() + name
            } else {
                name.to_string()
            }
        }
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
        TypeRef::String | TypeRef::Path | TypeRef::Json => true,
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
    writeln!(out, "    // Helper methods for FFI marshalling").ok();
    writeln!(out).ok();
    writeln!(out, "    private static String readCString(MemorySegment ptr) {{").ok();
    writeln!(out, "        if (ptr == null || ptr.address() == 0) {{").ok();
    writeln!(out, "            return null;").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "        return ptr.getUtf8String(0);").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();
    writeln!(
        out,
        "    private static byte[] readBytes(MemorySegment ptr, long len) {{"
    )
    .ok();
    writeln!(out, "        if (ptr == null || ptr.address() == 0) {{").ok();
    writeln!(out, "            return null;").ok();
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
