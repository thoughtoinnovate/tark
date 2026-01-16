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

#[tokio::test]
async fn test_model_picker_models_for_gemini_oauth() {
    use tark_cli::llm::models_db;
    use tark_cli::plugins::PluginRegistry;

    println!("\n=== Testing Model Picker Models ===\n");

    // 1. Check plugin
    let registry = PluginRegistry::new().expect("Failed to create registry");

    for plugin in registry.provider_plugins() {
        println!("Plugin: {}", plugin.id());
        for c in &plugin.manifest.contributes.providers {
            println!("  base_provider: {:?}", c.base_provider);

            if let Some(base) = &c.base_provider {
                println!("\n  Checking models.dev for '{}':", base);

                // Check cache
                let db = models_db();
                if let Some(models) = db.try_get_cached(base) {
                    println!("  Cache hit: {} models", models.len());
                    for m in models.iter().take(5) {
                        println!("    - {}", m.id);
                    }
                } else {
                    println!("  Cache miss - fetching from models.dev...");

                    // Fetch directly (not preload which is background)
                    match db.list_models(base).await {
                        Ok(models) => {
                            println!("  Fetched {} models:", models.len());
                            for m in models.iter().take(10) {
                                println!("    - {}: {}", m.id, m.name);
                            }
                        }
                        Err(e) => {
                            println!("  Fetch error: {}", e);
                        }
                    }
                }
            }
        }
    }

    println!("\n=== Done ===");
}

#[test]
fn test_get_plugin_provider_models_directly() {
    use tark_cli::llm::models_db;
    use tark_cli::plugins::PluginRegistry;

    println!("\n=== Simulating TUI model picker flow ===\n");

    let provider_id = "gemini-oauth";
    println!("1. Provider ID: {}", provider_id);

    // Step 1: Create registry
    let registry = match PluginRegistry::new() {
        Ok(r) => {
            println!("2. PluginRegistry created successfully");
            r
        }
        Err(e) => {
            println!("2. ERROR creating PluginRegistry: {}", e);
            panic!("Registry creation failed");
        }
    };

    // Step 2: Get provider plugins
    let plugins: Vec<_> = registry.provider_plugins().collect();
    println!("3. Found {} provider plugins", plugins.len());

    // Skip test if no provider plugins are installed
    if plugins.is_empty() {
        println!("Skipping test: No provider plugins installed");
        println!("\n=== Test Skipped ===");
        return;
    }

    // Step 3: Find matching plugin
    let mut found = false;
    for plugin in plugins {
        println!(
            "   - Plugin: {} (comparing to {})",
            plugin.id(),
            provider_id
        );
        if plugin.id() == provider_id {
            println!("4. MATCH FOUND for {}", plugin.id());
            found = true;

            // Check contributions
            for contribution in &plugin.manifest.contributes.providers {
                println!("   - Contribution ID: {}", contribution.id);
                println!("   - base_provider: {:?}", contribution.base_provider);

                if let Some(base_provider) = &contribution.base_provider {
                    println!("5. Getting models for base_provider: {}", base_provider);

                    let db = models_db();

                    // Try cache
                    if let Some(models) = db.try_get_cached(base_provider) {
                        println!("   Cache hit: {} models", models.len());
                    } else {
                        println!("   Cache miss, fetching via separate thread...");

                        // Use std::thread to create a new tokio runtime (like the TUI does)
                        let base_owned = base_provider.to_string();
                        let fetch_result = std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().ok()?;
                            rt.block_on(async {
                                let db = tark_cli::llm::models_db();
                                db.list_models(&base_owned).await.ok()
                            })
                        })
                        .join()
                        .ok()
                        .flatten();

                        match fetch_result {
                            Some(models) => {
                                println!("6. Fetched {} models:", models.len());
                                for m in models.iter().take(5) {
                                    println!("      - {}: {}", m.id, m.name);
                                }
                                assert!(!models.is_empty(), "Should have models");
                            }
                            None => {
                                println!("6. ERROR fetching models");
                            }
                        }
                    }
                } else {
                    println!("5. No base_provider, would use fallback");
                }
            }
            break;
        }
    }

    if !found {
        // Not an error - plugin may not be installed in CI
        println!("4. Plugin {} not found (may not be installed)", provider_id);
        println!("Skipping test: gemini-oauth plugin not installed");
        println!("\n=== Test Skipped ===");
        return;
    }

    println!("\n=== Test Complete ===");
}
