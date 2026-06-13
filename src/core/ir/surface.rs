use serde::{Deserialize, Serialize};

use super::items::{EnumDef, ErrorDef, FunctionDef, TypeDef};
use super::service::{HandlerContractDef, ServiceDef};

/// Complete API surface extracted from a Rust crate's public interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiSurface {
    pub crate_name: String,
    pub version: String,
    pub types: Vec<TypeDef>,
    pub functions: Vec<FunctionDef>,
    pub enums: Vec<EnumDef>,
    pub errors: Vec<ErrorDef>,
    #[serde(default)]
    pub excluded_type_paths: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub excluded_trait_names: std::collections::HashSet<String>,
    #[serde(default)]
    pub services: Vec<ServiceDef>,
    #[serde(default)]
    pub handler_contracts: Vec<HandlerContractDef>,
    #[serde(default)]
    pub unsupported_public_items: Vec<UnsupportedPublicItem>,
}

/// A public item that was discovered but not extracted into binding IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsupportedPublicItem {
    pub item_kind: String,
    pub item_path: String,
    pub reason: String,
    pub suggested_fix: String,
}
