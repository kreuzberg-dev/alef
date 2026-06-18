use std::collections::{HashMap, HashSet};

/// Resolves fixture field paths to language-specific accessor expressions.
#[derive(Clone)]
pub struct FieldResolver {
    pub(super) aliases: HashMap<String, String>,
    pub(super) optional_fields: HashSet<String>,
    pub(super) result_fields: HashSet<String>,
    pub(super) array_fields: HashSet<String>,
    pub(super) method_calls: HashSet<String>,
    /// Fields whose `Option<T>` inner type is a display/content union (e.g. `RichTextContent`)
    /// rather than a plain `String`. Language generators that would otherwise emit
    /// `string(*ptr)` (Go) or `Objects::toString()` (Java) for such fields will instead
    /// call the language-idiomatic text accessor (`.Text()` in Go/Java/C#, `.text()` in PHP)
    /// so the assertion compares the textual representation, not an opaque object address.
    ///
    /// Populated from `fields_display_as_text` in `alef.toml`.
    pub(super) display_as_text_fields: HashSet<String>,
    /// Aliases for error-path field access (used when assertion_type == "error").
    /// Maps fixture sub-field names (the part after "error.") to actual field names
    /// on the error type. E.g., `"status_code" -> "status_code"`.
    pub(super) error_field_aliases: HashMap<String, String>,
    /// Per-type PHP getter classification: maps an owner type's snake_case field
    /// name to whether THAT field on THAT type requires `->getCamelCase()` syntax
    /// (because the field's mapped PHP type is non-scalar and ext-php-rs emits a
    /// `#[php(getter)]` method) rather than `->camelCase` property access.
    /// Populated by `new_with_php_getters`; empty by default.
    ///
    /// Keying by (type, field) — not bare field name — is required because two
    /// different types can declare the same field name with different scalarness
    /// (e.g. `CrawlConfig.content: ContentConfig` is non-scalar while
    /// `MarkdownResult.content: String` is scalar).
    pub(super) php_getter_map: PhpGetterMap,
    /// Per-type Swift first-class/opaque classification, populated by the
    /// Swift e2e codegen. When non-empty, `accessor` uses
    /// `render_swift_with_first_class_map` instead of the legacy property-only
    /// `render_swift_with_optionals`, so paths that traverse from first-class
    /// types (property access) into opaque typealias types (method-call access)
    /// pick the correct syntax at each segment.
    pub(super) swift_first_class_map: SwiftFirstClassMap,
    /// Per-type Dart stringy field classification, populated by the Dart e2e
    /// codegen. Used to aggregate every readable text accessor on a `Vec<T>`
    /// element type for `contains` assertions.
    pub(super) dart_first_class_map: DartFirstClassMap,
}

/// Per-type PHP getter classification + chain-resolution metadata.
///
/// Holds enough information to resolve a multi-segment field path through the
/// IR's nested type graph and pick the correct accessor style at each segment:
///
/// * `getters[type_name]` — set of field names on `type_name` whose PHP binding
///   uses a `#[php(getter)]` method (caller must emit `->getCamelCase()`).
/// * `field_types[type_name][field_name]` — the IR-resolved `Named` type that
///   `field_name` traverses into, used to advance the "current type" cursor
///   for the next path segment. Absent for terminal/scalar fields.
/// * `root_type` — the IR type name backing the result variable at the start of
///   any chain. When `None`, chain traversal degrades to per-segment lookup
///   using a flattened union across all types (legacy bare-name behaviour),
///   which produces false positives when field names collide across types.
#[derive(Debug, Clone, Default)]
pub struct PhpGetterMap {
    pub getters: HashMap<String, HashSet<String>>,
    pub field_types: HashMap<String, HashMap<String, String>>,
    pub root_type: Option<String>,
    /// All field names per type — used to detect when the recorded `root_type`
    /// is a misclassification (a workspace-global root_type may not match the
    /// actual return type of a per-fixture call). When `owner_type` is set but
    /// `all_fields[owner_type]` doesn't contain `field_name`, the renderer
    /// falls back to the bare-name union instead of trusting the (wrong) owner.
    pub all_fields: HashMap<String, HashSet<String>>,
}

