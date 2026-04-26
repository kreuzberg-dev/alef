//! Java (Panama FFM) trait bridge code generation for plugin systems.
//!
//! This module generates Java code that wraps C FFI vtables for plugin registration.
//! Since Java cannot expose method references as raw C function pointers, we use
//! Java 21+ Foreign Function & Memory API (Panama) upcall stubs to bridge Java
//! implementations into the C vtable structure.
//!
//! For each `[[trait_bridges]]` entry, this module generates:
//!
//! 1. A `public interface I{TraitName} { ... }` with methods matching the trait's methods
//!    plus Plugin lifecycle methods (name, version, initialize, shutdown).
//! 2. A `{TraitName}Bridge` class that:
//!    - Allocates Panama FFM upcall stubs for each trait method
//!    - Builds the C vtable as a MemorySegment
//!    - Manages memory lifecycle with AutoCloseable
//! 3. Registration helper: `public static void register{TraitName}(I{TraitName} impl)`
//!    that builds the vtable and calls the C registration function.
//! 4. Unregistration helper: `public static void unregister{TraitName}(String name)`.

use alef_core::ir::{TypeDef, TypeRef};
use heck::{ToPascalCase, ToSnakeCase};
use std::fmt::Write;

use crate::type_map::{java_type, java_ffi_type};

