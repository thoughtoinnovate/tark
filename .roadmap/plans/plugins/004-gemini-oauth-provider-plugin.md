# Plan 004: Upgrade gemini-auth to Provider Plugin

## Overview

Convert the existing `gemini-auth` (auth type) plugin to `gemini-oauth` (provider type) so it appears in the model picker and handles full chat requests.

## Current State

- Plugin type: `auth`
- Exports: `status`, `get_token`, `init_with_credentials`, `logout`, `display_name`, `get_endpoint`
- Problem: tark looks for `provider` type plugins in model picker

## Target State

- Plugin type: `provider`
- Exports: `provider_info`, `provider_models`, `provider_auth_status`, `provider_auth_init`, `provider_chat`
- Result: Appears in model picker, handles full chat flow via Cloud Code Assist API

---

## Step 1: Update plugin.toml

**File:** `plugin.toml`

```toml
# Gemini OAuth Provider Plugin
# Full LLM provider using Gemini CLI OAuth credentials via Cloud Code Assist API

[plugin]
name = "gemini-oauth"
version = "0.2.0"
type = "provider"
description = "Gemini models via OAuth (uses Gemini CLI credentials)"
author = "InnoDrupe"
homepage = "https://github.com/InnoDrupe/tark-plugin-gemini-oauth"
api_version = "0.1"

[capabilities]
storage = true
http = [
    "oauth2.googleapis.com",
    "cloudcode-pa.googleapis.com"
]
env = [
    "GOOGLE_CLOUD_PROJECT",
    "GCLOUD_PROJECT", 
    "GCP_PROJECT",
    "HOME"
]
shell = false

[contributes]
providers = [
    { id = "gemini-oauth", name = "Gemini (OAuth)", description = "Gemini models via Cloud Code Assist API" }
]

[activation]
events = ["onProvider:gemini-oauth"]
```

---

## Step 2: Update Rust Plugin Code

**File:** `src/lib.rs`

