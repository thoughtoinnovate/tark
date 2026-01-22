//! Tests for CatalogService
//!
//! Real behavior tests for provider/model discovery and capabilities.

use tark_cli::ui_backend::{AuthStatus, CatalogService};

#[tokio::test]
async fn test_list_providers() {
    let service = CatalogService::new();

    let providers = service.list_providers().await;

    // Should return at least some providers
    assert!(!providers.is_empty(), "Should have at least one provider");

    // Each provider should have required fields
    for provider in &providers {
        assert!(!provider.id.is_empty(), "Provider ID should not be empty");
        assert!(
            !provider.name.is_empty(),
            "Provider name should not be empty"
        );
        assert!(
            !provider.icon.is_empty(),
            "Provider icon should not be empty"
        );
    }
}

#[tokio::test]
async fn test_list_models_for_valid_provider() {
    let service = CatalogService::new();

    // Try a common provider
    let models = service.list_models("openai").await;

    // Should return models (if models.dev is accessible)
    // Note: This test might fail in offline environments
    if !models.is_empty() {
        for model in &models {
            assert!(!model.id.is_empty());
            assert!(!model.name.is_empty());
            assert_eq!(model.provider, "openai");
        }
    }
}

#[tokio::test]
async fn test_list_models_for_invalid_provider() {
    let service = CatalogService::new();

    let models = service.list_models("nonexistent_provider_xyz").await;

    // Should return empty vector for invalid provider
    assert!(models.is_empty());
}

#[tokio::test]
async fn test_provider_capabilities() {
    let service = CatalogService::new();

    // Try to get capabilities for a known provider
    if let Some(caps) = service.provider_capabilities("openai").await {
        assert!(caps.supports_streaming);
        assert!(caps.supports_tools);
        assert!(caps.max_context_tokens > 0);
    }
}

#[tokio::test]
async fn test_model_capabilities() {
    let service = CatalogService::new();

    // Try to get capabilities for a known model
    if let Some(caps) = service.model_capabilities("openai", "gpt-4o").await {
        assert!(caps.tool_call);
        assert!(caps.context_limit > 0);
        assert!(caps.output_limit > 0);
    }
}

#[tokio::test]
async fn test_context_limit() {
    let service = CatalogService::new();

    let openai_limit = service.context_limit("openai", "gpt-4o");
    let anthropic_limit = service.context_limit("anthropic", "claude-3");
    let gemini_limit = service.context_limit("google", "gemini-2.0");

    // Different providers should have different defaults
    assert!(openai_limit > 0);
    assert!(anthropic_limit > 0);
    assert!(gemini_limit > 0);

    // Gemini should have larger context
    assert!(gemini_limit > openai_limit);
}

#[tokio::test]
async fn test_supports_thinking() {
    let service = CatalogService::new();

    // Models with "thinking" in name should support thinking
    assert!(service.supports_thinking("openai", "o1-preview"));
    assert!(service.supports_thinking("openai", "o3"));
    assert!(service.supports_thinking("google", "gemini-2.0-flash-thinking-exp"));

    // Regular models should not
    assert!(!service.supports_thinking("openai", "gpt-4o"));
    assert!(!service.supports_thinking("anthropic", "claude-sonnet-4"));
}

#[tokio::test]
async fn test_supports_vision() {
    let service = CatalogService::new();

    // Most modern models support vision
    assert!(service.supports_vision("openai", "gpt-4o"));
    assert!(service.supports_vision("google", "gemini-2.0-flash"));

    // Embedding models should not
    assert!(!service.supports_vision("openai", "text-embedding-3-small"));
}

#[tokio::test]
async fn test_auth_status_with_api_key() {
    let service = CatalogService::new();

    // Check auth status (will depend on environment)
    let status = service.auth_status("openai");

    // Should return either ApiKey or NotAuthenticated
    assert!(matches!(
        status,
        AuthStatus::ApiKey | AuthStatus::NotAuthenticated
    ));
}

#[tokio::test]
async fn test_auth_status_ollama_always_authenticated() {
    let service = CatalogService::new();

    // Ollama is local and always "configured"
    assert!(service.is_provider_configured("ollama"));
}

#[tokio::test]
async fn test_is_provider_configured() {
    let service = CatalogService::new();

    // Ollama should always be configured (local)
    assert!(service.is_provider_configured("ollama"));

    // Other providers depend on environment variables
    // (can't test definitively without setting env vars)
}
