# Generic OAuth2 System Implementation

## Summary

Implemented a fully generic OAuth2 PKCE authentication system that eliminates hardcoded provider-specific auth logic. Plugins now declare their OAuth requirements in `plugin.toml`, and tark core provides the native OAuth flow infrastructure.

## Architecture

```
Plugin (plugin.toml)         Tark Core (src/auth/)         User
     [oauth]            →    OAuthHandler               →  Browser
  - auth_url                 - Generate PKCE                 OAuth Flow
  - token_url                - Start callback server         ↓
  - client_id                - Open browser                Callback
  - scopes                   - Exchange code             ← 
  - redirect_uri             - Save tokens                  
  - credentials_path         ↓
                          PluginInstance
                          auth_process_tokens()
                          (optional callback)
```

## What Was Implemented

### 1. Core OAuth Infrastructure (tark)

**New Module:** `src/auth/`
- `oauth.rs` - Generic OAuth2 PKCE handler (359 lines)
  - PKCE code generation (SHA256 + base64url)
  - Authorization URL building
  - Local HTTP callback server (axum)
  - Browser automation
  - Code-for-token exchange
  - Token refresh support
  - Secure credential storage (0600 permissions)

**Extended Plugin System:**
- `src/plugins/manifest/mod.rs` - Added `OAuthConfig` struct
- `src/plugins/host/mod.rs` - Added `auth_process_tokens()` callback support
- `src/plugins/mod.rs` - Exported OAuth types

**CLI Auto-Discovery:**
- `src/transport/cli.rs`
  - Dynamic auth menu (discovers installed plugins)
  - Generic `run_plugin_oauth_flow()`
  - Generic logout handler for OAuth plugins
  - Removed hardcoded ChatGPT-specific code

**Dependencies Added:**
- `base64 = "0.22"` - PKCE code challenge encoding
- `rand = "0.8"` - Secure random generation
- `urlencoding = "2"` - URL parameter encoding
- `open = "5"` - Browser automation

### 2. ChatGPT Plugin Updates

**File:** `plugins/tark/chatgpt/plugin.toml`

Added OAuth configuration:
```toml
[oauth]
flow = "pkce"
auth_url = "https://auth.openai.com/authorize"
token_url = "https://auth.openai.com/oauth/token"
client_id = "app_EMoamEEZ73f0CkXaXp7hrann"
scopes = ["openid", "profile", "email", "offline_access"]
redirect_uri = "http://localhost:8888/callback"
credentials_path = "~/.config/tark/chatgpt_oauth.json"
process_tokens_callback = "auth_process_tokens"
```

**File:** `plugins/tark/chatgpt/src/lib.rs`

Added token processor:
```rust
#[no_mangle]
pub extern "C" fn auth_process_tokens(
    tokens_ptr: i32,
    tokens_len: i32,
    ret_ptr: i32,
) -> i32 {
    // Extract account_id from JWT
    // Add metadata
    // Return enhanced JSON
}
```

### 3. Gemini Plugin Migration

**File:** `plugins/tark/gemini-oauth/`

Moved from InnoDrupe to tark plugins repository:
- Updated author to "tark contributors"
- Updated homepage to tark repository
- No OAuth section needed (uses Gemini CLI credentials)

## How It Works

### User Flow

```bash
# 1. Install plugin
tark plugin add https://github.com/thoughtoinnovate/plugins --path tark/chatgpt

# 2. Authenticate (auto-discovered in menu)
tark auth
# Shows:
#   7. ChatGPT (OAuth) (plugin)  ← Auto-discovered!
#   8. Gemini (OAuth/Key) (plugin)

# 3. Select option 7, OAuth flow runs automatically
# - Browser opens to auth.openai.com
# - User logs in
# - Callback received
# - Tokens saved to ~/.config/tark/chatgpt_oauth.json
# - Plugin processes tokens (extracts account_id)

# 4. Use immediately
tark chat --provider chatgpt-oauth --model gpt-5.1-codex-max
```

### Technical Flow

1. **Plugin Declaration** - Plugin defines OAuth config in `plugin.toml`
2. **Auto-Discovery** - Tark scans plugins, finds OAuth configs
3. **User Selection** - User selects provider from dynamic menu
4. **Generic Flow** - `run_plugin_oauth_flow()` uses plugin's config
5. **PKCE Execution** - `OAuthHandler` performs OAuth dance
6. **Token Processing** - Optional plugin callback enhances tokens
7. **Secure Storage** - Credentials saved with 0600 permissions

## Benefits

### For Users
- Install new OAuth providers via `tark plugin add` (no tark updates needed)
- Consistent auth experience across providers
- Auto-discovered in menu (no manual configuration)

### For Developers
- Zero hardcoding in tark core
- Reusable OAuth handler for any provider
- Plugin self-containment (all config in one file)
- Easy to add new OAuth providers (just create plugin)

### For Architecture
- Clean separation of concerns
- Generic infrastructure, specific implementations
- Extensible without core changes
- Follows plugin philosophy

## Testing

Verified:
- ✅ Auth menu dynamically shows both plugins
- ✅ Selecting ChatGPT triggers generic OAuth flow
- ✅ OAuth URL generated correctly
- ✅ Plugin token processing works
- ✅ Gemini plugin (no OAuth section) still works
- ✅ All builds succeed (tark + plugins)
- ✅ No clippy warnings
- ✅ Existing OAuth tests pass

## Security

**Token Storage:**
- Plaintext JSON with file permissions (0600)
- Industry standard approach (same as gh, aws, gcloud)
- Future: Consider OS keychain integration

**WASM Sandbox:**
- Plugins cannot access network directly
- HTTP calls mediated by tark host
- Capability system enforces permissions
- OAuth flow runs in native code (WASM can't bind ports)

## Commits

**Tark Core:**
- `3f80617` - feat: implement generic OAuth2 PKCE system for plugins

**Plugins Repo:**
- `d421df5` - feat: move gemini-oauth plugin to tark/plugins
- `9cfe0e8` - feat: add ChatGPT OAuth plugin with generic OAuth support

## Future Enhancements

1. **Device Flow Support** - Add generic device flow handler
2. **Token Encryption** - OS keychain integration
3. **Auto-Refresh UI** - Show refresh in TUI status bar
4. **More Providers** - Azure OpenAI, LocalAI, custom endpoints
5. **OAuth Debugging** - `tark auth status --verbose` shows token details

## Example: Adding New OAuth Provider

To add a new OAuth2 provider (e.g., Azure OpenAI), just create a plugin:

**plugin.toml:**
```toml
[plugin]
name = "azure-openai"
version = "1.0.0"
type = "provider"

[oauth]
flow = "pkce"
auth_url = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
token_url = "https://login.microsoftonline.com/common/oauth2/v2.0/token"
client_id = "your-client-id"
scopes = ["https://cognitiveservices.azure.com/.default", "offline_access"]
redirect_uri = "http://localhost:8888/callback"
credentials_path = "~/.config/tark/azure_oauth.json"

[contributes]
providers = [
    { id = "azure-openai", name = "Azure OpenAI", base_provider = "openai" }
]
```

Install and authenticate:
```bash
tark plugin add your-repo/azure-openai
tark auth azure-openai  # Auto-discovered!
tark chat --provider azure-openai
```

**No changes to tark core needed!**