```rust
//! Gemini OAuth Provider Plugin
//!
//! Full LLM provider that uses Gemini CLI OAuth credentials
//! to communicate with Google's Cloud Code Assist API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Host Imports (provided by tark)
// =============================================================================

mod host {
    #[link(wasm_import_module = "tark:storage")]
    extern "C" {
        pub fn get(key_ptr: i32, key_len: i32, ret_ptr: i32) -> i32;
        pub fn set(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
        pub fn delete(key_ptr: i32, key_len: i32) -> i32;
    }

    #[link(wasm_import_module = "tark:http")]
    extern "C" {
        pub fn get(url_ptr: i32, url_len: i32, headers_ptr: i32, headers_len: i32, ret_ptr: i32) -> i32;
        pub fn post(url_ptr: i32, url_len: i32, body_ptr: i32, body_len: i32, headers_ptr: i32, headers_len: i32, ret_ptr: i32) -> i32;
    }

    #[link(wasm_import_module = "tark:env")]
    extern "C" {
        pub fn get(name_ptr: i32, name_len: i32, ret_ptr: i32) -> i32;
    }

    #[link(wasm_import_module = "tark:log")]
    extern "C" {
        pub fn info(msg_ptr: i32, msg_len: i32);
        pub fn debug(msg_ptr: i32, msg_len: i32);
        pub fn warn(msg_ptr: i32, msg_len: i32);
        pub fn error(msg_ptr: i32, msg_len: i32);
    }
}

// =============================================================================
// Types
// =============================================================================

#[derive(Debug, Serialize)]
struct ProviderInfo {
    id: String,
    display_name: String,
    description: String,
    requires_auth: bool,
}

#[derive(Debug, Serialize)]
struct ModelInfo {
    id: String,
    display_name: String,
    context_window: u32,
    supports_streaming: bool,
    supports_tools: bool,
}

#[derive(Debug, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    text: String,
    usage: Option<Usage>,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct GeminiCliCredentials {
    access_token: String,
    refresh_token: String,
    expiry_date: u64,
    token_type: String,
    scope: String,
}

// Cloud Code Assist API types
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeAssistRequest {
    contents: Vec<CodeAssistContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CodeAssistContent {
    role: String,
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeAssistResponse {
    candidates: Vec<Candidate>,
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: CodeAssistContent,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

// =============================================================================
// Memory Management
// =============================================================================

static mut BUFFER: [u8; 131072] = [0; 131072]; // 128KB buffer

#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    // Simple bump allocator - just return start of buffer
    // In production, use a proper allocator
    unsafe { BUFFER.as_ptr() as i32 }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn storage_get(key: &str) -> Option<String> {
    unsafe {
        let key_bytes = key.as_bytes();
        let ret_ptr = BUFFER.as_mut_ptr().add(65536) as i32; // Second half of buffer
        let len = host::get(
            key_bytes.as_ptr() as i32,
            key_bytes.len() as i32,
            ret_ptr,
        );
        if len <= 0 {
            return None;
        }
        let slice = std::slice::from_raw_parts(ret_ptr as *const u8, len as usize);
        String::from_utf8(slice.to_vec()).ok().filter(|s| !s.is_empty())
    }
}

fn storage_set(key: &str, value: &str) {
    unsafe {
        let key_bytes = key.as_bytes();
        let val_bytes = value.as_bytes();
        host::set(
            key_bytes.as_ptr() as i32,
            key_bytes.len() as i32,
            val_bytes.as_ptr() as i32,
            val_bytes.len() as i32,
        );
    }
}

fn http_post(url: &str, body: &str, headers: &[(String, String)]) -> Result<String, String> {
    unsafe {
        let url_bytes = url.as_bytes();
        let body_bytes = body.as_bytes();
        let headers_json = serde_json::to_string(headers).unwrap_or_default();
        let headers_bytes = headers_json.as_bytes();
        
        let ret_ptr = BUFFER.as_mut_ptr().add(65536) as i32;
        
        let len = host::post(
            url_bytes.as_ptr() as i32,
            url_bytes.len() as i32,
            body_bytes.as_ptr() as i32,
            body_bytes.len() as i32,
            headers_bytes.as_ptr() as i32,
            headers_bytes.len() as i32,
            ret_ptr,
        );
        
        if len < 0 {
            return Err(format!("HTTP POST failed with code: {}", len));
        }
        
        let slice = std::slice::from_raw_parts(ret_ptr as *const u8, len as usize);
        String::from_utf8(slice.to_vec()).map_err(|e| e.to_string())
    }
}

fn log_info(msg: &str) {
    unsafe {
        let bytes = msg.as_bytes();
        host::info(bytes.as_ptr() as i32, bytes.len() as i32);
    }
}

fn write_string_to_ptr(ptr: i32, s: &str) -> i32 {
    unsafe {
        let bytes = s.as_bytes();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        bytes.len() as i32
    }
}

fn read_string_from_ptr(ptr: i32, len: i32) -> String {
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).to_string()
    }
}

// =============================================================================
// Credential Management
// =============================================================================

fn get_credentials() -> Option<GeminiCliCredentials> {
    storage_get("credentials").and_then(|s| serde_json::from_str(&s).ok())
}

fn get_access_token() -> Option<String> {
    let creds = get_credentials()?;
    
    // Check if token is expired
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    
    if now_ms >= creds.expiry_date {
        // Token expired, try to refresh
        return refresh_token(&creds);
    }
    
    Some(creds.access_token)
}

fn refresh_token(creds: &GeminiCliCredentials) -> Option<String> {
    log_info("Refreshing OAuth token...");
    
    // Google OAuth token refresh
    let body = format!(
        "client_id=77185425430.apps.googleusercontent.com&\
         client_secret=OTJgUOQcT7lO7GsGZq2G4IlT&\
         refresh_token={}&\
         grant_type=refresh_token",
        creds.refresh_token
    );
    
    let headers = vec![
        ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
    ];
    
    let response = http_post("https://oauth2.googleapis.com/token", &body, &headers).ok()?;
    
    // Parse response
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        expires_in: u64,
    }
    
    // Extract body from HTTP response JSON
    let http_resp: serde_json::Value = serde_json::from_str(&response).ok()?;
    let body_str = http_resp["body"].as_str()?;
    let token_resp: TokenResponse = serde_json::from_str(body_str).ok()?;
    
    // Update stored credentials
    let new_creds = GeminiCliCredentials {
        access_token: token_resp.access_token.clone(),
        refresh_token: creds.refresh_token.clone(),
        expiry_date: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            + (token_resp.expires_in * 1000),
        token_type: creds.token_type.clone(),
        scope: creds.scope.clone(),
    };
    
    storage_set("credentials", &serde_json::to_string(&new_creds).unwrap_or_default());
    
    Some(token_resp.access_token)
}

// =============================================================================
// Provider Plugin Exports
// =============================================================================

/// Get provider info (JSON)
#[no_mangle]
pub extern "C" fn provider_info(ret_ptr: i32) -> i32 {
    let info = ProviderInfo {
        id: "gemini-oauth".to_string(),
        display_name: "Gemini (OAuth)".to_string(),
        description: "Gemini models via Cloud Code Assist API".to_string(),
        requires_auth: true,
    };
    
    let json = serde_json::to_string(&info).unwrap_or_default();
    write_string_to_ptr(ret_ptr, &json)
}

/// Get available models (JSON array)
#[no_mangle]
pub extern "C" fn provider_models(ret_ptr: i32) -> i32 {
    let models = vec![
        ModelInfo {
            id: "gemini-2.0-flash-exp".to_string(),
            display_name: "Gemini 2.0 Flash (Experimental)".to_string(),
            context_window: 1048576,
            supports_streaming: true,
            supports_tools: true,
        },
        ModelInfo {
            id: "gemini-1.5-pro".to_string(),
            display_name: "Gemini 1.5 Pro".to_string(),
            context_window: 2097152,
            supports_streaming: true,
            supports_tools: true,
        },
        ModelInfo {
            id: "gemini-1.5-flash".to_string(),
            display_name: "Gemini 1.5 Flash".to_string(),
            context_window: 1048576,
            supports_streaming: true,
            supports_tools: true,
        },
    ];
    
    let json = serde_json::to_string(&models).unwrap_or_default();
    write_string_to_ptr(ret_ptr, &json)
}

/// Get auth status
/// Returns: 0=not-required, 1=authenticated, 2=not-authenticated, 3=expired
#[no_mangle]
pub extern "C" fn provider_auth_status() -> i32 {
    match get_credentials() {
        None => 2, // NotAuthenticated
        Some(creds) => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            
            if now_ms >= creds.expiry_date {
                // Try refresh
                if refresh_token(&creds).is_some() {
                    1 // Authenticated
                } else {
                    3 // Expired
                }
            } else {
                1 // Authenticated
            }
        }
    }
}

/// Initialize with credentials JSON
#[no_mangle]
pub extern "C" fn provider_auth_init(creds_ptr: i32, creds_len: i32) -> i32 {
    let creds_json = read_string_from_ptr(creds_ptr, creds_len);
    
    // Parse Gemini CLI credentials format
    match serde_json::from_str::<GeminiCliCredentials>(&creds_json) {
        Ok(creds) => {
            storage_set("credentials", &creds_json);
            log_info("Credentials initialized successfully");
            0 // Success
        }
        Err(e) => {
            log_info(&format!("Failed to parse credentials: {}", e));
            -1 // Error
        }
    }
}

/// Logout
#[no_mangle]
pub extern "C" fn provider_auth_logout() -> i32 {
    unsafe {
        let key = "credentials";
        let key_bytes = key.as_bytes();
        host::delete(key_bytes.as_ptr() as i32, key_bytes.len() as i32);
    }
    0
}

/// Send chat request
/// msgs_ptr: JSON array of messages
/// model_ptr: model ID string
/// ret_ptr: where to write response JSON
/// Returns: length of response or negative error code
#[no_mangle]
pub extern "C" fn provider_chat(
    msgs_ptr: i32,
    msgs_len: i32,
    model_ptr: i32,
    model_len: i32,
    ret_ptr: i32,
) -> i32 {
    // Get access token
    let token = match get_access_token() {
        Some(t) => t,
        None => {
            let error = r#"{"error":"Not authenticated. Please initialize with credentials."}"#;
            return write_string_to_ptr(ret_ptr, error);
        }
    };
    
    // Parse messages
    let msgs_json = read_string_from_ptr(msgs_ptr, msgs_len);
    let messages: Vec<Message> = match serde_json::from_str(&msgs_json) {
        Ok(m) => m,
        Err(e) => {
            let error = format!(r#"{{"error":"Failed to parse messages: {}"}}"#, e);
            return write_string_to_ptr(ret_ptr, &error);
        }
    };
    
    // Parse model
    let model = read_string_from_ptr(model_ptr, model_len);
    
    // Convert to Cloud Code Assist format
    let mut system_instruction = None;
    let mut contents = Vec::new();
    
    for msg in &messages {
        match msg.role.as_str() {
            "system" => {
                system_instruction = Some(SystemInstruction {
                    parts: vec![Part { text: msg.content.clone() }],
                });
            }
            "user" => {
                contents.push(CodeAssistContent {
                    role: "user".to_string(),
                    parts: vec![Part { text: msg.content.clone() }],
                });
            }
            "assistant" => {
                contents.push(CodeAssistContent {
                    role: "model".to_string(),
                    parts: vec![Part { text: msg.content.clone() }],
                });
            }
            _ => {}
        }
    }
    
    let request = CodeAssistRequest {
        contents,
        system_instruction,
        generation_config: Some(GenerationConfig {
            max_output_tokens: Some(8192),
            temperature: Some(0.7),
        }),
    };
    
    let request_json = match serde_json::to_string(&request) {
        Ok(j) => j,
        Err(e) => {
            let error = format!(r#"{{"error":"Failed to serialize request: {}"}}"#, e);
            return write_string_to_ptr(ret_ptr, &error);
        }
    };
    
    // Send to Cloud Code Assist API
    let url = format!(
        "https://cloudcode-pa.googleapis.com/v1/models/{}:generateContent",
        model
    );
    
    let headers = vec![
        ("Authorization".to_string(), format!("Bearer {}", token)),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    
    log_info(&format!("Sending chat request to {}", url));
    
    let response = match http_post(&url, &request_json, &headers) {
        Ok(r) => r,
        Err(e) => {
            let error = format!(r#"{{"error":"HTTP request failed: {}"}}"#, e);
            return write_string_to_ptr(ret_ptr, &error);
        }
    };
    
    // Parse HTTP response wrapper
    let http_resp: serde_json::Value = match serde_json::from_str(&response) {
        Ok(v) => v,
        Err(e) => {
            let error = format!(r#"{{"error":"Failed to parse HTTP response: {}"}}"#, e);
            return write_string_to_ptr(ret_ptr, &error);
        }
    };
    
    // Check for HTTP error
    if let Some(status) = http_resp["status"].as_u64() {
        if status >= 400 {
            let body = http_resp["body"].as_str().unwrap_or("Unknown error");
            let error = format!(r#"{{"error":"API error ({}): {}"}}"#, status, body);
            return write_string_to_ptr(ret_ptr, &error);
        }
    }
    
    // Parse API response
    let body_str = http_resp["body"].as_str().unwrap_or("{}");
    let api_resp: CodeAssistResponse = match serde_json::from_str(body_str) {
        Ok(r) => r,
        Err(e) => {
            let error = format!(r#"{{"error":"Failed to parse API response: {} - body: {}"}}"#, e, body_str);
            return write_string_to_ptr(ret_ptr, &error);
        }
    };
    
    // Extract response text
    let text = api_resp
        .candidates
        .first()
        .and_then(|c| c.content.parts.first())
        .map(|p| p.text.clone())
        .unwrap_or_default();
    
    let finish_reason = api_resp
        .candidates
        .first()
        .and_then(|c| c.finish_reason.clone());
    
    let usage = api_resp.usage_metadata.map(|u| Usage {
        input_tokens: u.prompt_token_count.unwrap_or(0),
        output_tokens: u.candidates_token_count.unwrap_or(0),
    });
    
    let chat_response = ChatResponse {
        text,
        usage,
        finish_reason,
    };
    
    let response_json = serde_json::to_string(&chat_response).unwrap_or_default();
    write_string_to_ptr(ret_ptr, &response_json)
}

// =============================================================================
// Legacy Auth Plugin Exports (for backwards compatibility)
// =============================================================================

#[no_mangle]
pub extern "C" fn status() -> i32 {
    provider_auth_status()
}

#[no_mangle]
pub extern "C" fn get_token(ret_ptr: i32) -> i32 {
    match get_access_token() {
        Some(token) => write_string_to_ptr(ret_ptr, &token),
        None => -1,
    }
}

#[no_mangle]
pub extern "C" fn get_endpoint(ret_ptr: i32) -> i32 {
    write_string_to_ptr(ret_ptr, "https://cloudcode-pa.googleapis.com")
}

#[no_mangle]
pub extern "C" fn init_with_credentials(creds_ptr: i32, creds_len: i32) -> i32 {
    provider_auth_init(creds_ptr, creds_len)
}

#[no_mangle]
pub extern "C" fn logout() -> i32 {
    provider_auth_logout()
}

#[no_mangle]
pub extern "C" fn display_name(ret_ptr: i32) -> i32 {
    write_string_to_ptr(ret_ptr, "Gemini (OAuth)")
}
```