/// Swift first-class struct classification + chain-resolution metadata.
///
/// alef-backend-swift emits two flavors of binding types:
///
/// * **First-class Codable structs** — `public struct Foo: Codable { public let id: String }`.
///   Fields are Swift properties; access with `.id` (no parens).
/// * **Opaque typealiases** — `public typealias Foo = RustBridge.Foo` where the
///   RustBridge class exposes swift-bridge methods. Fields are methods;
///   access with `.id()` (parens).
///
/// The renderer needs per-segment dispatch because a path can traverse both:
/// e.g. `BatchListResponse` (first-class Codable, with `data: [BatchObject]`) →
/// indexed `[0]` → `BatchObject` (opaque typealias). At the `BatchObject` cursor
/// the renderer must switch to method-call access for `.id`, `.status`, etc.
///
/// * `first_class_types` — set of TypeDef names whose binding is a first-class
///   Codable struct. Membership = "use property access for fields on this type".
/// * `field_types[type_name][field_name]` — the IR-resolved `Named` type that
///   `field_name` traverses into.
/// * `vec_field_names` — flat set of field names whose IR type is `Vec<T>` on
///   any owner. Used by swift_count_target to keep `.count` straight on
///   RustVec-typed method-call accessors (don't inject `.toString()`).
/// * `root_type` — the IR type name backing the result variable.
///
/// Kind of a "stringy" field on an opaque DTO element type — used by the swift
/// e2e `contains` assertion to aggregate every readable text accessor on a
/// `Vec<T>` element instead of relying on a single primary accessor (which
/// often guesses wrong: e.g. `ImportInfo.source` is the module path but
/// `ImportInfo.items` carries the imported names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringyFieldKind {
    /// `field_name() -> RustString` (or `String`). Convert via `.toString()`.
    Plain,
    /// `field_name() -> Optional<RustString>`. Convert via `?.toString() ?? ""`.
    Optional,
    /// `field_name() -> RustVec<RustString>`. Iterate elements (RustStringRef
    /// → `.asStr().toString()` on each).
    Vec,
}

/// A single readable text accessor on an opaque DTO. The `name` is the Rust
/// field name (snake_case), used to derive the swift-bridge lowerCamelCase
/// method call.
#[derive(Debug, Clone)]
pub struct StringyField {
    pub name: String,
    pub kind: StringyFieldKind,
}

#[derive(Debug, Clone, Default)]
pub struct SwiftFirstClassMap {
    pub first_class_types: HashSet<String>,
    pub field_types: HashMap<String, HashMap<String, String>>,
    pub vec_field_names: HashSet<String>,
    pub root_type: Option<String>,
    /// Per-type readable text accessors. Keyed by IR TypeDef name. Used by the
    /// swift e2e `contains` assertion to aggregate every stringy field on a
    /// `Vec<T>` element type into a `contains(where: { ... })` closure that
    /// does substring matching against every text-bearing accessor. Mirrors
    /// python's `_alef_e2e_item_texts` helper.
    pub stringy_fields_by_type: HashMap<String, Vec<StringyField>>,
}

impl SwiftFirstClassMap {
    /// Returns true when fields on `type_name` should be accessed as properties
    /// (no parens), false when they should be accessed via method-call.
    ///
    /// When `type_name` is `None` the renderer defaults to method-call syntax —
    /// opaque swift-bridge types (with `.field()` methods) are the common case
    /// for unknown roots. Defaulting to `true` (property syntax) caused the
    /// e2e generator to emit `result.content` instead of `result.content()` for
    /// opaque `ExtractionResult` and similar types whose IR root type was not
    /// resolved by `swift_call_result_type`, producing a Swift compile error:
    /// "value of type '@Sendable () -> RustString' has no member 'contains'".
    pub fn is_first_class(&self, type_name: Option<&str>) -> bool {
        match type_name {
            Some(t) => self.first_class_types.contains(t),
            None => false,
        }
    }

    /// Returns the IR `Named` type that `field_name` traverses into for the
    /// next chain segment, or `None` if the field is terminal/scalar/unknown.
    pub fn advance(&self, owner_type: Option<&str>, field_name: &str) -> Option<String> {
        let owner = owner_type?;
        self.field_types.get(owner).and_then(|m| m.get(field_name).cloned())
    }

    /// True when `field_name` appears as a `Vec<T>` (or `Option<Vec<T>>`) on
    /// any IR type. swift codegen consults this when deciding whether `.count`
    /// on a method-call accessor needs `.toString()` injected: RustVec already
    /// supports `.count` directly; RustString does not.
    pub fn is_vec_field_name(&self, field_name: &str) -> bool {
        self.vec_field_names.contains(field_name)
    }

    /// True when no per-type information is recorded.
    pub fn is_empty(&self) -> bool {
        self.first_class_types.is_empty() && self.field_types.is_empty()
    }

