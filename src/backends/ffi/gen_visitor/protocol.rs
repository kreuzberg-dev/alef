use crate::core::config::TraitBridgeConfig;
use crate::core::ir::ApiSurface;

#[derive(Clone)]
pub(super) struct VisitorProtocol {
    pub(super) context_type: String,
    pub(super) context_path: String,
    pub(super) result_type: String,
    pub(super) result_path: String,
}

impl VisitorProtocol {
    pub(super) fn from_api(api: &ApiSurface, bridge_cfg: &TraitBridgeConfig) -> Option<Self> {
        let Some(context_type) = bridge_cfg.context_type.as_deref() else {
            eprintln!(
                "[alef] gen_visitor(ffi): trait bridge `{}` must configure context_type for visitor callbacks",
                bridge_cfg.trait_name
            );
            return None;
        };
        let Some(result_type) = bridge_cfg.result_type.as_deref() else {
            eprintln!(
                "[alef] gen_visitor(ffi): trait bridge `{}` must configure result_type for visitor callbacks",
                bridge_cfg.trait_name
            );
            return None;
        };
        let Some(context_def) = api.types.iter().find(|type_def| type_def.name == context_type) else {
            eprintln!(
                "[alef] gen_visitor(ffi): trait bridge `{}` context_type `{context_type}` is not present in IR",
                bridge_cfg.trait_name
            );
            return None;
        };
        let Some(result_def) = api.enums.iter().find(|enum_def| enum_def.name == result_type) else {
            eprintln!(
                "[alef] gen_visitor(ffi): trait bridge `{}` result_type `{result_type}` is not present in IR",
                bridge_cfg.trait_name
            );
            return None;
        };
        Some(Self {
            context_type: context_type.to_string(),
            context_path: context_def.rust_path.replace('-', "_"),
            result_type: result_type.to_string(),
            result_path: result_def.rust_path.replace('-', "_"),
        })
    }

    pub(super) fn from_bridge_config(core_import: &str, bridge_cfg: Option<&TraitBridgeConfig>) -> Option<Self> {
        let bridge_cfg = bridge_cfg?;
        let context_type = bridge_cfg.context_type.as_deref()?;
        let result_type = bridge_cfg.result_type.as_deref()?;
        Some(Self {
            context_type: context_type.to_string(),
            context_path: format!("{core_import}::{context_type}"),
            result_type: result_type.to_string(),
            result_path: format!("{core_import}::{result_type}"),
        })
    }
}