---

## Step 3: Update Cargo.toml

```toml
[package]
name = "tark-plugin-gemini-oauth"
version = "0.2.0"
edition = "2021"
description = "Gemini OAuth provider plugin for tark"
license = "MIT"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
```

---

## Step 4: Build Script

**File:** `build.sh`

```bash
#!/bin/bash
set -e

# Add WASM target
rustup target add wasm32-wasip1 || true

# Build
cargo build --target wasm32-wasip1 --release

# Copy to dist
mkdir -p dist
cp target/wasm32-wasip1/release/tark_plugin_gemini_oauth.wasm dist/plugin.wasm
cp plugin.toml dist/

echo "Built plugin:"
ls -lh dist/
```

---

## Step 5: Install Updated Plugin

```bash
# Build the plugin
./build.sh

# Remove old plugin
tark plugin remove gemini-auth

# Install new plugin
tark plugin add ./dist

# Verify
tark plugin list
tark plugin info gemini-oauth
```

---

## Verification

After installation, the plugin should:

1. ✅ Appear in model picker (`/model` command)
2. ✅ Show as "Gemini (OAuth)" with available models
3. ✅ Auto-initialize with `~/.gemini/oauth_creds.json` if present
4. ✅ Handle full chat requests via Cloud Code Assist API

```bash
# Test
tark chat -p gemini-oauth "Hello, world!"
```

---

## Summary

| Before | After |
|--------|-------|
| Type: `auth` | Type: `provider` |
| Only provides tokens | Handles full chat |
| Not in model picker | Shows in model picker |
| tark routes to wrong API | Plugin routes to correct API |
