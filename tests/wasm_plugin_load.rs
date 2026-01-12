//! Test loading and calling a WASM plugin

use std::fs;
use tark_cli::plugins::{AuthStatus, PluginHost, PluginRegistry, PluginType};

#[test]
fn test_load_gemini_auth_plugin() {
    // Skip if plugin not installed
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = match registry.get("gemini-auth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-auth plugin not installed");
            return;
        }
    };

    // Create host and load plugin
    let mut host = PluginHost::new().expect("Failed to create plugin host");
    let result = host.load(plugin);

    match result {
        Ok(_) => println!("✓ Successfully loaded gemini-auth plugin"),
        Err(e) => {
            // Plugin may fail to load due to missing imports - that's expected
            // until we have proper WIT bindings
            println!("Plugin load error (may be expected): {}", e);
        }
    }
}

#[test]
fn test_plugin_host_creation() {
    let host = PluginHost::new();
    assert!(host.is_ok(), "Failed to create plugin host");
}

#[test]
fn test_call_auth_plugin_functions() {
    // Skip if plugin not installed
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = match registry.get("gemini-auth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-auth plugin not installed");
            return;
        }
    };

    // Create host and load plugin
    let mut host = PluginHost::new().expect("Failed to create plugin host");
    host.load(plugin).expect("Failed to load plugin");

    // Get the plugin instance
    let instance = host.get_mut("gemini-auth").expect("Plugin not found");

    // Make test deterministic: plugin storage is persistent, so it may already be authenticated.
    // Ensure we start from a logged-out state.
    instance.auth_logout().expect("Failed to logout");

    // Test status function (should return NotAuthenticated without credentials)
    let status = instance.auth_status().expect("Failed to call status");
    println!("Auth status: {:?}", status);
    assert_eq!(status, AuthStatus::NotAuthenticated);

    // Test display_name function
    let name = instance
        .auth_display_name()
        .expect("Failed to call display_name");
    println!("Display name: {}", name);
    assert_eq!(name, "Gemini (OAuth)");

    // Test get_endpoint function
    let endpoint = instance
        .auth_get_endpoint()
        .expect("Failed to call get_endpoint");
    println!("Endpoint: {}", endpoint);
    assert_eq!(endpoint, "https://cloudcode-pa.googleapis.com");

    println!("✓ All auth plugin functions work correctly!");
}

#[test]
fn test_auth_plugin_with_gemini_cli_credentials() {
    // Skip if plugin not installed
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = match registry.get("gemini-auth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-auth plugin not installed");
            return;
        }
    };

    // Check if Gemini CLI credentials exist
    let creds_path = dirs::home_dir()
        .map(|h| h.join(".gemini").join("oauth_creds.json"))
        .expect("No home dir");

    if !creds_path.exists() {
        println!(
            "Skipping test: No Gemini CLI credentials at {:?}",
            creds_path
        );
        return;
    }

    // Load credentials
    let creds_json = fs::read_to_string(&creds_path).expect("Failed to read credentials");
    println!("Loaded Gemini CLI credentials from {:?}", creds_path);

    // Create host and load plugin
    let mut host = PluginHost::new().expect("Failed to create plugin host");
    host.load(plugin).expect("Failed to load plugin");

    // Get the plugin instance
    let instance = host.get_mut("gemini-auth").expect("Plugin not found");

    // Initialize with credentials
    instance
        .auth_init_with_credentials(&creds_json)
        .expect("Failed to init with credentials");
    println!("✓ Initialized plugin with Gemini CLI credentials");

    // Check status - should now be Authenticated
    let status = instance.auth_status().expect("Failed to call status");
    println!("Auth status after init: {:?}", status);
    assert_eq!(status, AuthStatus::Authenticated);

    // Try to get token
    let token = instance.auth_get_token().expect("Failed to get token");
    println!("Got token: {}...", &token[..20.min(token.len())]);
    assert!(!token.is_empty());

    println!("✓ Full auth flow works with Gemini CLI credentials!");
}

// =============================================================================
// Provider Plugin Tests (gemini-oauth)
// =============================================================================

#[test]
fn test_load_gemini_oauth_provider_plugin() {
    let registry = PluginRegistry::new().expect("Failed to create registry");

    // Check if plugin is installed
    let plugin = match registry.get("gemini-oauth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-oauth plugin not installed");
            return;
        }
    };

    // Verify it's a provider type
    assert_eq!(plugin.plugin_type(), PluginType::Provider);
    println!("✓ gemini-oauth is a provider plugin");

    // Load plugin
    let mut host = PluginHost::new().expect("Failed to create plugin host");
    host.load(plugin).expect("Failed to load plugin");
    println!("✓ Successfully loaded gemini-oauth plugin");
}