    /// Returns the list of stringy accessors recorded for `type_name`, or
    /// `None` if the type has no recorded stringy fields.
    pub fn stringy_fields(&self, type_name: &str) -> Option<&[StringyField]> {
        self.stringy_fields_by_type.get(type_name).map(Vec::as_slice)
    }
}

/// Dart opaque type classification + chain-resolution metadata, mirroring
/// Swift's needs to track stringy field accessors on element types for
/// `Vec<T>` contains assertions. Unlike Swift, Dart doesn't distinguish
/// first-class vs opaque; we just track stringy fields per type.
#[derive(Debug, Clone, Default)]
pub struct DartFirstClassMap {
    pub field_types: HashMap<String, HashMap<String, String>>,
    pub root_type: Option<String>,
    /// Per-type readable text accessors. Used by the dart e2e `contains`
    /// assertion to aggregate every stringy field on a `Vec<T>` element type.
    pub stringy_fields_by_type: HashMap<String, Vec<StringyField>>,
}

impl DartFirstClassMap {
    /// Returns the IR `Named` type that `field_name` traverses into for the
    /// next chain segment, or `None` if the field is terminal/scalar/unknown.
    pub fn advance(&self, owner_type: Option<&str>, field_name: &str) -> Option<String> {
        let owner = owner_type?;
        self.field_types.get(owner).and_then(|m| m.get(field_name).cloned())
    }

    /// Returns the list of stringy accessors recorded for `type_name`, or
    /// `None` if the type has no recorded stringy fields.
    pub fn stringy_fields(&self, type_name: &str) -> Option<&[StringyField]> {
        self.stringy_fields_by_type.get(type_name).map(Vec::as_slice)
    }

    /// True when no per-type information is recorded.
    pub fn is_empty(&self) -> bool {
        self.field_types.is_empty() && self.stringy_fields_by_type.is_empty()
    }
}

impl PhpGetterMap {
    /// Returns true if `(owner_type, field_name)` requires getter-method syntax.
    ///
    /// When `owner_type` is `None` (root type unknown, or chain advanced into an
    /// unmapped type), falls back to the union across all types: any type
    /// declaring `field_name` as non-scalar marks it as needing a getter. This
    /// is the legacy behaviour and is unsafe when field names collide.
    pub fn needs_getter(&self, owner_type: Option<&str>, field_name: &str) -> bool {
        if let Some(t) = owner_type {
            // Only trust the owner-type classification if the type actually declares
            // this field. A misclassified root_type (workspace-global guess that
            // doesn't match the per-fixture call's actual return type) shouldn't
            // shadow the bare-name fallback.
            let owner_has_field = self.all_fields.get(t).is_some_and(|s| s.contains(field_name));
            if owner_has_field {
                // The owner declares this field — the per-type `getters` map is
                // the authoritative answer. Returning early here prevents the
                // global bare-name union (below) from flipping a scalar field
                // (e.g. `ProcessingResult.content: String`) into a getter call
                // just because some unrelated type declares a same-named field
                // as non-scalar (e.g. `Chunk.content: Vec<Span>`).
                return self.getters.get(t).is_some_and(|fields| fields.contains(field_name));
            }
        }
        self.getters.values().any(|set| set.contains(field_name))
    }

    /// Returns the IR `Named` type that `field_name` traverses into for the
    /// next chain segment, or `None` if the field is terminal/scalar/unknown.
    pub fn advance(&self, owner_type: Option<&str>, field_name: &str) -> Option<String> {
        let owner = owner_type?;
        self.field_types.get(owner).and_then(|m| m.get(field_name).cloned())
    }

    /// True when no per-type information is recorded — equivalent to the legacy
    /// "no PHP getter resolution" code path.
    pub fn is_empty(&self) -> bool {
        self.getters.is_empty()
    }
}

/// A parsed segment of a field path.
#[derive(Debug, Clone)]
pub(super) enum PathSegment {
    /// Struct field access: `foo`
    Field(String),
    /// Array field access with explicit numeric index: `foo[N]`
    ///
    /// The `index` is the integer parsed from the bracket (e.g. `choices[2]` → index 2).
    /// When synthesised by `inject_array_indexing` the index defaults to `0`.
    ArrayField { name: String, index: usize },
    /// Map/dict key access: `foo[key]`
    MapAccess { field: String, key: String },
    /// Length/count of the preceding collection: `.length`
    Length,
}
