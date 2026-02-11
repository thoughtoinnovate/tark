use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const EDITOR_ADAPTER_API_V1: &str = "v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorContextV1 {
    pub adapter_id: String,
    pub adapter_version: String,
    #[serde(default = "default_api_version")]
    pub api_version: String,
    pub endpoint: EditorEndpoint,
    #[serde(default)]
    pub capabilities: EditorCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorEndpoint {
    pub base_url: String,
    #[serde(default)]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorCapabilities {
    #[serde(default)]
    pub definition: bool,
    #[serde(default)]
    pub references: bool,
    #[serde(default)]
    pub hover: bool,
    #[serde(default)]
    pub symbols: bool,
    #[serde(default)]
    pub diagnostics: bool,
    #[serde(default)]
    pub open_file: bool,
    #[serde(default)]
    pub cursor: bool,
    #[serde(default)]
    pub buffers: bool,
    #[serde(default)]
    pub buffer_content: bool,
}

fn default_api_version() -> String {
    EDITOR_ADAPTER_API_V1.to_string()
}

impl EditorContextV1 {
    pub fn validate_api_version(&self) -> Result<()> {
        if self.api_version == EDITOR_ADAPTER_API_V1 {
            Ok(())
        } else {
            anyhow::bail!(
                "Unsupported editor adapter api_version '{}'. Supported versions: {}",
                self.api_version,
                EDITOR_ADAPTER_API_V1
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_defaults_api_version() {
        let json = r#"{
            "adapter_id": "tark.nvim",
            "adapter_version": "0.11.4",
            "endpoint": {"base_url": "http://127.0.0.1:8787"},
            "capabilities": {"definition": true}
        }"#;

        let parsed: EditorContextV1 = serde_json::from_str(json).expect("parse context");
        assert_eq!(parsed.api_version, EDITOR_ADAPTER_API_V1);
        assert!(parsed.validate_api_version().is_ok());
    }

    #[test]
    fn context_rejects_unsupported_api_version() {
        let ctx = EditorContextV1 {
            adapter_id: "tark.nvim".to_string(),
            adapter_version: "0.11.4".to_string(),
            api_version: "v2".to_string(),
            endpoint: EditorEndpoint {
                base_url: "http://127.0.0.1:8787".to_string(),
                auth_token: None,
            },
            capabilities: EditorCapabilities::default(),
        };

        assert!(ctx.validate_api_version().is_err());
    }
}