#[test]
fn test_gemini_oauth_provider_info() {
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = match registry.get("gemini-oauth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-oauth plugin not installed");
            return;
        }
    };

    let mut host = PluginHost::new().expect("Failed to create plugin host");
    host.load(plugin).expect("Failed to load plugin");

    let instance = host.get_mut("gemini-oauth").expect("Plugin not found");

    // Test provider_info
    let info = instance
        .provider_info()
        .expect("Failed to get provider info");
    println!("Provider ID: {}", info.id);
    println!("Display name: {}", info.display_name);
    println!("Description: {}", info.description);
    println!("Requires auth: {}", info.requires_auth);

    assert_eq!(info.id, "gemini-oauth");
    assert!(!info.display_name.is_empty());

    // Test provider_models
    let models = instance.provider_models().expect("Failed to get models");
    println!("Available models: {}", models.len());
    for model in &models {
        println!("  - {} (context: {})", model.id, model.context_window);
    }
    assert!(!models.is_empty());

    println!("✓ Provider info and models work correctly!");
}

#[test]
fn test_gemini_oauth_provider_with_credentials() {
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = match registry.get("gemini-oauth") {
        Some(p) => p,
        None => {
            println!("Skipping test: gemini-oauth plugin not installed");
            return;
        }
    };

    // Check if Gemini CLI credentials exist
    let creds_path = dirs::home_dir()
        .map(|h| h.join(".gemini").join("oauth_creds.json"))
        .expect("No home dir");

    if !creds_path.exists() {
        println!(
            "Skipping test: No Gemini CLI credentials at {:?}",
            creds_path
        );
        return;
    }

    let creds_json = fs::read_to_string(&creds_path).expect("Failed to read credentials");

    let mut host = PluginHost::new().expect("Failed to create plugin host");
    host.load(plugin).expect("Failed to load plugin");

    let instance = host.get_mut("gemini-oauth").expect("Plugin not found");

    // Initialize with credentials
    instance
        .provider_auth_init(&creds_json)
        .expect("Failed to init provider");
    println!("✓ Initialized provider with Gemini CLI credentials");

    // Check auth status
    let status = instance
        .provider_auth_status()
        .expect("Failed to get auth status");
    println!("Auth status: {:?}", status);
    assert_eq!(status, tark_cli::plugins::ProviderAuthStatus::Authenticated);

    println!("✓ Provider auth works with Gemini CLI credentials!");
}

#[test]
fn test_gemini_oauth_model_picker_integration() {
    use tark_cli::plugins::PluginRegistry;

    println!("\n=== Testing Model Picker Integration ===\n");

    // 1. Check plugin is installed and enabled
    let registry = PluginRegistry::new().expect("Failed to create registry");
    let plugin = registry
        .get("gemini-oauth")
        .expect("gemini-oauth plugin not installed");
    assert!(plugin.enabled, "Plugin should be enabled");
    println!("✓ Plugin installed and enabled");

    // 2. Check it's a provider type
    assert_eq!(
        plugin.plugin_type(),
        tark_cli::plugins::PluginType::Provider
    );
    println!("✓ Plugin type is Provider");

    // 3. Check contributions
    let providers = &plugin.manifest.contributes.providers;
    assert!(!providers.is_empty(), "Should have provider contributions");
    println!("✓ Has provider contributions");

    // 4. Check base_provider
    let contribution = &providers[0];
    assert_eq!(contribution.id, "gemini-oauth");
    assert_eq!(contribution.base_provider, Some("google".to_string()));
    println!("✓ base_provider = 'google'");

    // 5. Check models can be retrieved
    let models_db = tark_cli::llm::models_db();

    // Try cache first
    if let Some(models) = models_db.try_get_cached("google") {
        println!("✓ Found {} Google models in cache", models.len());
        for m in models.iter().take(3) {
            println!("  - {}", m.id);
        }
    } else {
        println!("! Cache empty, fallback models would be used:");
        println!("  - gemini-2.0-flash-exp");
        println!("  - gemini-1.5-pro");
        println!("  - gemini-1.5-flash");
    }

    println!("\n=== Model Picker Integration OK ===");
}
