# New LLM Providers

This document describes the three new LLM providers added to Tark: **GitHub Copilot**, **Google Gemini**, and **OpenRouter**.

## GitHub Copilot

GitHub Copilot integration with **Device Flow OAuth** authentication.

### Setup

1. **No manual token needed!** On first use, Tark will:
   - Generate a device code
   - Display a URL and code (e.g., `github.com/login/device` with code `XXXX-XXXX`)
   - Wait for you to authorize in your browser
   - Save the token automatically to `~/.config/tark/copilot_token.json`

2. **Configuration** (optional):
   ```toml
   # config.toml
   [llm.copilot]
   model = "gpt-4o"
   max_tokens = 4096
   auth_timeout_secs = 1800  # 30 minutes (default)
   ```
   
   **Note**: `auth_timeout_secs` controls how long the auth command waits for browser authorization.

3. **Select provider**:
   ```bash
   tark chat --provider copilot
   # or
   tark chat --provider github
   ```

### Features
- ✅ Device Flow OAuth (automatic browser-based auth)
- ✅ Token caching and auto-refresh
- ✅ Streaming support
- ✅ OpenAI-compatible API
- ✅ Subscription-aware model listing (shows only models you have access to)
- ⚠️ Tool calling not fully supported (Copilot limitation)

### Subscription Tiers

GitHub Copilot automatically detects your subscription tier from the token:

| Tier | Models Available |
|------|------------------|
| **Free** | GPT-4o (limited usage) |
| **Individual/Business** | GPT-4o, GPT-4 |
| **Enterprise** | GPT-4o, GPT-4, Claude 3.5 Sonnet (if enabled), O1 (if enabled) |

The model picker shows only the models available in your subscription.

---

## Google Gemini

Google's Gemini models (2.0 Flash, Pro, etc.)

### Setup

1. **Get API Key**:
   - Visit https://aistudio.google.com/apikey
   - Create an API key
   - Set environment variable:
   ```bash
   export GEMINI_API_KEY="your-api-key-here"
   ```

2. **Configuration** (optional):
   ```toml
   # config.toml
   [llm.gemini]
   model = "gemini-2.0-flash-exp"
   max_tokens = 8192
   ```

3. **Usage**:
   ```bash
   tark chat --provider gemini
   # or
   tark chat --provider google
   ```

### Features
- ✅ Latest Gemini models (2.0 Flash, Pro, etc.)
- ✅ Streaming support
- ✅ Function/tool calling
- ✅ Large context windows (up to 2M tokens)
- ✅ Free tier available

### Available Models
- `gemini-2.0-flash-exp` (default) - Fast, efficient, great for coding
- `gemini-2.0-flash-thinking-exp` - With extended thinking
- `gemini-1.5-pro` - Larger model, better at complex tasks
- `gemini-1.5-flash` - Fast and lightweight

---

## OpenRouter

Access to **200+ models** from various providers through a unified API.

### Setup

1. **Get API Key**:
   - Visit https://openrouter.ai/keys
   - Create an account and generate an API key
   - Set environment variable:
   ```bash
   export OPENROUTER_API_KEY="your-api-key-here"
   ```

2. **Configuration** (optional):
   ```toml
   # config.toml
   [llm.openrouter]
   model = "anthropic/claude-sonnet-4"
   max_tokens = 4096
   site_url = "https://github.com/yourusername/tark"  # Optional
   app_name = "Tark"  # Optional
   ```

3. **Usage**:
   ```bash
   tark chat --provider openrouter
   ```

### Features
- ✅ Access to 200+ models from multiple providers
- ✅ OpenAI-compatible API
- ✅ Streaming support
- ✅ Tool calling support
- ✅ Many free models available
- ✅ Automatic fallback and load balancing

### Popular Models on OpenRouter

**Free Models:**
- `google/gemma-2-9b-it:free` - Gemma 2 (free)
- `meta-llama/llama-3.1-8b-instruct:free` - Llama 3.1 (free)
- `qwen/qwen-2.5-7b-instruct:free` - Qwen (free)

**Cheap Models:**
- `deepseek/deepseek-chat` - Very affordable, great performance
- `anthropic/claude-3.5-haiku` - Fast Claude
- `google/gemini-2.0-flash-exp:free` - Gemini (free)

**Chinese/Free Models (via OpenRouter):**
- `deepseek/deepseek-chat` - DeepSeek (very cheap)
- `qwen/qwen-2.5-72b-instruct` - Alibaba Qwen
- Various Zhipu AI (GLM) models when available

**Premium Models:**
- `anthropic/claude-sonnet-4` - Latest Claude (default)
- `openai/gpt-4o` - GPT-4o
- `google/gemini-2.0-flash-thinking-exp:free` - Gemini with thinking

---

## Quick Comparison

| Provider | Auth | Free Tier | Models | Best For |
|----------|------|-----------|--------|----------|
| **Copilot** | Device Flow | ❌ (Requires subscription) | GPT-4o class | GitHub users with Copilot |
| **Gemini** | API Key | ✅ Yes | Gemini 2.0 | Long context, free usage |
| **OpenRouter** | API Key | ✅ Many free models | 200+ models | Model variety, experimentation |

---

## Configuration Examples

### Use Different Providers for Different Tasks

```bash
# Use Gemini for long context tasks
tark chat --provider gemini

# Use OpenRouter with a free model for quick questions
tark chat --provider openrouter --model "google/gemma-2-9b-it:free"

# Use Copilot for GitHub-integrated workflows
tark chat --provider copilot
```

### Set Default Provider

```toml
# config.toml
[llm]
default_provider = "gemini"  # or "copilot", "openrouter"
```

---

## Troubleshooting

### GitHub Copilot: "Authentication Required"
- You'll see a prompt with a URL and code
- Visit the URL in your browser
- Enter the code shown
- Authorize Tark
- Token is saved automatically for future use

### Gemini: "API Key Not Set"
```bash
export GEMINI_API_KEY="your-key-here"
```

### OpenRouter: "Invalid API Key"
- Check https://openrouter.ai/keys
- Ensure you have credits (many models are free)
- Verify environment variable: `echo $OPENROUTER_API_KEY`

### General: "Provider Not Found"
```bash
# List available providers
tark --help

# Supported providers:
# - claude, anthropic
# - openai, gpt
# - ollama, local
# - copilot, github (NEW)
# - gemini, google (NEW)
# - openrouter (NEW)
```

---

## Examples

### Using Gemini for Code Review
```bash
export GEMINI_API_KEY="your-key"
tark chat --provider gemini
> Review the code in src/main.rs
```

### Using OpenRouter with Free Models
```bash
export OPENROUTER_API_KEY="your-key"
tark chat --provider openrouter --model "deepseek/deepseek-chat"
> Explain how async/await works in Rust
```

### Using Copilot (First Time)
```bash
tark chat --provider copilot
# Follow the on-screen instructions to authenticate
# Token is saved, no need to authenticate again
```

---

## Notes

- **Token Storage**: GitHub Copilot tokens are stored in `~/.config/tark/copilot_token.json`
- **Streaming**: All three providers support real-time streaming responses
- **Tool Calling**: Gemini and OpenRouter support tool calling; Copilot has limitations
- **Rate Limits**: Respect provider rate limits; use free tiers responsibly
- **Free Models**: OpenRouter provides access to many free/cheap models including Chinese providers

---

For more information, see:
- GitHub Copilot: https://docs.github.com/en/copilot
- Google Gemini: https://ai.google.dev/gemini-api
- OpenRouter: https://openrouter.ai/docs

