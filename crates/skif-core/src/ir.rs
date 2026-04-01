use serde::{Deserialize, Serialize};

/// Complete API surface extracted from a Rust crate's public interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSurface {
    pub crate_name: String,
    pub version: String,
    pub types: Vec<TypeDef>,
    pub functions: Vec<FunctionDef>,
    pub enums: Vec<EnumDef>,
    pub errors: Vec<ErrorDef>,
}

/// A public struct exposed to bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub rust_path: String,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<MethodDef>,
    pub is_opaque: bool,
    pub is_clone: bool,
    pub doc: String,
}

/// A field on a public struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeRef,
    pub optional: bool,
    pub default: Option<String>,
    pub doc: String,
    /// True if this field's type was sanitized (e.g., Duration→u64, trait object→String).
    /// Fields marked sanitized cannot participate in auto-generated From/Into conversions.
    #[serde(default)]
    pub sanitized: bool,
}

/// A method on a public struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<ParamDef>,
    pub return_type: TypeRef,
    pub is_async: bool,
    pub is_static: bool,
    pub error_type: Option<String>,
    pub doc: String,
    pub receiver: Option<ReceiverKind>,
}

/// How `self` is received.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReceiverKind {
    Ref,
    RefMut,
    Owned,
}

/// A free function exposed to bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub rust_path: String,
    pub params: Vec<ParamDef>,
    pub return_type: TypeRef,
    pub is_async: bool,
    pub error_type: Option<String>,
    pub doc: String,
}

/// A function/method parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    pub ty: TypeRef,
    pub optional: bool,
    pub default: Option<String>,
}

/// A public enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    pub rust_path: String,
    pub variants: Vec<EnumVariant>,
    pub doc: String,
}

/// An enum variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub doc: String,
}

/// An error type (enum used in Result<T, E>).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDef {
    pub name: String,
    pub rust_path: String,
    pub variants: Vec<ErrorVariant>,
    pub doc: String,
}

/// An error variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorVariant {
    pub name: String,
    pub message: Option<String>,
    pub doc: String,
}

/// Reference to a type, with enough info for codegen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TypeRef {
    Primitive(PrimitiveType),
    String,
    Bytes,
    Optional(Box<TypeRef>),
    Vec(Box<TypeRef>),
    Map(Box<TypeRef>, Box<TypeRef>),
    Named(String),
    Path,
    Unit,
    Json,
}

/// Rust primitive types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrimitiveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Usize,
    Isize,
}
