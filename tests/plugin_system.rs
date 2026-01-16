//! Integration tests for the plugin system

use tark_cli::plugins::{PluginCapabilities, PluginManifest, PluginRegistry, PluginType};
use tempfile::TempDir;

#[test]
fn test_manifest_parsing() {
    let toml = r#"
[plugin]
name = "test-plugin"
version = "1.0.0"
type = "auth"
description = "Test plugin"

[capabilities]
storage = true
http = ["api.example.com"]
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert_eq!(manifest.plugin.name, "test-plugin");
    assert_eq!(manifest.plugin_type(), PluginType::Auth);
    assert!(manifest.capabilities.storage);
}

#[test]
fn test_manifest_all_plugin_types() {
    let types = [
        ("auth", PluginType::Auth),
        ("tool", PluginType::Tool),
        ("provider", PluginType::Provider),
        ("hook", PluginType::Hook),
    ];

    for (type_str, expected_type) in types {
        let toml = format!(
            r#"
[plugin]
name = "test-{}"
version = "1.0.0"
type = "{}"
"#,
            type_str, type_str
        );

        let manifest: PluginManifest = toml::from_str(&toml).unwrap();
        assert_eq!(manifest.plugin_type(), expected_type);
    }
}

#[test]
fn test_manifest_validation_empty_name() {
    let toml = r#"
[plugin]
name = ""
version = "1.0.0"
type = "auth"
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert!(manifest.validate().is_err());
}

#[test]
fn test_manifest_validation_invalid_version() {
    let toml = r#"
[plugin]
name = "test-plugin"
version = "not-semver"
type = "auth"
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert!(manifest.validate().is_err());
}

#[test]
fn test_manifest_validation_invalid_name_chars() {
    let toml = r#"
[plugin]
name = "test plugin with spaces"
version = "1.0.0"
type = "auth"
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert!(manifest.validate().is_err());
}

#[test]
fn test_capabilities_http_wildcards() {
    let caps = PluginCapabilities {
        http: vec!["*.googleapis.com".to_string()],
        ..Default::default()
    };

    assert!(caps.is_http_allowed("oauth2.googleapis.com"));
    assert!(caps.is_http_allowed("generativelanguage.googleapis.com"));
    assert!(!caps.is_http_allowed("googleapis.com.evil.com"));
    assert!(!caps.is_http_allowed("evil.com"));
}

#[test]
fn test_capabilities_http_exact_match() {
    let caps = PluginCapabilities {
        http: vec!["api.example.com".to_string()],
        ..Default::default()
    };

    assert!(caps.is_http_allowed("api.example.com"));
    assert!(!caps.is_http_allowed("other.example.com"));
    assert!(!caps.is_http_allowed("api.example.com.evil.com"));
}

#[test]
fn test_capabilities_env_wildcards() {
    let caps = PluginCapabilities {
        env: vec!["GEMINI_*".to_string(), "GOOGLE_API_KEY".to_string()],
        ..Default::default()
    };

    assert!(caps.is_env_allowed("GEMINI_API_KEY"));
    assert!(caps.is_env_allowed("GEMINI_PROJECT_ID"));
    assert!(caps.is_env_allowed("GOOGLE_API_KEY"));
    assert!(!caps.is_env_allowed("OPENAI_API_KEY"));
}

#[test]
fn test_registry_empty() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("XDG_DATA_HOME", temp_dir.path());

    let registry = PluginRegistry::new().unwrap();
    assert_eq!(registry.all().count(), 0);
}

#[test]
fn test_plugin_type_display() {
    assert_eq!(PluginType::Auth.to_string(), "auth");
    assert_eq!(PluginType::Tool.to_string(), "tool");
    assert_eq!(PluginType::Provider.to_string(), "provider");
    assert_eq!(PluginType::Hook.to_string(), "hook");
}

#[test]
fn test_manifest_default_wasm_file() {
    let toml = r#"
[plugin]
name = "test-plugin"
version = "1.0.0"
type = "auth"
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert_eq!(manifest.plugin.wasm, "plugin.wasm");
}

#[test]
fn test_manifest_custom_wasm_file() {
    let toml = r#"
[plugin]
name = "test-plugin"
version = "1.0.0"
type = "auth"
wasm = "custom.wasm"
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert_eq!(manifest.plugin.wasm, "custom.wasm");
}

#[test]
fn test_manifest_full_metadata() {
    let toml = r#"
[plugin]
name = "full-plugin"
version = "2.0.0"
type = "tool"
description = "A fully featured plugin"
author = "Test Author"
homepage = "https://example.com"
license = "MIT"
min_tark_version = "0.5.0"

[capabilities]
storage = true
http = ["api.example.com", "*.googleapis.com"]
env = ["API_KEY", "SECRET_*"]
shell = false
filesystem = ["./data", "./config"]
"#;

    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert_eq!(manifest.plugin.name, "full-plugin");
    assert_eq!(manifest.plugin.version, "2.0.0");
    assert_eq!(manifest.plugin_type(), PluginType::Tool);
    assert_eq!(manifest.plugin.description, "A fully featured plugin");
    assert_eq!(manifest.plugin.author, "Test Author");
    assert_eq!(manifest.plugin.homepage, "https://example.com");
    assert_eq!(manifest.plugin.license, "MIT");
    assert_eq!(manifest.plugin.min_tark_version, Some("0.5.0".to_string()));

    assert!(manifest.capabilities.storage);
    assert_eq!(manifest.capabilities.http.len(), 2);
    assert_eq!(manifest.capabilities.env.len(), 2);
    assert!(!manifest.capabilities.shell);
    assert_eq!(manifest.capabilities.filesystem.len(), 2);
}