/// Generate all trait bridge code for a single trait definition.
/// Returns Java source code as a String.
///
/// Takes a full TypeDef so we have access to method parameters and full type information.
pub fn gen_trait_bridge(
    trait_def: &TypeDef,
    prefix: &str,
    has_super_trait: bool,
) -> String {
    let trait_name = &trait_def.name;
    let trait_pascal = trait_name.to_pascal_case();
    let trait_snake = trait_name.to_snake_case();
    let prefix_upper = prefix.to_uppercase();

    let mut out = String::with_capacity(8192);

    // --- Public interface ---
    writeln!(out, "/**").ok();
    writeln!(out, " * Bridge interface for {} plugin system.", trait_pascal).ok();
    writeln!(out, " *").ok();
    writeln!(
        out,
        " * Implementations provide methods that are called via upcall stubs"
    )
    .ok();
    writeln!(out, " * into the C vtable during registration.").ok();
    writeln!(out, " */").ok();
    writeln!(out, "public interface I{} {{", trait_pascal).ok();
    writeln!(out).ok();

    // Plugin lifecycle methods — only when a super_trait (Plugin) is configured
    if has_super_trait {
        writeln!(out, "    /** Return the plugin name. */").ok();
        writeln!(out, "    String name();").ok();
        writeln!(out).ok();

        writeln!(out, "    /** Return the plugin version. */").ok();
        writeln!(out, "    String version();").ok();
        writeln!(out).ok();

        writeln!(out, "    /** Initialize the plugin. */").ok();
        writeln!(out, "    void initialize() throws Exception;").ok();
        writeln!(out).ok();

        writeln!(out, "    /** Shut down the plugin. */").ok();
        writeln!(out, "    void shutdown() throws Exception;").ok();
        writeln!(out).ok();
    }

    // Trait methods
    for method in &trait_def.methods {
        let return_type_str = java_type(&method.return_type);
        let params_str = method
            .params
            .iter()
            .map(|p| format!("{} {}", java_type(&p.ty), p.name))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(out, "    /**").ok();
        writeln!(out, "     * {}", method.name).ok();
        writeln!(out, "     */").ok();
        if method.error_type.is_some() {
            writeln!(out, "    {} {}({}) throws Exception;", return_type_str, method.name, params_str).ok();
        } else {
            writeln!(out, "    {} {}({});", return_type_str, method.name, params_str).ok();
        }
        writeln!(out).ok();
    }

    writeln!(out, "}}").ok();
    writeln!(out).ok();

    // --- Bridge class for FFI upcall stubs ---
    writeln!(out, "/**").ok();
    writeln!(
        out,
        " * Allocates Panama FFM upcall stubs for a {} trait implementation",
        trait_pascal
    )
    .ok();
    writeln!(out, " * and assembles the C vtable in native memory.").ok();
    writeln!(out, " */").ok();
    writeln!(out, "final class {}Bridge implements AutoCloseable {{", trait_pascal).ok();
    writeln!(out).ok();

    writeln!(out, "    private static final Linker LINKER = Linker.nativeLinker();").ok();
    writeln!(
        out,
        "    private static final MethodHandles.Lookup LOOKUP = MethodHandles.lookup();"
    )
    .ok();
    writeln!(out).ok();

    // Number of vtable fields: optionally name_fn, version_fn, initialize_fn, shutdown_fn,
    // then trait methods, then free_user_data.
    let num_methods = trait_def.methods.len();
    let num_super_slots = if has_super_trait { 4usize } else { 0usize };
    let num_vtable_fields = num_super_slots + num_methods + 1; // (super-trait methods +) trait methods + free_user_data
    writeln!(
        out,
        "    // C vtable: {} fields ({} plugin methods + {} trait methods + free_user_data)",
        num_vtable_fields, num_super_slots, num_methods
    )
    .ok();
    writeln!(
        out,
        "    private static final long VTABLE_SIZE = (long) ValueLayout.ADDRESS.byteSize() * {}L;",
        num_vtable_fields
    )
    .ok();
    writeln!(out).ok();

    writeln!(out, "    private final Arena arena;").ok();
    writeln!(out, "    private final MemorySegment vtable;").ok();
    writeln!(out, "    private final I{} impl;", trait_pascal).ok();
    writeln!(out).ok();

    // Constructor
    writeln!(out, "    {}Bridge(final I{} impl) {{", trait_pascal, trait_pascal).ok();
    writeln!(out, "        this.impl = impl;").ok();
    writeln!(out, "        this.arena = Arena.ofConfined();").ok();
    writeln!(out, "        this.vtable = arena.allocate(VTABLE_SIZE);").ok();
    writeln!(out).ok();
    writeln!(out, "        try {{").ok();
    writeln!(out, "            long offset = 0L;").ok();
    writeln!(out).ok();

    if has_super_trait {
        // Register name_fn
        writeln!(
            out,
            "            var stubName = LINKER.upcallStub(LOOKUP.bind(this, \"handleName\","
        )
        .ok();
        writeln!(out, "                MethodType.methodType(MemorySegment.class)),").ok();
        writeln!(
            out,
            "                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS),"
        )
        .ok();
        writeln!(out, "                arena);").ok();
        writeln!(out, "            vtable.set(ValueLayout.ADDRESS, offset, stubName);").ok();
        writeln!(out, "            offset += ValueLayout.ADDRESS.byteSize();").ok();
        writeln!(out).ok();

        // Register version_fn
        writeln!(
            out,
            "            var stubVersion = LINKER.upcallStub(LOOKUP.bind(this, \"handleVersion\","
        )
        .ok();
        writeln!(out, "                MethodType.methodType(MemorySegment.class)),").ok();
        writeln!(
            out,
            "                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS),"
        )
        .ok();
        writeln!(out, "                arena);").ok();
        writeln!(out, "            vtable.set(ValueLayout.ADDRESS, offset, stubVersion);").ok();
        writeln!(out, "            offset += ValueLayout.ADDRESS.byteSize();").ok();
        writeln!(out).ok();

        // Register initialize_fn
        writeln!(
            out,
            "            var stubInitialize = LINKER.upcallStub(LOOKUP.bind(this, \"handleInitialize\","
        )
        .ok();
        writeln!(out, "                MethodType.methodType(int.class)),").ok();
        writeln!(
            out,
            "                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS),"
        )
        .ok();
        writeln!(out, "                arena);").ok();
        writeln!(
            out,
            "            vtable.set(ValueLayout.ADDRESS, offset, stubInitialize);"
        )
        .ok();
        writeln!(out, "            offset += ValueLayout.ADDRESS.byteSize();").ok();
        writeln!(out).ok();

        // Register shutdown_fn
        writeln!(
            out,
            "            var stubShutdown = LINKER.upcallStub(LOOKUP.bind(this, \"handleShutdown\","
        )
        .ok();
        writeln!(out, "                MethodType.methodType(int.class)),").ok();
        writeln!(
            out,
            "                FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS),"
        )
        .ok();
        writeln!(out, "                arena);").ok();
        writeln!(
            out,
            "            vtable.set(ValueLayout.ADDRESS, offset, stubShutdown);"
        )
        .ok();
        writeln!(out, "            offset += ValueLayout.ADDRESS.byteSize();").ok();
        writeln!(out).ok();
    }

    // Register trait methods
    for method in &trait_def.methods {
        let handle_name = format!("handle{}", method.name.to_pascal_case());

        // Build MethodType with all parameters + return type
        // Method signature: (user_data: void*, [params...], [out_result: char**, when non-void], out_error: char**) -> int32_t
        let mut method_type_params = vec!["MemorySegment.class".to_string()]; // user_data

        // Add C FFI types for parameters (all passed as MemorySegment across the FFI boundary)
        for _param in &method.params {
            method_type_params.push("MemorySegment.class".to_string());
        }

        // Add output parameter slots
        if !matches!(method.return_type, TypeRef::Unit) {
            method_type_params.push("MemorySegment.class".to_string()); // out_result
        }
        method_type_params.push("MemorySegment.class".to_string()); // out_error

        writeln!(
            out,
            "            var stub{} = LINKER.upcallStub(LOOKUP.bind(this, \"{}\",",
            method.name.to_pascal_case(),
            handle_name
        )
        .ok();
        writeln!(out, "                MethodType.methodType(int.class, {})),", method_type_params.join(", ")).ok();

        // Build FunctionDescriptor
        let mut func_desc_params = vec!["ValueLayout.ADDRESS".to_string()]; // user_data: void*
        for param in &method.params {
            // Map parameter type to FFI layout
            let ffi_layout = match &param.ty {
                TypeRef::Primitive(p) => java_ffi_type(p).to_string(),
                _ => "ValueLayout.ADDRESS".to_string(), // All complex types passed as char*
            };
            func_desc_params.push(ffi_layout);
        }
        if !matches!(method.return_type, TypeRef::Unit) {
            func_desc_params.push("ValueLayout.ADDRESS".to_string()); // out_result: char**
        }
        func_desc_params.push("ValueLayout.ADDRESS".to_string()); // out_error: char**

        writeln!(
            out,
            "                FunctionDescriptor.of(ValueLayout.JAVA_INT, {}),"
,            func_desc_params.join(", ")
        )
        .ok();
        writeln!(out, "                arena);").ok();
        writeln!(
            out,
            "            vtable.set(ValueLayout.ADDRESS, offset, stub{});",
            method.name.to_pascal_case()
        )
        .ok();
        writeln!(out, "            offset += ValueLayout.ADDRESS.byteSize();").ok();
        writeln!(out).ok();
    }

    // Register free_user_data (NULL for now)
    writeln!(
        out,
        "            vtable.set(ValueLayout.ADDRESS, offset, MemorySegment.NULL);"
    )
    .ok();
    writeln!(out).ok();

    writeln!(out, "        }} catch (ReflectiveOperationException e) {{").ok();
    writeln!(out, "            arena.close();").ok();
    writeln!(
        out,
        "            throw new RuntimeException(\"Failed to create trait bridge stubs\", e);"
    )
    .ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();

    // Accessor method
    writeln!(out, "    MemorySegment vtableSegment() {{").ok();
    writeln!(out, "        return vtable;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out).ok();

    // Handle methods
    writeln!(
        out,
        "    // --- Upcall handlers (return MemorySegment pointing to allocated strings) ---"
    )
    .ok();
    writeln!(out).ok();

    if has_super_trait {
        writeln!(out, "    private MemorySegment handleName() {{").ok();
        writeln!(out, "        try {{").ok();
        writeln!(out, "            String name = impl.name();").ok();
        writeln!(out, "            return arena.allocateFrom(name);").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(out, "            return MemorySegment.NULL;").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();

        writeln!(out, "    private MemorySegment handleVersion() {{").ok();
        writeln!(out, "        try {{").ok();
        writeln!(out, "            String version = impl.version();").ok();
        writeln!(out, "            return arena.allocateFrom(version);").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(out, "            return MemorySegment.NULL;").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();

        writeln!(out, "    private int handleInitialize() {{").ok();
        writeln!(out, "        try {{").ok();
        writeln!(out, "            impl.initialize();").ok();
        writeln!(out, "            return 0;").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(out, "            return 1;").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();

        writeln!(out, "    private int handleShutdown() {{").ok();
        writeln!(out, "        try {{").ok();
        writeln!(out, "            impl.shutdown();").ok();
        writeln!(out, "            return 0;").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(out, "            return 1;").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();
    }

    // Trait method handlers
    for method in &trait_def.methods {
        // Method signature matches C vtable: (void* user_data, [params...], [char** out_result], char** out_error) -> int32_t
        let mut sig_params = vec!["MemorySegment userData".to_string()];
        for param in &method.params {
            sig_params.push(format!("MemorySegment {}", param.name));
        }
        if !matches!(method.return_type, TypeRef::Unit) {
            sig_params.push("MemorySegment outResult".to_string());
        }
        sig_params.push("MemorySegment outError".to_string());

        writeln!(
            out,
            "    private int handle{}({}) {{",
            method.name.to_pascal_case(),
            sig_params.join(", ")
        )
        .ok();
        writeln!(out, "        try {{").ok();

        // Unmarshal parameters from MemorySegment to Java types
        for param in &method.params {
            gen_param_unmarshal(&mut out, &param.name, &param.ty);
        }

        // Call the method
        let java_params: Vec<String> = method.params.iter().map(|p| p.name.clone()).collect();
        if matches!(method.return_type, TypeRef::Unit) {
            writeln!(out, "            impl.{}({});", method.name, java_params.join(", ")).ok();
        } else {
            let return_type_str = java_type(&method.return_type);
            writeln!(out, "            {} result = impl.{}({});", return_type_str, method.name, java_params.join(", ")).ok();
            // Marshal result to JSON and store in outResult
            writeln!(out, "            String json = new com.fasterxml.jackson.databind.ObjectMapper().writeValueAsString(result);").ok();
            writeln!(out, "            MemorySegment jsonCs = arena.allocateFrom(json);").ok();
            writeln!(out, "            outResult.set(ValueLayout.ADDRESS, 0, jsonCs);").ok();
        }
        writeln!(out, "            return 0; // success").ok();
        writeln!(out, "        }} catch (Throwable e) {{").ok();
        writeln!(out, "            String errMsg = e.getClass().getSimpleName() + \": \" + e.getMessage();").ok();
        writeln!(out, "            MemorySegment errCs = arena.allocateFrom(errMsg);").ok();
        writeln!(out, "            outError.set(ValueLayout.ADDRESS, 0, errCs);").ok();
        writeln!(out, "            return 1; // error").ok();
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out).ok();
    }

    writeln!(out, "    @Override").ok();
    writeln!(out, "    public void close() {{").ok();
    writeln!(out, "        arena.close();").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();

    // --- Bridge registry (keeps arenas + upcall stubs alive) ---
    writeln!(
        out,
        "/** Registry of live {} bridges — keeps upcall stubs and arenas alive. */",
        trait_pascal
    )
    .ok();
    writeln!(
        out,
        "private static final java.util.concurrent.ConcurrentHashMap<String, {}Bridge> {}_BRIDGES",
        trait_pascal,
        trait_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "    = new java.util.concurrent.ConcurrentHashMap<>();").ok();
    writeln!(out).ok();

    // --- Registration helpers ---
    writeln!(
        out,
        "/** Register a {} implementation via Panama FFM upcall stubs. */",
        trait_pascal
    )
    .ok();
    writeln!(
        out,
        "public static void register{}(final I{} impl) throws Exception {{",
        trait_pascal, trait_pascal
    )
    .ok();
    // Do NOT use try-with-resources: the bridge arena must stay open for the lifetime
    // of the plugin. We store it in the static registry so it is not GC'd or closed early.
    writeln!(out, "    var bridge = new {}Bridge(impl);", trait_pascal).ok();
    writeln!(out, "    try {{").ok();
    writeln!(out, "        try (var nameArena = Arena.ofConfined()) {{").ok();
    writeln!(out, "            var nameCs = nameArena.allocateFrom(impl.name());").ok();
    writeln!(out, "            var outErrArena = Arena.ofConfined();").ok();
    writeln!(
        out,
        "            MemorySegment outErr = outErrArena.allocate(ValueLayout.ADDRESS);"
    )
    .ok();
    writeln!(
        out,
        "            int rc = (int) NativeLib.{}_REGISTER_{}.invoke(nameCs, bridge.vtableSegment(), MemorySegment.NULL, outErr);",
        prefix_upper,
        trait_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "            if (rc != 0) {{").ok();
    writeln!(
        out,
        "                MemorySegment errPtr = outErr.get(ValueLayout.ADDRESS, 0);"
    )
    .ok();
    writeln!(
        out,
        "                String msg = errPtr.equals(MemorySegment.NULL) ? \"registration failed (rc=\" + rc + \")\" : errPtr.reinterpret(Long.MAX_VALUE).getString(0);"
    )
    .ok();
    writeln!(
        out,
        "                throw new RuntimeException(\"register{}: \" + msg);",
        trait_pascal
    )
    .ok();
    writeln!(out, "            }}").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }} catch (Throwable t) {{").ok();
    writeln!(out, "        bridge.close();").ok();
    writeln!(out, "        throw t;").ok();
    writeln!(out, "    }}").ok();
    writeln!(
        out,
        "    {}_BRIDGES.put(impl.name(), bridge);",
        trait_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();

    writeln!(out, "/** Unregister a {} implementation. */", trait_pascal).ok();
    writeln!(
        out,
        "public static void unregister{}(String name) throws Exception {{",
        trait_pascal
    )
    .ok();
    writeln!(out, "    try (var nameArena = Arena.ofConfined()) {{").ok();
    writeln!(out, "        var nameCs = nameArena.allocateFrom(name);").ok();
    writeln!(out, "        var outErrArena = Arena.ofConfined();").ok();
    writeln!(
        out,
        "        MemorySegment outErr = outErrArena.allocate(ValueLayout.ADDRESS);"
    )
    .ok();
    writeln!(
        out,
        "        int rc = (int) NativeLib.{}_UNREGISTER_{}.invoke(nameCs, outErr);",
        prefix_upper,
        trait_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "        if (rc != 0) {{").ok();
    writeln!(
        out,
        "            MemorySegment errPtr = outErr.get(ValueLayout.ADDRESS, 0);"
    )
    .ok();
    writeln!(
        out,
        "            String msg = errPtr.equals(MemorySegment.NULL) ? \"unregistration failed (rc=\" + rc + \")\" : errPtr.reinterpret(Long.MAX_VALUE).getString(0);"
    )
    .ok();
    writeln!(
        out,
        "            throw new RuntimeException(\"unregister{}: \" + msg);",
        trait_pascal
    )
    .ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    // Close and remove the bridge after the C unregister call succeeds
    writeln!(
        out,
        "    {}Bridge old = {}_BRIDGES.remove(name);",
        trait_pascal,
        trait_snake.to_uppercase()
    )
    .ok();
    writeln!(out, "    if (old != null) {{ old.close(); }}").ok();
    writeln!(out, "}}").ok();

    out
}

/// Generate code to unmarshal a parameter from MemorySegment to Java type.
fn gen_param_unmarshal(out: &mut String, param_name: &str, param_type: &TypeRef) {
    use std::fmt::Write;
    match param_type {
        TypeRef::Primitive(_) => {
            // For primitives, assume the parameter is already the right type
            // (this is handled by the MethodType/FunctionDescriptor)
        }
        TypeRef::String | TypeRef::Path => {
            writeln!(out, "            String {} = {}.reinterpret(Long.MAX_VALUE).getString(0);", param_name, param_name).ok();
        }
        TypeRef::Bytes => {
            writeln!(out, "            byte[] {} = {}.reinterpret(Long.MAX_VALUE).toArray(ValueLayout.JAVA_BYTE);", param_name, param_name).ok();
        }
        TypeRef::Named(_) => {
            // For Named types, deserialize from JSON
            writeln!(out, "            String {}_json = {}.reinterpret(Long.MAX_VALUE).getString(0);", param_name, param_name).ok();
            writeln!(out, "            var {}_obj = new com.fasterxml.jackson.databind.ObjectMapper().readValue({}_json, Object.class);", param_name, param_name).ok();
            writeln!(out, "            Object {} = {}_obj;", param_name, param_name).ok();
        }
        _ => {
            // For Optional, Vec, Map, etc., deserialize from JSON
            writeln!(out, "            String {}_json = {}.reinterpret(Long.MAX_VALUE).getString(0);", param_name, param_name).ok();
            writeln!(out, "            var {}_obj = new com.fasterxml.jackson.databind.ObjectMapper().readValue({}_json, Object.class);", param_name, param_name).ok();
            writeln!(out, "            Object {} = {}_obj;", param_name, param_name).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::{MethodDef, ParamDef, TypeDef};

    fn make_test_trait(name: &str, methods: Vec<MethodDef>) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            rust_path: format!("test::{}", name),
            original_rust_path: format!("test::{}", name),
            fields: vec![],
            methods,
            is_opaque: false,
            is_clone: false,
            doc: String::new(),
            cfg: None,
            is_trait: true,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
        }
    }

    fn make_test_method(name: &str, return_type: TypeRef, params: Vec<ParamDef>) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            params,
            return_type,
            is_async: false,
            is_static: false,
            error_type: None,
            doc: String::new(),
            receiver: None,
            sanitized: false,
            trait_source: None,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
            has_default_impl: false,
        }
    }

    fn make_test_param(name: &str, ty: TypeRef) -> ParamDef {
        ParamDef {
            name: name.to_string(),
            ty,
            optional: false,
            default: None,
            sanitized: false,
            typed_default: None,
            is_ref: false,
            is_mut: false,
            newtype_wrapper: None,
            original_type: None,
        }
    }

    #[test]
    fn test_gen_trait_bridge_basic() {
        let trait_def = make_test_trait(
            "MyPlugin",
            vec![
                make_test_method("doWork", TypeRef::String, vec![]),
                make_test_method("getStatus", TypeRef::Primitive(alef_core::ir::PrimitiveType::I32), vec![]),
            ],
        );

        let code = gen_trait_bridge(&trait_def, "mylib", true);

        // Basic sanity checks
        assert!(code.contains("public interface IMyPlugin"));
        assert!(code.contains("String name()"));
        assert!(code.contains("String version()"));
        assert!(code.contains("void initialize()"));
        assert!(code.contains("void shutdown()"));
        assert!(code.contains("doWork"));
        assert!(code.contains("getStatus"));
        assert!(code.contains("MyPluginBridge"));
        assert!(code.contains("registerMyPlugin"));
        assert!(code.contains("unregisterMyPlugin"));
    }

    #[test]
    fn test_gen_trait_bridge_vtable_stubs() {
        let trait_def = make_test_trait("Handler", vec![]);

        let code = gen_trait_bridge(&trait_def, "lib", true);

        // Verify Panama FFM upcall stubs are generated
        assert!(code.contains("LINKER.upcallStub"));
        assert!(code.contains("handleName"));
        assert!(code.contains("handleVersion"));
        assert!(code.contains("handleInitialize"));
        assert!(code.contains("handleShutdown"));
    }

    #[test]
    fn test_gen_trait_bridge_method_with_params() {
        let trait_def = make_test_trait(
            "Processor",
            vec![make_test_method(
                "process",
                TypeRef::String,
                vec![
                    make_test_param("input", TypeRef::String),
                    make_test_param("count", TypeRef::Primitive(alef_core::ir::PrimitiveType::I32)),
                ],
            )],
        );

        let code = gen_trait_bridge(&trait_def, "pfx", true);

        // Verify method parameters are emitted
        assert!(code.contains("String input"));
        assert!(code.contains("int count"));
        assert!(code.contains("process"));
    }

    #[test]
    fn test_gen_trait_bridge_no_super_trait_omits_lifecycle() {
        let trait_def = make_test_trait(
            "Transformer",
            vec![make_test_method(
                "transform",
                TypeRef::String,
                vec![],
            )],
        );

        let code = gen_trait_bridge(&trait_def, "lib", false);

        // Without super_trait, no lifecycle slots should be emitted
        assert!(
            !code.contains("String name()"),
            "no lifecycle methods without super_trait"
        );
        assert!(
            !code.contains("handleName"),
            "no lifecycle handlers without super_trait"
        );
        assert!(code.contains("transform"), "trait method must still be emitted");
    }
}
