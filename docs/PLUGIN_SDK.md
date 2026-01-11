# Tark Plugin SDK

This guide explains how to build plugins for tark.

## Overview

Tark plugins are WebAssembly (WASM) modules that run in a sandboxed environment. They communicate with tark through WIT (WebAssembly Interface Types) interfaces, providing a secure and portable way to extend tark's functionality.

## Plugin Types

| Type | Purpose | Example Use Cases |
|------|---------|-------------------|
| `auth` | Add authentication methods | OAuth providers, API key management |
| `tool` | Add agent capabilities | Custom file operations, API integrations |
| `provider` | Add LLM providers | Custom model endpoints, local models |
| `hook` | Lifecycle event handlers | Pre/post processing, logging |

## Quick Start

### 1. Create a new Rust project

```bash
cargo new my-tark-plugin --lib
cd my-tark-plugin
```

### 2. Configure Cargo.toml

```toml
[package]
name = "my-tark-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.36"
```

### 3. Create plugin.toml

Every plugin must have a `plugin.toml` manifest:

```toml
[plugin]
name = "my-plugin"
version = "0.1.0"
type = "auth"  # or "tool", "provider", "hook"
description = "My awesome plugin"
author = "Your Name"
homepage = "https://github.com/you/my-plugin"
license = "MIT"

[capabilities]
storage = true
http = ["api.example.com", "*.googleapis.com"]
env = ["MY_API_KEY"]
```

### 4. Implement the plugin

Copy the `tark.wit` file from tark's source (`src/plugins/wit/tark.wit`) to your project.

```rust
// src/lib.rs
wit_bindgen::generate!({
    world: "auth-world",
    path: "tark.wit",
});

struct MyAuthPlugin;

impl exports::tark::plugin::auth_plugin::Guest for MyAuthPlugin {
    fn display_name() -> String {
        "My Auth".to_string()
    }
    
    fn status() -> AuthStatus {
        // Check for stored credentials
        if tark::plugin::storage::get("token").is_some() {
            AuthStatus::Authenticated
        } else {
            AuthStatus::NotAuthenticated
        }
    }
    
    fn start_device_flow() -> Result<DeviceCode, String> {
        // Implement OAuth device flow
        todo!()
    }
    
    fn poll(device_code: String) -> PollResult {
        // Poll for token
        todo!()
    }
    
    fn get_token() -> Result<String, String> {
        tark::plugin::storage::get("token")
            .ok_or_else(|| "Not authenticated".to_string())
    }
    
    fn logout() -> Result<(), String> {
        tark::plugin::storage::delete("token")
    }
}

export!(MyAuthPlugin);
```

### 5. Build the plugin

```bash
# Install WASM target if needed
rustup target add wasm32-wasip1

# Build
cargo build --release --target wasm32-wasip1

# Copy to plugin directory
cp target/wasm32-wasip1/release/my_tark_plugin.wasm plugin.wasm
```

### 6. Install the plugin

```bash
# From local directory
tark plugin add ./

# Or from git repository
tark plugin add https://github.com/you/my-plugin
```

## Plugin Interfaces

### Auth Plugin

Add authentication methods (OAuth, API keys).

```rust
interface auth-plugin {
    // Get plugin display name
    display-name: func() -> string;
    
    // Get current authentication status
    status: func() -> auth-status;
    
    // Start device flow authentication
    start-device-flow: func() -> result<device-code, string>;
    
    // Poll for token (call after user authorizes)
    poll: func(device-code: string) -> poll-result;
    
    // Get current access token (if authenticated)
    get-token: func() -> result<string, string>;
    
    // Logout (clear stored credentials)
    logout: func() -> result<_, string>;
}
```

### Tool Plugin

Add tools the agent can use.

```rust
interface tool-plugin {
    // Get tool definitions
    get-tools: func() -> list<tool-def>;
    
    // Get risk level for a tool
    get-risk-level: func(tool-name: string) -> risk-level;
    
    // Execute a tool
    execute: func(tool-name: string, arguments: string) -> tool-result;
}
```

### Provider Plugin

Add LLM providers.

