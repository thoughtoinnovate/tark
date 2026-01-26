# Tark Plugin Development Guide

A comprehensive guide to building, testing, and publishing WASM plugins for tark.

## Table of Contents

- [What is a Tark Plugin?](#what-is-a-tark-plugin)
- [Repository Anatomy](#repository-anatomy)
- [Quick Start: Create Your First Plugin](#quick-start-create-your-first-plugin)
- [Plugin Manifest (plugin.toml)](#plugin-manifest-plugintoml)
- [Implementing Plugin Types](#implementing-plugin-types)
- [Building and Packaging](#building-and-packaging)
- [Installing and Testing Locally](#installing-and-testing-locally)
- [Debugging and Troubleshooting](#debugging-and-troubleshooting)
- [Real-World Examples](#real-world-examples)
- [Publishing Your Plugin](#publishing-your-plugin)

## What is a Tark Plugin?

Tark plugins are **WebAssembly (WASM) modules** that extend tark's functionality in a secure, sandboxed environment. They communicate with tark through **WIT (WebAssembly Interface Types)** interfaces and follow a strict capability-based security model.

### Plugin Types

| Type | Purpose | Use Cases |
|------|---------|-----------|
| **`auth`** | Add authentication methods | OAuth providers, API key management, SSO |
| **`tool`** | Add agent capabilities | Custom file operations, API integrations, specialized workflows |
| **`provider`** | Add LLM providers | Custom model endpoints, local models, alternative APIs |
| **`channel`** | Add messaging channels | Slack, Discord, Signal bridges |
| **`hook`** | Lifecycle event handlers | Pre/post processing, logging, notifications |

### Security Model

Plugins run in a **WASM sandbox** with explicit capabilities:

- **No filesystem access** by default
- **No network access** by default  
- **No environment variable access** by default
- **No shell execution** by default

Each capability must be explicitly declared in `plugin.toml` and approved by the user.

### Why WASM?

- **Portable**: Runs on any platform tark supports
- **Secure**: Sandboxed execution with no system access by default
- **Fast**: Near-native performance
- **Language-agnostic**: Can be written in Rust, C, C++, or any language that compiles to WASM

## Repository Anatomy

Understanding where plugin-related code lives in the tark repository:

```
tark/
├── src/plugins/              # Host-side plugin infrastructure
│   ├── host/                 # WASM runtime and host functions
│   ├── registry/             # Plugin discovery and loading
│   ├── manifest/             # plugin.toml parsing and validation
│   └── wit/
│       └── tark.wit          # WIT interface definitions (copy this!)
├── plugins/                  # Bundled plugins
│   └── gemini-oauth/         # Example: OAuth provider plugin
│       ├── plugin.toml
│       └── plugin.wasm
└── examples/tark-config/plugins/  # Example user plugins
    └── git-helper/
        └── plugin.toml
```

**Key file**: `src/plugins/wit/tark.wit` defines all interfaces between tark and plugins.

## Quick Start: Create Your First Plugin

Let's build a minimal authentication plugin from scratch.

### Step 1: Create a New Rust Project

```bash
# Create a new library project
cargo new --lib my-tark-plugin
cd my-tark-plugin
```

### Step 2: Configure Cargo.toml

```toml
[package]
name = "my-tark-plugin"
version = "0.1.0"
edition = "2021"

[lib]
# IMPORTANT: cdylib is required for WASM
crate-type = ["cdylib"]

[dependencies]
# WIT bindings generator
wit-bindgen = "0.36"

[profile.release]
# Optimize for size
opt-level = "z"
lto = true
strip = true
```

### Step 3: Get the WIT Interface

Copy `tark.wit` from the tark repository:

```bash
# Option 1: Clone tark repo and copy
git clone https://github.com/thoughtoinnovate/tark.git
cp tark/src/plugins/wit/tark.wit .

# Option 2: Download directly
curl -o tark.wit https://raw.githubusercontent.com/thoughtoinnovate/tark/main/src/plugins/wit/tark.wit
```

**Keep it in sync**: When tark updates its WIT interface, you'll need to update your local copy and rebuild.

### Step 4: Create plugin.toml

```toml
[plugin]
name = "my-auth-plugin"
version = "0.1.0"
type = "auth"
description = "My custom authentication plugin"
author = "Your Name"
homepage = "https://github.com/you/my-auth-plugin"
license = "MIT"

# Minimum tark version this plugin requires
min_tark_version = "0.5.0"

[capabilities]
# Request only what you need
storage = true
http = ["auth.example.com"]
env = ["MY_API_KEY"]
```

### Step 5: Implement the Plugin

```rust
// src/lib.rs

// Generate WIT bindings
wit_bindgen::generate!({
    world: "auth-world",
    path: "tark.wit",
});

// Import host capabilities we need
use exports::tark::plugin::auth_plugin::{AuthStatus, DeviceCode, Guest, PollResult};

struct MyAuthPlugin;

impl Guest for MyAuthPlugin {
    fn display_name() -> String {
        "My Auth Provider".to_string()
    }
    
    fn status() -> AuthStatus {
        // Check if we have a stored token
        if let Some(_token) = tark::plugin::storage::get("access_token") {
            AuthStatus::Authenticated
        } else {
            AuthStatus::NotAuthenticated
        }
    }
    
    fn start_device_flow() -> Result<DeviceCode, String> {
        // Make HTTP request to start device flow
        let response = tark::plugin::http::post(
            "https://auth.example.com/device/code",
            r#"{"client_id":"my-client"}"#,
            vec![("Content-Type", "application/json")]
        ).map_err(|e| format!("HTTP error: {:?}", e))?;
        
        // Parse response and return device code
        Ok(DeviceCode {
            device_code: "device-code-123".to_string(),
            user_code: "USER-CODE".to_string(),
            verification_uri: "https://auth.example.com/device".to_string(),
            expires_in: 900,
            interval: 5,
        })
    }
    
    fn poll(device_code: String) -> PollResult {
        // Poll for token
        match tark::plugin::http::post(
            "https://auth.example.com/token",
            &format!(r#"{{"device_code":"{}"}}"#, device_code),
            vec![("Content-Type", "application/json")]
        ) {
            Ok(response) if response.status == 200 => {
                // Store token
                let _ = tark::plugin::storage::set("access_token", &response.body);
                PollResult::Success
            }
            Ok(response) if response.status == 400 => {
                PollResult::Pending
            }
            Ok(_) => PollResult::Error("Unexpected response".to_string()),
            Err(e) => PollResult::Error(format!("Request failed: {:?}", e)),
        }
    }
    
    fn get_token() -> Result<String, String> {
        tark::plugin::storage::get("access_token")
            .ok_or_else(|| "Not authenticated".to_string())
    }
    
    fn logout() -> Result<(), String> {
        tark::plugin::storage::delete("access_token")
    }
}

// Export the plugin
export!(MyAuthPlugin);
```

### Step 6: Build

```bash
# Install WASM target (first time only)
rustup target add wasm32-wasip1

# Build the plugin
cargo build --release --target wasm32-wasip1

# Copy to plugin directory
cp target/wasm32-wasip1/release/my_tark_plugin.wasm plugin.wasm
```

### Step 7: Install and Test

```bash
# Install the plugin
tark plugin add ./

# Verify it's installed
tark plugin list

# Test authentication
tark auth my-auth-plugin
```

🎉 You've built your first tark plugin!

## Plugin Manifest (plugin.toml)

The `plugin.toml` file is the plugin's configuration and metadata.

### Required Fields

```toml
[plugin]
name = "my-plugin"        # Lowercase, alphanumeric, dashes only
version = "1.0.0"         # Semantic versioning (semver)
type = "auth"             # One of: auth, tool, provider, channel, hook
```

### Optional Metadata

```toml
[plugin]
description = "What this plugin does"
author = "Your Name <email@example.com>"
homepage = "https://github.com/you/plugin"
license = "MIT"
min_tark_version = "0.5.0"
wasm = "plugin.wasm"      # Default: "plugin.wasm"
```

### Capabilities

Declare what your plugin needs access to:

#### Storage

Persistent key-value storage, namespaced per plugin.

```toml
[capabilities]
storage = true
```

**Use cases**: Storing auth tokens, caching API responses, user preferences.

#### HTTP

Network access to specific hosts.

```toml
[capabilities]
# Exact domains
http = ["api.example.com", "oauth2.googleapis.com"]

# Wildcards
http = ["*.example.com", "*.googleapis.com"]
```

**Security**: Requests to non-declared hosts will be blocked.

#### Environment Variables

Read specific environment variables.

```toml
[capabilities]
# Exact names
env = ["API_KEY", "GOOGLE_CLOUD_PROJECT"]

# Wildcards
env = ["GEMINI_*", "GCP_*"]
```

#### Filesystem Read

Read specific files or directories.

```toml
[capabilities]
fs_read = [
    "~/.config/app/credentials.json",
    "./data",
]
```

**Security**: Paths outside declared list are inaccessible.

#### Shell Execution

**⚠️ HIGH RISK**: Execute shell commands.

```toml
[capabilities]
shell = true  # Use with extreme caution!
```

**Warning**: Only request this if absolutely necessary. Most plugins don't need it.

### Validation Rules

| Field | Rule |
|-------|------|
| `name` | Lowercase, alphanumeric, hyphens only. No spaces. |
| `version` | Valid semantic version (e.g., `1.0.0`, `0.2.3-beta.1`) |
| `type` | One of: `auth`, `tool`, `provider`, `channel`, `hook` |
| `http` | Valid domain or wildcard pattern |
| `env` | Valid environment variable name or wildcard |

## Implementing Plugin Types

### Auth Plugin

Add authentication methods (OAuth, API keys, SSO).

**Interface**: `auth-plugin`

**Required methods**:

```rust
impl exports::tark::plugin::auth_plugin::Guest for MyPlugin {
    // Plugin display name
    fn display_name() -> String;
    
    // Current authentication status
    fn status() -> AuthStatus;  // Authenticated | NotAuthenticated
    
    // Start device flow (returns code for user to enter)
    fn start_device_flow() -> Result<DeviceCode, String>;
    
    // Poll for token after user authorizes
    fn poll(device_code: String) -> PollResult;  // Success | Pending | Error
    
    // Get current access token
    fn get_token() -> Result<String, String>;
    
    // Clear stored credentials
    fn logout() -> Result<(), String>;
}
```

**Example skeleton**:

```rust
wit_bindgen::generate!({
    world: "auth-world",
    path: "tark.wit",
});

use exports::tark::plugin::auth_plugin::*;

struct MyAuth;

impl Guest for MyAuth {
    fn display_name() -> String {
        "My OAuth Provider".to_string()
    }
    
    fn status() -> AuthStatus {
        // Check for stored token
        match tark::plugin::storage::get("token") {
            Some(_) => AuthStatus::Authenticated,
            None => AuthStatus::NotAuthenticated,
        }
    }
    
    fn start_device_flow() -> Result<DeviceCode, String> {
        // 1. Make HTTP POST to start device flow
        // 2. Return device code info for user
        todo!("Implement device flow start")
    }
    
    fn poll(device_code: String) -> PollResult {
        // 1. Poll authorization server
        // 2. If approved, store token and return Success
        // 3. If pending, return Pending
        // 4. If error, return Error
        todo!("Implement polling")
    }
    
    fn get_token() -> Result<String, String> {
        tark::plugin::storage::get("token")
            .ok_or_else(|| "Not authenticated".into())
    }
    
    fn logout() -> Result<(), String> {
        tark::plugin::storage::delete("token")
    }
}

export!(MyAuth);
```

### Tool Plugin

Add custom tools the agent can use.

**Interface**: `tool-plugin`

**Required methods**:

```rust
impl exports::tark::plugin::tool_plugin::Guest for MyPlugin {
    // Return list of tools this plugin provides
    fn get_tools() -> Vec<ToolDef>;
    
    // Get risk level for a tool
    fn get_risk_level(tool_name: String) -> RiskLevel;
    
    // Execute a tool
    fn execute(tool_name: String, arguments: String) -> ToolResult;
}
```

**Example**: A tool that fetches GitHub repo info

```rust
wit_bindgen::generate!({
    world: "tool-world",
    path: "tark.wit",
});

use exports::tark::plugin::tool_plugin::*;

struct GitHubTools;

impl Guest for GitHubTools {
    fn get_tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "github_repo_info".to_string(),
                description: "Get information about a GitHub repository".to_string(),
                input_schema: r#"{
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"},
                        "repo": {"type": "string"}
                    },
                    "required": ["owner", "repo"]
                }"#.to_string(),
            }
        ]
    }
    
    fn get_risk_level(_tool_name: String) -> RiskLevel {
        RiskLevel::Read  // Read-only, safe
    }
    
    fn execute(tool_name: String, arguments: String) -> ToolResult {
        match tool_name.as_str() {
            "github_repo_info" => {
                // Parse arguments
                let args: serde_json::Value = serde_json::from_str(&arguments)
                    .map_err(|e| ToolResult::Error(format!("Invalid args: {}", e)))?;
                
                let owner = args["owner"].as_str().unwrap();
                let repo = args["repo"].as_str().unwrap();
                
                // Make API request
                let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
                let response = tark::plugin::http::get(&url, vec![
                    ("User-Agent", "tark-plugin"),
                ])
                .map_err(|e| ToolResult::Error(format!("Request failed: {:?}", e)))?;
                
                if response.status == 200 {
                    ToolResult::Success(response.body)
                } else {
                    ToolResult::Error(format!("HTTP {}", response.status))
                }
            }
            _ => ToolResult::Error(format!("Unknown tool: {}", tool_name)),
        }
    }
}

export!(GitHubTools);
```

### Provider Plugin

Add LLM providers.

**Interface**: `provider-plugin`

**Required methods**:

```rust
impl exports::tark::plugin::provider_plugin::Guest for MyPlugin {
    // Provider name (e.g., "openai", "anthropic")
    fn name() -> String;
    
    // List of available models
    fn models() -> Vec<String>;
    
    // Send chat completion request
    fn chat(messages: Vec<Message>, model: String) -> Result<ChatResponse, String>;
}
```

**Example**: Custom local model provider

```rust
wit_bindgen::generate!({
    world: "provider-world",
    path: "tark.wit",
});

use exports::tark::plugin::provider_plugin::*;

struct MyProvider;

impl Guest for MyProvider {
    fn name() -> String {
        "my-local-llm".to_string()
    }
    
    fn models() -> Vec<String> {
        vec![
            "my-model-7b".to_string(),
            "my-model-13b".to_string(),
        ]
    }
    
    fn chat(messages: Vec<Message>, model: String) -> Result<ChatResponse, String> {
        // Convert messages to API format
        let payload = serde_json::json!({
            "model": model,
            "messages": messages,
        });
        
        // Call local API
        let response = tark::plugin::http::post(
            "http://localhost:11434/api/chat",
            &payload.to_string(),
            vec![("Content-Type", "application/json")]
        ).map_err(|e| format!("Request failed: {:?}", e))?;
        
        if response.status == 200 {
            // Parse response
            let json: serde_json::Value = serde_json::from_str(&response.body)
                .map_err(|e| format!("Parse error: {}", e))?;
            
            Ok(ChatResponse {
                content: json["response"].as_str().unwrap_or("").to_string(),
                model: model,
                usage_tokens: json["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            })
        } else {
            Err(format!("HTTP {}: {}", response.status, response.body))
        }
    }
}

export!(MyProvider);
```

### Hook Plugin

React to lifecycle events.

**Interface**: `hook-plugin`

**Example use cases**:
- Log all tool executions
- Send notifications on errors
- Pre-process file operations
- Custom metrics collection

```rust
wit_bindgen::generate!({
    world: "hook-world",
    path: "tark.wit",
});

use exports::tark::plugin::hook_plugin::*;

struct MyHooks;

impl Guest for MyHooks {
    fn on_tool_execute(tool_name: String, args: String) {
        // Called before any tool executes
        tark::plugin::log::info(&format!("Tool: {} with args: {}", tool_name, args));
    }
    
    fn on_tool_complete(tool_name: String, result: String) {
        // Called after tool completes
        tark::plugin::log::info(&format!("Tool {} completed: {}", tool_name, result));
    }
    
    fn on_error(error: String) {
        // Called on any error
        tark::plugin::log::error(&format!("Error occurred: {}", error));
    }
}

export!(MyHooks);
```

## Building and Packaging

### Install WASM Target

First time only:

```bash
rustup target add wasm32-wasip1
```

### Build Release Binary

```bash
# Build optimized WASM
cargo build --release --target wasm32-wasip1

# Output is at:
# target/wasm32-wasip1/release/your_plugin_name.wasm
```

### Optimize Binary Size

Add to `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
strip = true        # Strip symbols
codegen-units = 1   # Single codegen unit
```

### Package Structure

Recommended folder layout for distribution:

```
my-plugin/
├── plugin.toml       # Manifest
├── plugin.wasm       # Built WASM binary
├── README.md         # Usage instructions
├── LICENSE           # License file
└── src/              # Source code (for transparency)
    └── lib.rs
```

### Copy to Plugin Directory

```bash
cp target/wasm32-wasip1/release/my_plugin.wasm plugin.wasm
```

### Verify Binary

```bash
# Check file type
file plugin.wasm
# Should output: WebAssembly (wasm) binary module

# Check size
ls -lh plugin.wasm
# Typical size: 50KB - 500KB depending on complexity
```

## Installing and Testing Locally

### Install Plugin

```bash
# From local directory
tark plugin add ./my-plugin

# From git repository
tark plugin add https://github.com/you/my-plugin

# From specific branch/tag
tark plugin add https://github.com/you/my-plugin#v1.0.0
```

### List Installed Plugins

```bash
tark plugin list
# Output:
# gemini-oauth (provider) v0.2.0 - Gemini LLM provider
# my-plugin (auth) v0.1.0 - My custom auth plugin
```

### Show Plugin Details

```bash
tark plugin info my-plugin
# Shows: name, version, type, capabilities, status
```

### Enable/Disable

```bash
# Disable without removing
tark plugin disable my-plugin

# Re-enable
tark plugin enable my-plugin
```

### Remove Plugin

```bash
tark plugin remove my-plugin
```

### Plugin Storage Location

Plugins are stored in:

```
# Linux/macOS
$XDG_DATA_HOME/tark/plugins/  # usually ~/.local/share/tark/plugins/
# OR
~/.config/tark/plugins/

# Each plugin gets its own directory:
plugins/
├── gemini-oauth/
│   ├── plugin.toml
│   ├── plugin.wasm
│   └── data/          # Plugin storage (from storage capability)
└── my-plugin/
    ├── plugin.toml
    ├── plugin.wasm
    └── data/
```

### Test Plugin

#### For Auth Plugins

```bash
# Start authentication flow
tark auth my-plugin

# Check status
tark auth status
```

#### For Provider Plugins

```bash
# Use in chat
tark chat --provider my-provider --model my-model-7b

# Or select interactively
tark chat
# Then type: /model
# Select your provider from the list
```

#### For Tool Plugins

```bash
# Tools are automatically available to the agent
tark chat
# Agent can now use your tool when appropriate
```

### Integration Testing

Create a test script:

```bash
#!/bin/bash
set -e

echo "Building plugin..."
cargo build --release --target wasm32-wasip1
cp target/wasm32-wasip1/release/*.wasm plugin.wasm

echo "Installing plugin..."
tark plugin add ./

echo "Testing plugin..."
tark plugin info my-plugin

echo "Starting chat..."
tark chat
```

## Debugging and Troubleshooting

### Enable Debug Logs

```bash
# Set log level
export RUST_LOG=debug

# Or more specific
export RUST_LOG=tark_cli::plugins=debug

# Run tark
tark plugin add ./my-plugin
```

### Log from Plugin

Use the logging capability:

```rust
tark::plugin::log::debug("Debug message");
tark::plugin::log::info("Info message");
tark::plugin::log::warn("Warning message");
tark::plugin::log::error("Error message");
```

### Common Errors

#### 1. Plugin Won't Load

**Error**: `Failed to load plugin: file not found`

**Solution**:
- Verify `plugin.wasm` exists in plugin directory
- Check file name matches `plugin.toml` (`wasm` field)
- Ensure WASM binary is valid: `file plugin.wasm`

#### 2. Manifest Parse Error

**Error**: `Failed to parse plugin.toml`

**Solution**:
- Validate TOML syntax: `toml validate plugin.toml` (if you have toml CLI)
- Check required fields: `name`, `version`, `type`
- Ensure version is valid semver: `1.0.0` not `1.0`

#### 3. Capability Denied

**Error**: `HTTP request blocked: host not allowed`

**Solution**:
- Add host to `capabilities.http` in `plugin.toml`
- Use wildcards if needed: `*.example.com`
- Ensure HTTPS for secure endpoints

**Error**: `Environment variable access denied`

**Solution**:
- Add variable to `capabilities.env` in `plugin.toml`
- Use exact name or wildcard: `MY_*`

#### 4. WASM Module Error

**Error**: `WASM validation failed`

**Solution**:
- Ensure you built with `wasm32-wasip1` target
- Check for incompatible dependencies
- Rebuild with correct target: `cargo build --target wasm32-wasip1`

#### 5. Version Mismatch

**Error**: `Plugin requires tark >= 0.6.0, but current version is 0.5.0`

**Solution**:
- Update tark: `cargo install --git https://github.com/thoughtoinnovate/tark.git`
- Or adjust `min_tark_version` in your `plugin.toml`

#### 6. Storage Not Persisting

**Error**: Data lost between sessions

**Solution**:
- Verify `capabilities.storage = true` in manifest
- Check plugin data directory exists and is writable
- Use unique keys to avoid conflicts with other plugins

### Testing Infrastructure

Reference existing tests:

```bash
# Run plugin system tests
cargo test --test plugin_system

# Run WASM loading tests
cargo test --test wasm_plugin_load

# Run with logs
RUST_LOG=debug cargo test --test plugin_system -- --nocapture
```

### Debugging Checklist

- [ ] `plugin.toml` is valid TOML
- [ ] `plugin.wasm` exists and is valid WASM
- [ ] All required fields are present in manifest
- [ ] Version is valid semver
- [ ] Plugin type is one of: `auth`, `tool`, `provider`, `channel`, `hook`
- [ ] Capabilities match what your code uses
- [ ] HTTP hosts are properly declared
- [ ] Environment variables are declared
- [ ] WASM binary built with `wasm32-wasip1` target
- [ ] No `unwrap()` or `panic!()` in production code

## Real-World Examples

### Example 1: Gemini OAuth Provider

Full LLM provider with OAuth authentication.

**Location**: `plugins/gemini-oauth/`

**Manifest** (`plugin.toml`):

```toml
[plugin]
name = "gemini-oauth"
version = "0.2.0"
description = "Gemini LLM provider (supports GEMINI_API_KEY or OAuth)"
author = "InnoDrupe"
type = "provider"

[capabilities]
storage = true
http = [
    "oauth2.googleapis.com",
    "cloudcode-pa.googleapis.com",
    "generativelanguage.googleapis.com"
]
env = [
    "GOOGLE_CLOUD_PROJECT",
    "GEMINI_API_KEY"
]
fs_read = [
    "~/.gemini/oauth_creds.json"
]
```

**Key features**:
- Supports both API key and OAuth
- Reads OAuth credentials from Gemini CLI
- Stores and refreshes tokens
- Implements full provider interface

### Example 2: Git Helper Tools

Simple tool plugin using shell commands.

**Location**: `examples/tark-config/plugins/git-helper/`

**Manifest** (`plugin.toml`):

```toml
name = "git-helper"
version = "1.0.0"
description = "Git workflow helpers for tark"
plugin_type = "tool"

[[tools]]
name = "git_status_detailed"
description = "Get detailed git status with branch info and recent commits"
command = "bash"
args = ["-c", "git status && git log --oneline -5"]

[[tools]]
name = "git_diff_staged"
description = "Show staged changes ready for commit"
command = "git"
args = ["diff", "--cached"]
```

**Key features**:
- No WASM needed (config-based tools)
- Useful for common workflows
- Can inject rules into conversations

### Example 3: Minimal Auth Plugin

Skeleton for building auth plugins.

```rust
// Minimal viable auth plugin
wit_bindgen::generate!({
    world: "auth-world",
    path: "tark.wit",
});

use exports::tark::plugin::auth_plugin::*;

struct MinimalAuth;

impl Guest for MinimalAuth {
    fn display_name() -> String { "Minimal Auth".into() }
    fn status() -> AuthStatus { AuthStatus::NotAuthenticated }
    fn start_device_flow() -> Result<DeviceCode, String> {
        Err("Not implemented".into())
    }
    fn poll(_: String) -> PollResult {
        PollResult::Error("Not implemented".into())
    }
    fn get_token() -> Result<String, String> {
        Err("Not authenticated".into())
    }
    fn logout() -> Result<(), String> { Ok(()) }
}

export!(MinimalAuth);
```

## Publishing Your Plugin

### Pre-Publishing Checklist

- [ ] **Version**: Follows semantic versioning
- [ ] **License**: Clearly specified and LICENSE file included
- [ ] **README**: Installation and usage instructions
- [ ] **Security**: Minimal capabilities requested
- [ ] **Testing**: Works on all target platforms
- [ ] **Documentation**: API examples and troubleshooting
- [ ] **min_tark_version**: Set appropriately
- [ ] **Metadata**: Author, homepage, description complete

### Recommended README Template

```markdown
# My Tark Plugin

Brief description of what your plugin does.

## Installation

\`\`\`bash
tark plugin add https://github.com/you/my-plugin
\`\`\`

## Usage

[Provide examples]

## Capabilities

This plugin requests:
- `storage`: To store credentials
- `http`: To access api.example.com

## License

MIT
```

### Version Management

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): Breaking changes to plugin interface
- **MINOR** (0.1.0): New features, backward compatible
- **PATCH** (0.0.1): Bug fixes, backward compatible

### Set min_tark_version

```toml
[plugin]
min_tark_version = "0.5.0"  # Minimum tark version required
```

Test with the minimum version before releasing.

### Security Best Practices

1. **Request minimal capabilities**
   ```toml
   # BAD: Requesting everything
   [capabilities]
   http = ["*"]
   env = ["*"]
   shell = true
   
   # GOOD: Request only what's needed
   [capabilities]
   http = ["api.example.com"]
   env = ["API_KEY"]
   ```

2. **Validate all inputs**
   ```rust
   fn execute(tool_name: String, arguments: String) -> ToolResult {
       // Validate arguments
       let args: MyArgs = serde_json::from_str(&arguments)
           .map_err(|e| ToolResult::Error(format!("Invalid args: {}", e)))?;
       
       // Proceed with validated data
   }
   ```

3. **Handle errors gracefully**
   ```rust
   // BAD: Panic on error
   let token = get_token().unwrap();
   
   // GOOD: Return error
   let token = get_token()
       .map_err(|e| format!("Failed to get token: {}", e))?;
   ```

4. **Don't log secrets**
   ```rust
   // BAD
   tark::plugin::log::debug(&format!("Token: {}", token));
   
   // GOOD
   tark::plugin::log::debug("Token retrieved successfully");
   ```

### Distribution Options

#### Option 1: GitHub Releases

1. Create a release on GitHub
2. Attach `plugin.wasm` and `plugin.toml`
3. Users install via: `tark plugin add https://github.com/you/repo#v1.0.0`

#### Option 2: Plugin Registry

(Future feature) Central plugin registry for discovery and installation.

### Maintenance

- **Update WIT**: When tark updates `tark.wit`, rebuild your plugin
- **Test new versions**: Test against new tark releases
- **Security updates**: Promptly fix security issues
- **Deprecation**: Announce breaking changes in advance

## Additional Resources

- **WIT Specification**: [WebAssembly Component Model](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)
- **wit-bindgen**: [Official Documentation](https://github.com/bytecodealliance/wit-bindgen)
- **Wasmtime**: [Runtime Documentation](https://docs.wasmtime.dev/)
- **Tark Plugin SDK**: [Interface Reference](./PLUGIN_SDK.md)
- **Example Plugins**: `plugins/` and `examples/tark-config/plugins/` in the tark repository

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/thoughtoinnovate/tark/issues)
- **Discussions**: [GitHub Discussions](https://github.com/thoughtoinnovate/tark/discussions)
- **Examples**: Check `plugins/` directory for working examples

---

**Happy plugin building! 🚀**
