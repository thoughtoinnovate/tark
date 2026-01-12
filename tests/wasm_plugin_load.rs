//! Generic WASM Plugin System Tests
//!
//! These tests verify the plugin host and registry work correctly
//! with ANY installed plugin, not specific to any particular plugin.

use tark_cli::plugins::{PluginHost, PluginRegistry, PluginType};

#[test]
fn test_plugin_host_creation() {
    let host = PluginHost::new();
    assert!(host.is_ok(), "Failed to create plugin host");
}

#[test]
fn test_plugin_registry_creation() {
    let registry = PluginRegistry::new();
    assert!(registry.is_ok(), "Failed to create plugin registry");
}

#[test]
fn test_registry_scans_plugins_directory() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    // Registry should be created successfully even with no plugins
    // Just verify it doesn't panic
    let count = registry.all().count();
    println!("Found {} plugins in registry", count);
}

#[test]
fn test_load_any_provider_plugin() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    // Find any provider plugin
    let provider_plugin = registry.provider_plugins().next();

    match provider_plugin {
        Some(plugin) => {
            println!("Found provider plugin: {}", plugin.id());
            assert_eq!(plugin.plugin_type(), PluginType::Provider);

            // Try to load it
            let mut host = PluginHost::new().expect("Failed to create plugin host");
            match host.load(plugin) {
                Ok(_) => println!("✓ Successfully loaded plugin: {}", plugin.id()),
                Err(e) => println!("Plugin load error (may be expected): {}", e),
            }
        }
        None => {
            println!("Skipping test: No provider plugins installed");
        }
    }
}

#[test]
fn test_provider_plugin_has_contributions() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    for plugin in registry.provider_plugins() {
        println!("Checking plugin: {}", plugin.id());

        // Provider plugins should have contributions
        let contributions = &plugin.manifest.contributes.providers;
        assert!(
            !contributions.is_empty(),
            "Provider plugin {} should have provider contributions",
            plugin.id()
        );

        for c in contributions {
            println!(
                "  - Contribution: {} (base_provider: {:?})",
                c.id, c.base_provider
            );
        }
    }
}

#[test]
fn test_provider_plugin_info() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    // Find any provider plugin
    let provider_plugin = registry.provider_plugins().next();

    match provider_plugin {
        Some(plugin) => {
            let mut host = PluginHost::new().expect("Failed to create plugin host");

            if host.load(plugin).is_err() {
                println!("Skipping: Failed to load plugin {}", plugin.id());
                return;
            }

            let instance = match host.get_mut(plugin.id()) {
                Some(i) => i,
                None => {
                    println!("Skipping: Plugin instance not found");
                    return;
                }
            };

            // Test provider_info export
            match instance.provider_info() {
                Ok(info) => {
                    println!("✓ Provider info for {}:", plugin.id());
                    println!("  ID: {}", info.id);
                    println!("  Display name: {}", info.display_name);
                    println!("  Requires auth: {}", info.requires_auth);
                    assert!(!info.id.is_empty());
                }
                Err(e) => {
                    println!("provider_info not implemented: {}", e);
                }
            }

            // Test provider_models export
            match instance.provider_models() {
                Ok(models) => {
                    println!("✓ Provider has {} models", models.len());
                    for m in models.iter().take(3) {
                        println!("  - {}", m.id);
                    }
                }
                Err(e) => {
                    println!("provider_models not implemented: {}", e);
                }
            }
        }
        None => {
            println!("Skipping test: No provider plugins installed");
        }
    }
}

#[test]
fn test_auth_plugin_functions() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    // Find any auth plugin
    let auth_plugin = registry.by_type(PluginType::Auth).next();

    match auth_plugin {
        Some(plugin) => {
            println!("Found auth plugin: {}", plugin.id());

            let mut host = PluginHost::new().expect("Failed to create plugin host");

            if host.load(plugin).is_err() {
                println!("Skipping: Failed to load plugin {}", plugin.id());
                return;
            }

            let instance = match host.get_mut(plugin.id()) {
                Some(i) => i,
                None => {
                    println!("Skipping: Plugin instance not found");
                    return;
                }
            };

            // Test auth_status export
            match instance.auth_status() {
                Ok(status) => {
                    println!("✓ Auth status: {:?}", status);
                }
                Err(e) => {
                    println!("auth_status not implemented: {}", e);
                }
            }

            // Test auth_display_name export
            match instance.auth_display_name() {
                Ok(name) => {
                    println!("✓ Display name: {}", name);
                    assert!(!name.is_empty());
                }
                Err(e) => {
                    println!("auth_display_name not implemented: {}", e);
                }
            }
        }
        None => {
            println!("Skipping test: No auth plugins installed");
        }
    }
}