```rust
interface provider-plugin {
    // Get provider name
    name: func() -> string;
    
    // Get available models
    models: func() -> list<string>;
    
    // Send chat completion (non-streaming)
    chat: func(messages: list<message>, model: string) -> result<chat-response, string>;
}
```

## Host Capabilities

Plugins can import these capabilities (must be declared in `plugin.toml`):

### Storage

Persistent key-value storage namespaced per plugin.

```rust
// Get a value
let value: Option<String> = tark::plugin::storage::get("key");

// Set a value
tark::plugin::storage::set("key", "value")?;

// Delete a key
tark::plugin::storage::delete("key")?;

// List all keys
let keys: Vec<String> = tark::plugin::storage::list_keys();
```

### HTTP

Make HTTP requests to allowed hosts.

```rust
// GET request
let response = tark::plugin::http::get(
    "https://api.example.com/data",
    vec![("Authorization", "Bearer token")]
)?;

// POST request
let response = tark::plugin::http::post(
    "https://api.example.com/data",
    r#"{"key": "value"}"#,
    vec![("Content-Type", "application/json")]
)?;

// Response fields
println!("Status: {}", response.status);
println!("Body: {}", response.body);
```

### Environment

Read allowed environment variables.

```rust
if let Some(api_key) = tark::plugin::env::get("MY_API_KEY") {
    // Use the API key
}
```

### Logging

Log messages at various levels.

```rust
tark::plugin::log::debug("Debug message");
tark::plugin::log::info("Info message");
tark::plugin::log::warn("Warning message");
tark::plugin::log::error("Error message");
```

## Security Model

Plugins run in a WASM sandbox with strict capability controls:

| Capability | Description | Risk Level |
|------------|-------------|------------|
| `storage` | Persistent key-value storage | Low |
| `http` | Network access to declared hosts | Medium |
| `env` | Read declared environment variables | Medium |
| `shell` | Execute shell commands | **High** |
| `filesystem` | Access declared paths | Medium |

### Best Practices

1. **Request minimal capabilities** - Only declare what you need
2. **Validate all inputs** - Don't trust data from external sources
3. **Handle errors gracefully** - Return meaningful error messages
4. **Document capabilities** - Explain why each capability is needed

## Plugin Management

```bash
# List installed plugins
tark plugin list

# Show plugin details
tark plugin info <plugin-id>

# Install from git
tark plugin add https://github.com/user/plugin

# Install from local directory
tark plugin add ./my-plugin

# Uninstall
tark plugin remove <plugin-id>

# Enable/disable
tark plugin enable <plugin-id>
tark plugin disable <plugin-id>
```

## Example Plugins

### Minimal Auth Plugin

```rust
use wit_bindgen::generate;

generate!({
    world: "auth-world",
    path: "tark.wit",
});

struct MinimalAuth;

impl exports::tark::plugin::auth_plugin::Guest for MinimalAuth {
    fn display_name() -> String {
        "Minimal Auth".to_string()
    }
    
    fn status() -> AuthStatus {
        AuthStatus::NotAuthenticated
    }
    
    fn start_device_flow() -> Result<DeviceCode, String> {
        Err("Not implemented".to_string())
    }
    
    fn poll(_device_code: String) -> PollResult {
        PollResult::Error("Not implemented".to_string())
    }
    
    fn get_token() -> Result<String, String> {
        Err("Not authenticated".to_string())
    }
    
    fn logout() -> Result<(), String> {
        Ok(())
    }
}

export!(MinimalAuth);
```

## Troubleshooting

### Plugin won't load

1. Check that `plugin.wasm` exists in the plugin directory
2. Verify `plugin.toml` is valid TOML
3. Ensure version is valid semver (e.g., "1.0.0")
4. Check tark logs for detailed error messages

### HTTP requests fail

1. Verify the host is declared in `capabilities.http`
2. Check for wildcard patterns (e.g., `*.googleapis.com`)
3. Ensure HTTPS is used for secure endpoints

### Storage not persisting

1. Verify `capabilities.storage = true` in manifest
2. Check plugin data directory permissions
3. Use unique keys to avoid conflicts

## Resources

- [WIT Specification](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)
- [wit-bindgen Documentation](https://github.com/bytecodealliance/wit-bindgen)
- [wasmtime Documentation](https://docs.wasmtime.dev/)
