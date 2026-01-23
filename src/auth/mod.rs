//! Authentication infrastructure for OAuth flows
//!
//! This module provides generic OAuth2 implementations that can be configured
//! via plugin manifests, eliminating the need for provider-specific hardcoding.

mod oauth;

pub use oauth::{OAuthHandler, TokenResponse};

// Re-export device flow auth from llm module
pub use crate::llm::auth::{AuthStatus, DeviceCodeResponse, DeviceFlowAuth, OAuthToken};
