# Thinking/Reasoning Support Implementation Plan

This document provides a detailed implementation plan for adding unified thinking/reasoning support to tark. It integrates with the models.dev database for dynamic model information and provides a consistent abstraction layer across different LLM providers.

---

## Table of Contents

1. [Overview](#overview)
2. [Background: Provider-Specific Thinking APIs](#background-provider-specific-thinking-apis)
3. [Architecture](#architecture)
4. [Phase 1: Extend models.dev Integration](#phase-1-extend-modelsdev-integration)
5. [Phase 2: Configuration Layer](#phase-2-configuration-layer)
6. [Phase 3: Provider Integration](#phase-3-provider-integration)
7. [Phase 4: Model Picker Enhancement](#phase-4-model-picker-enhancement)
8. [Phase 5: Fix /thinking Toggle](#phase-5-fix-thinking-toggle)
9. [Phase 6: Thinking Block UI Enhancement](#phase-6-thinking-block-ui-enhancement)
10. [Phase 7: Enhanced /thinking Command](#phase-7-enhanced-thinking-command)
11. [Testing Checklist](#testing-checklist)
12. [Appendix: API Reference](#appendix-api-reference)

---

## Overview

### Goal

Enable unified thinking/reasoning support across multiple LLM providers (Claude, OpenAI, Gemini) with:

1. **Smart defaults** from models.dev database
2. **User-configurable overrides** via config.toml
3. **Cost awareness** with budget limits and estimates
4. **Enhanced UI** showing model capabilities and thinking content

### Key Discovery

The models.dev API already provides:
- `reasoning: bool` - Whether model supports thinking
- `cost.reasoning: float` - Separate reasoning token cost (when different from output)

Example from models.dev:
```json
{
  "id": "o1",
  "reasoning": true,
  "cost": {
    "input": 15,
    "output": 60,
    "reasoning": 60
  }
}
```

---

## Background: Provider-Specific Thinking APIs

### Claude (Anthropic)

```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 16000,
  "thinking": {
    "type": "enabled",
    "budget_tokens": 10000
  }
}
```

**Key parameter**: `thinking.budget_tokens` (u32)

### OpenAI (o1/o3 models)

```json
{
  "model": "o1",
  "reasoning_effort": "medium"
}
```

**Key parameter**: `reasoning_effort` (String: "low" | "medium" | "high")

### Gemini (Google)

```json
{
  "model": "gemini-2.0-flash-thinking-exp",
  "generationConfig": {
    "thinkingConfig": {
      "thinkingBudget": 8192
    }
  }
}
```

**Key parameter**: `thinkingConfig.thinkingBudget` (u32)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        models.dev API                           │
│  { reasoning: bool, cost.reasoning: float, limit.output: int }  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ModelsDbManager                              │
│  get_thinking_defaults(provider, model) -> ModelThinkingDefaults│
│    - suggested_budget: calculated from output limit             │
│    - cost_per_1k: from cost.reasoning or cost.output            │
│    - param_type: BudgetTokens | ReasoningEffort | ThinkingBudget│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                 ThinkingConfig (config.toml)                    │
│  enabled: bool                                                  │
│  max_budget_tokens: u32  (safety cap)                           │
│  fallback_reasoning_effort: String                              │
│  max_visible_lines: usize                                       │
│  auto_collapse: bool                                            │
│  models: HashMap<model_id, ModelThinkingOverride>               │
│    └─ per-model: budget_tokens, reasoning_effort, disabled      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│            ThinkingSettings::resolve(provider, model, config)   │
│                                                                 │
│  Priority: config.models[model] > model defaults > fallbacks   │
│  Safety:   min(resolved_budget, max_budget_tokens)              │
└─────────────────────────────────────────────────────────────────┘
                              │
            ┌─────────────────┼─────────────────┐
            ▼                 ▼                 ▼
    ┌───────────┐     ┌───────────┐     ┌───────────┐
    │  Claude   │     │  OpenAI   │     │  Gemini   │
    │ thinking: │     │reasoning_ │     │thinkConfig│
    │  budget   │     │  effort   │     │  Budget   │
    └───────────┘     └───────────┘     └───────────┘
```

---

## Phase 1: Extend models.dev Integration

### File: `src/llm/models_db.rs`

#### Step 1.1: Add `reasoning` to `ModelCost`

**Location**: `ModelCost` struct (around line 24-38)

**Current code**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}
```

**New code**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
    /// Reasoning/thinking token cost per million (if different from output)
    #[serde(default)]
    pub reasoning: Option<f64>,
}

impl ModelCost {
    /// Get the cost for reasoning tokens (falls back to output cost)
    pub fn reasoning_cost_per_million(&self) -> f64 {
        self.reasoning.unwrap_or(self.output)
    }
}
```

#### Step 1.2: Add `reasoning_cost` to `ModelCapabilities`

**Location**: `ModelCapabilities` struct (around line 591-608)

**Add new field and method**:
```rust
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub tool_call: bool,
    pub reasoning: bool,
    pub vision: bool,
    pub audio_input: bool,
    pub video_input: bool,
    pub pdf: bool,
    pub image_output: bool,
    pub audio_output: bool,
    pub structured_output: bool,
    pub temperature: bool,
    pub context_limit: u32,
    pub output_limit: u32,
    pub input_cost: f64,
    pub output_cost: f64,
    pub supports_caching: bool,
    /// Reasoning/thinking token cost per million
    pub reasoning_cost: f64,  // NEW
}

impl ModelCapabilities {
    // ... existing methods ...

    /// Estimate thinking cost for given token budget
    pub fn estimate_thinking_cost(&self, budget_tokens: u32) -> f64 {
        let cost_per_million = if self.reasoning_cost > 0.0 {
            self.reasoning_cost
        } else {
            self.output_cost
        };
        (budget_tokens as f64) * cost_per_million / 1_000_000.0
    }
}
```

#### Step 1.3: Update `get_capabilities` to populate `reasoning_cost`

**Location**: `ModelsDbManager::get_capabilities` method (around line 548-587)

**Update the method to include reasoning_cost**:
```rust
pub async fn get_capabilities(&self, provider: &str, model_id: &str) -> ModelCapabilities {
    if let Ok(Some(model)) = self.get_model(provider, model_id).await {
        ModelCapabilities {
            tool_call: model.tool_call,
            reasoning: model.reasoning,
            vision: model.supports_vision(),
            audio_input: model.supports_audio_input(),
            video_input: model.supports_video_input(),
            pdf: model.supports_pdf(),
            image_output: model.supports_image_output(),
            audio_output: model.supports_audio_output(),
            structured_output: model.structured_output.unwrap_or(false),
            temperature: model.temperature,
            context_limit: model.limit.context,
            output_limit: model.limit.output,
            input_cost: model.cost.input,
            output_cost: model.cost.output,
            supports_caching: model.cost.cache_read.is_some(),
            reasoning_cost: model.cost.reasoning_cost_per_million(),  // NEW
        }
    } else {
        // Fallback defaults
        ModelCapabilities {
            // ... existing fields ...
            reasoning_cost: 0.0,  // NEW
        }
    }
}
```

#### Step 1.4: Add `ThinkingParamType` enum

**Location**: Add after `ModelCapabilities` struct (around line 663)

```rust
/// Provider-specific thinking parameter type
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ThinkingParamType {
    #[default]
    None,
    /// Claude: thinking.budget_tokens
    BudgetTokens,
    /// OpenAI o1/o3: reasoning_effort (low/medium/high)
    ReasoningEffort,
    /// Gemini: thinkingConfig.thinkingBudget
    ThinkingBudget,
}
```

#### Step 1.5: Add `ModelThinkingDefaults` struct

**Location**: Add after `ThinkingParamType` enum

```rust
/// Thinking configuration defaults for a specific model
#[derive(Debug, Clone, Default)]
pub struct ModelThinkingDefaults {
    /// Whether this model supports thinking
    pub supported: bool,
    /// Suggested token budget (based on model's output limit)
    pub suggested_budget: u32,
    /// Cost per 1K thinking tokens
    pub cost_per_1k: f64,
    /// Provider-specific param type
    pub param_type: ThinkingParamType,
}
```

#### Step 1.6: Add `get_thinking_defaults` method to `ModelsDbManager`

**Location**: Add to `ModelsDbManager` impl block (after `get_capabilities`)

```rust
impl ModelsDbManager {
    // ... existing methods ...

    /// Get smart thinking defaults for a specific model from models.dev
    pub async fn get_thinking_defaults(
        &self,
        provider: &str,
        model_id: &str,
    ) -> ModelThinkingDefaults {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            if !model.reasoning {
                return ModelThinkingDefaults::default();
            }

            // Calculate suggested budget: 1/4 of output limit, min 8K, max 50K
            let suggested_budget = (model.limit.output / 4).max(8192).min(50000);

            // Get thinking cost (falls back to output cost)
            let cost_per_million = model.cost.reasoning.unwrap_or(model.cost.output);
            let cost_per_1k = cost_per_million / 1000.0;

            // Determine param type based on provider
            let param_type = match Self::normalize_provider(provider).as_str() {
                "anthropic" => ThinkingParamType::BudgetTokens,
                "openai" => ThinkingParamType::ReasoningEffort,
                "google" => ThinkingParamType::ThinkingBudget,
                _ => ThinkingParamType::BudgetTokens,
            };

            ModelThinkingDefaults {
                supported: true,
                suggested_budget,
                cost_per_1k,
                param_type,
            }
        } else {
            Self::fallback_thinking_defaults(provider, model_id)
        }
    }

    /// Fallback thinking defaults for unknown models
    fn fallback_thinking_defaults(provider: &str, model_id: &str) -> ModelThinkingDefaults {
        // Check known reasoning models by name pattern
        let is_reasoning = model_id.contains("o1")
            || model_id.contains("o3")
            || model_id.contains("thinking")
            || model_id.contains("sonnet-4")
            || model_id.contains("3-7-sonnet")
            || model_id.contains("deepseek-r1");

        if !is_reasoning {
            return ModelThinkingDefaults::default();
        }

        match provider.to_lowercase().as_str() {
            "openai" | "gpt" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 0, // Uses effort level, not tokens
                cost_per_1k: 0.06,   // ~$60/M for o1
                param_type: ThinkingParamType::ReasoningEffort,
            },
            "anthropic" | "claude" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 10_000,
                cost_per_1k: 0.015, // ~$15/M
                param_type: ThinkingParamType::BudgetTokens,
            },
            "google" | "gemini" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 8_192,
                cost_per_1k: 0.0, // Included in output
                param_type: ThinkingParamType::ThinkingBudget,
            },
            _ => ModelThinkingDefaults::default(),
        }
    }
}
```

#### Step 1.7: Export new types in `src/llm/mod.rs`

**Location**: `src/llm/mod.rs` line 17

**Current**:
```rust
pub use models_db::{models_db, ModelCapabilities};
```

**New**:
```rust
pub use models_db::{
    models_db, ModelCapabilities, ModelThinkingDefaults, ThinkingParamType,
};
```

---

## Phase 2: Configuration Layer

### File: `src/config/mod.rs`

#### Step 2.1: Add imports

**Location**: Top of file, add to imports

```rust
use std::collections::HashMap;
```

#### Step 2.2: Add `ThinkingConfig` struct

**Location**: After `ToolsConfig` struct (around line 218)

```rust
/// Configuration for thinking/reasoning features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThinkingConfig {
    /// Enable thinking by default
    pub enabled: bool,
    /// Maximum token budget allowed (cost protection)
    pub max_budget_tokens: u32,
    /// Fallback reasoning effort for OpenAI o1/o3: "low", "medium", "high"
    pub fallback_reasoning_effort: String,
    /// Maximum visible lines for thinking block in UI
    pub max_visible_lines: usize,
    /// Automatically collapse thinking block after response complete
    pub auto_collapse: bool,
    /// Per-model overrides (model_id -> settings)
    #[serde(default)]
    pub models: HashMap<String, ModelThinkingOverride>,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            enabled: false,                           // Opt-in (cost protection)
            max_budget_tokens: 50_000,                // ~$0.75 safety cap
            fallback_reasoning_effort: "medium".to_string(),
            max_visible_lines: 6,
            auto_collapse: false,
            models: HashMap::new(),
        }
    }
}

/// Per-model thinking configuration override
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelThinkingOverride {
    /// Override token budget (for Claude, Gemini)
    pub budget_tokens: Option<u32>,
    /// Override reasoning effort (for OpenAI o1/o3)
    pub reasoning_effort: Option<String>,
    /// Disable thinking for this model even if supported
    pub disabled: Option<bool>,
}
```

#### Step 2.3: Add `thinking` field to `Config` struct

**Location**: `Config` struct (around line 12)

**Current**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub completion: CompletionConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
}
```

**New**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub completion: CompletionConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
    pub thinking: ThinkingConfig,  // NEW
}
```

### File: `src/llm/types.rs`

#### Step 2.4: Add `ThinkingSettings` type

**Location**: After `StreamingResponseBuilder` (around line 393)

```rust
use crate::config::ThinkingConfig;
use crate::llm::models_db::{models_db, ThinkingParamType};

/// Per-request thinking settings (resolved from model defaults + config overrides)
#[derive(Debug, Clone, Default)]
pub struct ThinkingSettings {
    /// Whether thinking is enabled for this request
    pub enabled: bool,
    /// Token budget for Claude/Gemini
    pub budget_tokens: Option<u32>,
    /// Reasoning effort for OpenAI o1/o3
    pub reasoning_effort: Option<String>,
    /// Which API param type to use
    pub param_type: ThinkingParamType,
}

impl ThinkingSettings {
    /// Resolve settings: models.dev defaults -> config overrides
    ///
    /// Priority:
    /// 1. Per-model config override (config.thinking.models[model_id])
    /// 2. Model defaults from models.dev
    /// 3. Global config fallbacks
    ///
    /// Safety: Budget is capped at config.thinking.max_budget_tokens
    pub async fn resolve(
        provider: &str,
        model_id: &str,
        config: &ThinkingConfig,
    ) -> Self {
        let db = models_db();
        let defaults = db.get_thinking_defaults(provider, model_id).await;

        // Check if model supports thinking
        if !defaults.supported {
            return Self::default();
        }

        // Check for per-model config override
        let override_settings = config.models.get(model_id);

        // Check if explicitly disabled
        if let Some(ovr) = override_settings {
            if ovr.disabled.unwrap_or(false) {
                return Self::default();
            }
        }

        // Resolve budget_tokens: override > model default
        let budget_tokens = override_settings
            .and_then(|o| o.budget_tokens)
            .or(Some(defaults.suggested_budget))
            .map(|b| b.min(config.max_budget_tokens)); // Apply cap

        // Resolve reasoning_effort: override > fallback
        let reasoning_effort = override_settings
            .and_then(|o| o.reasoning_effort.clone())
            .or(Some(config.fallback_reasoning_effort.clone()));

        Self {
            enabled: config.enabled,
            budget_tokens,
            reasoning_effort,
            param_type: defaults.param_type,
        }
    }

    /// Create disabled settings
    pub fn disabled() -> Self {
        Self::default()
    }
}
```

#### Step 2.5: Export `ThinkingSettings` in `src/llm/mod.rs`

**Location**: `src/llm/mod.rs`

Add to the `pub use types::*;` line, or explicitly:
```rust
pub use types::{ThinkingSettings, /* ... other types ... */};
```

Since `types::*` is already exported, `ThinkingSettings` will be automatically exported.

---

## Phase 3: Provider Integration

### File: `src/llm/mod.rs`

#### Step 3.1: Update `LlmProvider` trait

**Location**: `LlmProvider` trait (around line 27-125)

**Add optional `thinking` parameter to `chat` and `chat_streaming`**:

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Check if the provider/model supports native extended thinking
    fn supports_native_thinking(&self) -> bool {
        false
    }

    /// Send a chat completion request (non-streaming)
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        thinking: Option<&ThinkingSettings>,  // NEW PARAMETER
    ) -> Result<LlmResponse>;

    /// Send a streaming chat completion request
    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        thinking: Option<&ThinkingSettings>,  // NEW PARAMETER
    ) -> Result<LlmResponse> {
        // Default implementation (fallback to non-streaming)
        if let Some(check) = interrupt_check {
            if check() {
                return Ok(LlmResponse::Text {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    usage: None,
                });
            }
        }

        let response = self.chat(messages, tools, thinking).await?;

        if let Some(text) = response.text() {
            callback(StreamEvent::TextDelta(text.to_string()));
        }

        for tool_call in response.tool_calls() {
            callback(StreamEvent::ToolCallStart {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
            });
            callback(StreamEvent::ToolCallDelta {
                id: tool_call.id.clone(),
                arguments_delta: tool_call.arguments.to_string(),
            });
            callback(StreamEvent::ToolCallComplete {
                id: tool_call.id.clone(),
            });
        }

        callback(StreamEvent::Done);
        Ok(response)
    }

    // ... rest of trait unchanged ...
}
```

### File: `src/llm/claude.rs`

#### Step 3.2: Update Claude `create_thinking_config` method

**Location**: Around line 116-125

**Current**:
```rust
fn create_thinking_config(&self) -> Option<ThinkingConfig> {
    if self.supports_extended_thinking() {
        Some(ThinkingConfig {
            thinking_type: "enabled".to_string(),
            budget_tokens: 10000,
        })
    } else {
        None
    }
}
```

**New**:
```rust
/// Create thinking config from ThinkingSettings
fn create_thinking_config(
    &self,
    thinking: Option<&super::ThinkingSettings>,
) -> Option<ClaudeThinkingConfig> {
    // Only enable if:
    // 1. Model supports it
    // 2. ThinkingSettings is provided and enabled
    if !self.supports_extended_thinking() {
        return None;
    }

    if let Some(settings) = thinking {
        if settings.enabled {
            return Some(ClaudeThinkingConfig {
                thinking_type: "enabled".to_string(),
                budget_tokens: settings.budget_tokens.unwrap_or(10000),
            });
        }
    }

    None
}
```

**Note**: Rename internal `ThinkingConfig` to `ClaudeThinkingConfig` to avoid confusion:

**Location**: Around line 664-668

```rust
#[derive(Debug, Serialize)]
struct ClaudeThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: u32,
}
```

**Update `ClaudeRequest` struct** (around line 648-661):
```rust
#[derive(Debug, Serialize)]
struct ClaudeRequest {
    // ... existing fields ...
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ClaudeThinkingConfig>,  // Updated type name
}
```

#### Step 3.3: Update Claude `chat` method signature

**Location**: Around line 162-231

**Update signature**:
```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    thinking: Option<&super::ThinkingSettings>,  // NEW
) -> Result<LlmResponse> {
    // ... existing code ...
    
    let mut request = ClaudeRequest {
        model: self.model.clone(),
        max_tokens: self.max_tokens,
        system,
        messages: claude_messages,
        tools: None,
        stream: None,
        thinking: self.create_thinking_config(thinking),  // UPDATED
    };
    
    // ... rest unchanged ...
}
```

#### Step 3.4: Update Claude `chat_streaming` method signature

**Location**: Around line 237-466

**Update signature**:
```rust
async fn chat_streaming(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    callback: StreamCallback,
    interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    thinking: Option<&super::ThinkingSettings>,  // NEW
) -> Result<LlmResponse> {
    // ... existing code ...
    
    let mut request = ClaudeRequest {
        model: self.model.clone(),
        max_tokens: self.max_tokens,
        system,
        messages: claude_messages,
        tools: None,
        stream: Some(true),
        thinking: self.create_thinking_config(thinking),  // UPDATED
    };
    
    // ... rest unchanged ...
}
```

### File: `src/llm/openai.rs`

#### Step 3.5: Update OpenAI `get_reasoning_effort` method

**Location**: Around line 236-241

**Current**:
```rust
fn get_reasoning_effort(&self) -> Option<String> {
    if self.supports_reasoning() {
        Some("medium".to_string()) // Default to medium
    } else {
        None
    }
}
```

**New**:
```rust
/// Get reasoning effort from ThinkingSettings
fn get_reasoning_effort(
    &self,
    thinking: Option<&super::ThinkingSettings>,
) -> Option<String> {
    // Only set for reasoning models when thinking is enabled
    if !self.supports_reasoning() {
        return None;
    }

    if let Some(settings) = thinking {
        if settings.enabled {
            return settings.reasoning_effort.clone();
        }
    }

    None
}
```

#### Step 3.6: Update OpenAI `chat` method

**Location**: Around line 530-638

**Update signature and body**:
```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    thinking: Option<&super::ThinkingSettings>,  // NEW
) -> Result<LlmResponse> {
    // ... existing code ...
    
    let request = OpenAiRequest {
        model: self.model.clone(),
        messages: openai_messages,
        max_completion_tokens: Some(self.max_tokens),
        tools: tools_vec,
        stream: None,
        reasoning_effort: self.get_reasoning_effort(thinking),  // UPDATED
    };
    
    // ... rest unchanged ...
}
```

#### Step 3.7: Update OpenAI `chat_streaming` method

**Location**: Around line 640-864

**Update signature and body similarly**.

### File: `src/llm/gemini.rs`

#### Step 3.8: Add thinking support to Gemini

**Location**: Around line 147-157

**Update `GeminiGenerationConfig` struct**:
```rust
#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingConfig")]
    thinking_config: Option<GeminiThinkingConfig>,  // NEW
}

#[derive(Debug, Serialize)]
struct GeminiThinkingConfig {
    #[serde(rename = "thinkingBudget")]
    thinking_budget: u32,
}
```

**Add helper method to `GeminiProvider`**:
```rust
impl GeminiProvider {
    // ... existing methods ...

    /// Create thinking config from ThinkingSettings
    fn create_thinking_config(
        &self,
        thinking: Option<&super::ThinkingSettings>,
    ) -> Option<GeminiThinkingConfig> {
        if !self.supports_native_thinking() {
            return None;
        }

        if let Some(settings) = thinking {
            if settings.enabled {
                return Some(GeminiThinkingConfig {
                    thinking_budget: settings.budget_tokens.unwrap_or(8192),
                });
            }
        }

        None
    }
}
```

#### Step 3.9: Update Gemini `chat` method

**Location**: Around line 140-203

**Update signature and body**:
```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    thinking: Option<&super::ThinkingSettings>,  // NEW
) -> Result<LlmResponse> {
    // ... existing code ...

    let mut request = GeminiRequest {
        contents,
        system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
            parts: vec![GeminiPart::Text { text }],
        }),
        generation_config: Some(GeminiGenerationConfig {
            max_output_tokens: Some(self.max_tokens),
            temperature: Some(1.0),
            thinking_config: self.create_thinking_config(thinking),  // NEW
        }),
        tools: None,
    };

    // ... rest unchanged ...
}
```

### Other Providers

#### Step 3.10: Update remaining providers

For `ollama.rs`, `copilot.rs`, and `openrouter.rs`:

1. Update `chat` signature to include `thinking: Option<&ThinkingSettings>`
2. Update `chat_streaming` signature to include `thinking: Option<&ThinkingSettings>`
3. For `openrouter.rs`, pass thinking settings through to the underlying API

**Example for Ollama** (doesn't support thinking):
```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    _thinking: Option<&super::ThinkingSettings>,  // Ignored
) -> Result<LlmResponse> {
    // ... existing implementation (thinking not supported) ...
}
```

---

## Phase 4: Model Picker Enhancement

### File: `src/tui/agent_bridge.rs`

#### Step 4.1: Add `PickerModelInfo` struct

**Location**: Add near top of file after imports

```rust
/// Rich model info for picker display
#[derive(Debug, Clone)]
pub struct PickerModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub reasoning: bool,
    pub cost_input: f64,
    pub cost_output: f64,
    pub cost_reasoning: Option<f64>,
}
```

#### Step 4.2: Add `list_available_models_full` method

**Location**: Add to `AgentBridge` impl

```rust
impl AgentBridge {
    // ... existing methods ...

    /// List available models with full metadata for picker
    pub async fn list_available_models_full(&self) -> Vec<PickerModelInfo> {
        let db = crate::llm::models_db();
        
        if let Ok(models) = db.list_models(&self.provider_name).await {
            models.into_iter().map(|m| PickerModelInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                description: m.capability_summary(),
                reasoning: m.reasoning,
                cost_input: m.cost.input,
                cost_output: m.cost.output,
                cost_reasoning: m.cost.reasoning,
            }).collect()
        } else {
            Vec::new()
        }
    }
}
```

### File: `src/tui/widgets/picker.rs`

#### Step 4.3: Add reasoning/cost fields to `PickerItem`

**Location**: `PickerItem` struct (around line 17-30)

**Current**:
```rust
pub struct PickerItem {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_active: bool,
    pub is_disabled: bool,
}
```

**New**:
```rust
pub struct PickerItem {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_active: bool,
    pub is_disabled: bool,
    /// Whether this model supports reasoning/thinking
    pub has_reasoning: bool,  // NEW
    /// Cost info for display (e.g., "$3/$15/M")
    pub cost_info: Option<String>,  // NEW
}
```

**Update `PickerItem::new`**:
```rust
impl PickerItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: None,
            is_active: false,
            is_disabled: false,
            has_reasoning: false,  // NEW
            cost_info: None,       // NEW
        }
    }

    /// Set whether model supports reasoning
    pub fn with_reasoning(mut self, has_reasoning: bool) -> Self {
        self.has_reasoning = has_reasoning;
        self
    }

    /// Set cost info string
    pub fn with_cost_info(mut self, cost_info: impl Into<String>) -> Self {
        self.cost_info = Some(cost_info.into());
        self
    }
}
```

#### Step 4.4: Add `ModelFilter` enum

**Location**: After `PickerItem` impl

```rust
/// Filter mode for model picker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelFilter {
    #[default]
    All,
    ThinkingOnly,
    StandardOnly,
}

impl ModelFilter {
    /// Cycle to the next filter mode
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::ThinkingOnly,
            Self::ThinkingOnly => Self::StandardOnly,
            Self::StandardOnly => Self::All,
        }
    }

    /// Get display label
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::ThinkingOnly => "🧠 Thinking",
            Self::StandardOnly => "Standard",
        }
    }
}
```

#### Step 4.5: Add filter to `Picker` struct

**Location**: `Picker` struct (around line 70-85)

**Add new field**:
```rust
pub struct Picker {
    title: String,
    items: Vec<PickerItem>,
    selected_index: usize,
    visible: bool,
    filter: String,
    filtered_indices: Vec<usize>,
    /// Model filter (for model pickers)
    pub model_filter: ModelFilter,  // NEW
}
```

**Update `Picker::new`**:
```rust
pub fn new(title: impl Into<String>) -> Self {
    Self {
        title: title.into(),
        items: Vec::new(),
        selected_index: 0,
        visible: false,
        filter: String::new(),
        filtered_indices: Vec::new(),
        model_filter: ModelFilter::default(),  // NEW
    }
}
```

#### Step 4.6: Update filter logic

**Location**: `Picker::update_filter` method

**Update to include model filter**:
```rust
fn update_filter(&mut self) {
    let filter_lower = self.filter.to_lowercase();
    self.filtered_indices = self
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            // Text filter
            let matches_text = filter_lower.is_empty()
                || item.label.to_lowercase().contains(&filter_lower)
                || item
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&filter_lower))
                    .unwrap_or(false);

            // Model filter (only applies if has_reasoning is set)
            let matches_model_filter = match self.model_filter {
                ModelFilter::All => true,
                ModelFilter::ThinkingOnly => item.has_reasoning,
                ModelFilter::StandardOnly => !item.has_reasoning,
            };

            matches_text && matches_model_filter
        })
        .map(|(i, _)| i)
        .collect();

    // Reset selection if out of bounds
    if !self.filtered_indices.is_empty() && self.selected_index >= self.filtered_indices.len() {
        self.selected_index = 0;
    }
}
```

#### Step 4.7: Add method to cycle filter

**Location**: Add to `Picker` impl

```rust
/// Cycle model filter to next mode
pub fn cycle_model_filter(&mut self) {
    self.model_filter = self.model_filter.next();
    self.update_filter();
}
```

### File: `src/tui/app.rs`

#### Step 4.8: Update model picker items creation

**Location**: Find `get_model_picker_items_for_provider_dynamic` function

**Update to use new fields**:
```rust
async fn get_model_picker_items_for_provider_dynamic(
    provider: &str,
    current_model: &str,
    agent_bridge: &AgentBridge,
) -> Vec<PickerItem> {
    let models = agent_bridge.list_available_models_full().await;
    
    models.into_iter().map(|m| {
        let cost_info = if m.cost_input > 0.0 || m.cost_output > 0.0 {
            Some(format!("${:.1}/${:.1}/M", m.cost_input, m.cost_output))
        } else {
            Some("free".to_string())
        };
        
        let icon = if m.reasoning { Some("🧠".to_string()) } else { None };
        
        PickerItem::new(&m.id, &m.name)
            .with_description(&m.description)
            .with_icon_opt(icon)
            .with_active(m.id == current_model)
            .with_reasoning(m.reasoning)
            .with_cost_info_opt(cost_info)
    }).collect()
}
```

#### Step 4.9: Handle Tab key for filter cycling

**Location**: Find `handle_picker_key` or similar key handler

**Add Tab key handling**:
```rust
KeyCode::Tab => {
    // Cycle model filter
    self.picker.cycle_model_filter();
}
```

---

## Phase 5: Fix /thinking Toggle

### Problem

Currently, `/thinking` only toggles the **display** of thinking blocks in the UI. It doesn't actually control whether thinking is **requested** from the LLM.

### File: `src/tui/app.rs`

#### Step 5.1: Pass thinking settings to agent

**Location**: Find where messages are sent to the agent (search for `send_message` or `agent_bridge`)

**Update the send flow**:
```rust
// When sending a message to the agent
async fn send_user_message(&mut self, content: String) {
    // Resolve thinking settings based on toggle state
    let thinking_settings = if self.state.thinking_mode {
        Some(ThinkingSettings::resolve(
            &self.agent_bridge.provider_name(),
            &self.agent_bridge.model_name(),
            &self.config.thinking,
        ).await)
    } else {
        None  // Don't request thinking at all (saves cost)
    };

    // Pass to agent bridge
    self.agent_bridge.send_message(content, thinking_settings).await;
}
```

#### Step 5.2: Update AgentBridge to accept ThinkingSettings

**Location**: `src/tui/agent_bridge.rs`

**Update relevant methods to accept and pass ThinkingSettings**:
```rust
pub async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    thinking: Option<&ThinkingSettings>,
) -> Result<LlmResponse> {
    self.provider.chat(messages, tools, thinking).await
}

pub async fn chat_streaming(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    callback: StreamCallback,
    interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    thinking: Option<&ThinkingSettings>,
) -> Result<LlmResponse> {
    self.provider.chat_streaming(messages, tools, callback, interrupt_check, thinking).await
}
```

#### Step 5.3: Show thinking cost estimate when enabled

**Location**: Toggle handler in `handle_command_result`

**Update the status message**:
```rust
ToggleSetting::Thinking => {
    self.state.thinking_mode = !self.state.thinking_mode;
    
    let status = if self.state.thinking_mode {
        // Get cost estimate
        let db = crate::llm::models_db();
        let caps = db.get_capabilities(
            &self.agent_bridge.provider_name(),
            &self.agent_bridge.model_name(),
        ).await;
        
        if caps.reasoning {
            let est_cost = caps.estimate_thinking_cost(10_000); // Assume 10K budget
            format!("Thinking enabled (~${:.3}/request)", est_cost)
        } else {
            "Thinking enabled (model may not support native thinking)".to_string()
        }
    } else {
        "Thinking disabled".to_string()
    };
    
    self.state.status_message = Some(status);
}
```

---

## Phase 6: Thinking Block UI Enhancement

### File: `src/tui/widgets/thinking_block.rs`

#### Step 6.1: Add scroll state to ThinkingBlock

**Location**: `ThinkingBlock` struct (around line 26-35)

**Update struct**:
```rust
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub id: String,
    pub content: Vec<String>,
    pub expanded: bool,
    pub is_streaming: bool,
    /// Current scroll position
    pub scroll_offset: usize,  // NEW
    /// Maximum visible lines (from config)
    pub max_visible_lines: usize,  // NEW
    /// Whether this block has focus for scrolling
    pub is_focused: bool,  // NEW
}

impl ThinkingBlock {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: Vec::new(),
            expanded: true,
            is_streaming: false,
            scroll_offset: 0,
            max_visible_lines: 6,  // Default, can be overridden from config
            is_focused: false,
        }
    }
}
```

#### Step 6.2: Add scroll methods

**Location**: Add to `ThinkingBlock` impl

```rust
impl ThinkingBlock {
    // ... existing methods ...

    /// Set max visible lines from config
    pub fn with_max_visible_lines(mut self, lines: usize) -> Self {
        self.max_visible_lines = lines;
        self
    }

    /// Scroll up within the thinking block
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    /// Scroll down within the thinking block
    pub fn scroll_down(&mut self) {
        let max_offset = self.content.len().saturating_sub(self.max_visible_lines);
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    /// Auto-scroll to bottom when streaming
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.content.len().saturating_sub(self.max_visible_lines);
    }

    /// Get visible content slice
    pub fn visible_content(&self) -> &[String] {
        let start = self.scroll_offset;
        let end = (start + self.max_visible_lines).min(self.content.len());
        if start < self.content.len() {
            &self.content[start..end]
        } else {
            &[]
        }
    }

    /// Check if there's content above the visible area
    pub fn has_content_above(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Check if there's content below the visible area
    pub fn has_content_below(&self) -> bool {
        self.scroll_offset + self.max_visible_lines < self.content.len()
    }

    /// Get lines above count
    pub fn lines_above(&self) -> usize {
        self.scroll_offset
    }

    /// Get lines below count
    pub fn lines_below(&self) -> usize {
        self.content.len().saturating_sub(self.scroll_offset + self.max_visible_lines)
    }

    /// Get scroll position string
    pub fn scroll_position(&self) -> String {
        if self.content.is_empty() {
            String::new()
        } else {
            let current = self.scroll_offset + 1;
            let total = self.content.len();
            format!("[{}/{}]", current, total)
        }
    }
}
```

#### Step 6.3: Update render_lines for fixed height and scrolling

**Location**: `render_lines` method (around line 141-220)

**Replace with new implementation**:
```rust
pub fn render_lines(&self) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let border_style = Style::default().fg(Color::DarkGray);
    let header_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);
    let thinking_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::ITALIC);
    let scroll_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    let streaming_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::SLOW_BLINK);

    // Header with line count and scroll position
    let header_text = if self.content.is_empty() {
        self.display_header()
    } else {
        format!(
            "{} ({} lines) {}",
            self.display_header(),
            self.content.len(),
            self.scroll_position()
        )
    };
    lines.push(Line::from(Span::styled(
        format!("╭── {} ", header_text),
        header_style,
    )));

    // Content if expanded
    if self.expanded {
        if self.content.is_empty() {
            // Show placeholder when empty
            if self.is_streaming {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled("Thinking...", streaming_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(
                        "(no thinking content)",
                        Style::default().fg(Color::Gray),
                    ),
                ]));
            }
        } else {
            // Show "more above" indicator
            if self.has_content_above() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(
                        format!("↑ {} more lines above", self.lines_above()),
                        scroll_style,
                    ),
                ]));
            }

            // Render only visible content (fixed height)
            for line in self.visible_content() {
                // Parse inline markdown for styling
                let styled_spans = self.render_markdown_line(line);
                let mut span_vec = vec![Span::styled("│ ", border_style)];
                span_vec.extend(styled_spans);
                lines.push(Line::from(span_vec));
            }

            // Show "more below" indicator
            if self.has_content_below() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(
                        format!("↓ {} more lines below", self.lines_below()),
                        scroll_style,
                    ),
                ]));
            }

            // Show streaming indicator at the end if still streaming
            if self.is_streaming {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled("...", streaming_style),
                ]));
            }
        }

        // Closing border
        lines.push(Line::from(Span::styled(
            "╰──────────────────────────────────────────────────",
            border_style,
        )));
    }

    lines
}
```

#### Step 6.4: Add markdown rendering helper

**Location**: Add to `ThinkingBlock` impl

```rust
impl ThinkingBlock {
    // ... existing methods ...

    /// Render a line with basic markdown formatting
    fn render_markdown_line(&self, line: &str) -> Vec<Span<'static>> {
        let thinking_style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC);
        let bold_style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC);
        let code_style = Style::default()
            .fg(Color::Yellow)
            .bg(Color::DarkGray);

        let mut spans = Vec::new();
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        let mut in_bold = false;
        let mut in_code = false;

        while let Some(c) = chars.next() {
            match c {
                '*' if chars.peek() == Some(&'*') => {
                    // Toggle bold
                    chars.next();
                    if !current.is_empty() {
                        let style = if in_code {
                            code_style
                        } else if in_bold {
                            bold_style
                        } else {
                            thinking_style
                        };
                        spans.push(Span::styled(std::mem::take(&mut current), style));
                    }
                    in_bold = !in_bold;
                }
                '`' => {
                    // Toggle inline code
                    if !current.is_empty() {
                        let style = if in_code {
                            code_style
                        } else if in_bold {
                            bold_style
                        } else {
                            thinking_style
                        };
                        spans.push(Span::styled(std::mem::take(&mut current), style));
                    }
                    in_code = !in_code;
                }
                _ => current.push(c),
            }
        }

        if !current.is_empty() {
            let style = if in_code {
                code_style
            } else if in_bold {
                bold_style
            } else {
                thinking_style
            };
            spans.push(Span::styled(current, style));
        }

        if spans.is_empty() {
            spans.push(Span::styled(String::new(), thinking_style));
        }

        spans
    }
}
```

#### Step 6.5: Update ThinkingBlockManager

**Location**: `ThinkingBlockManager` (around line 258-341)

**Add config support**:
```rust
#[derive(Debug, Clone, Default)]
pub struct ThinkingBlockManager {
    blocks: Vec<ThinkingBlock>,
    max_visible_lines: usize,  // NEW
}

impl ThinkingBlockManager {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            max_visible_lines: 6,  // Default
        }
    }

    /// Create with config settings
    pub fn with_config(max_visible_lines: usize) -> Self {
        Self {
            blocks: Vec::new(),
            max_visible_lines,
        }
    }

    /// Add a new thinking block with config settings
    pub fn add(&mut self, message_id: &str) -> &mut ThinkingBlock {
        let index = self.blocks.len();
        let mut block = ThinkingBlock::from_message(message_id, index);
        block.max_visible_lines = self.max_visible_lines;
        self.blocks.push(block);
        self.blocks.last_mut().unwrap()
    }

    // ... rest of methods ...
}
```

---

## Phase 7: Enhanced /thinking Command

### File: `src/tui/commands.rs`

#### Step 7.1: Add new command result variants

**Location**: `CommandResult` enum (around line 12-67)

**Add new variants**:
```rust
pub enum CommandResult {
    // ... existing variants ...
    
    /// Set thinking budget for current session
    SetThinkingBudget(u32),
    /// Set reasoning effort for current session
    SetReasoningEffort(String),
    /// Show thinking cost estimate
    ShowThinkingCost,
    /// Show thinking config for current model
    ShowThinkingConfig,
    /// Reset thinking settings to defaults
    ResetThinkingSettings,
}
```

#### Step 7.2: Update /thinking command handling

**Location**: Find thinking command registration

**Update to support subcommands**:
```rust
// In command registry or execute_command function:

// /thinking - toggle
// /thinking budget <N> - set budget
// /thinking effort <low|medium|high> - set effort
// /thinking cost - show cost
// /thinking config - show config
// /thinking reset - reset to defaults

fn parse_thinking_command(args: &str) -> CommandResult {
    let args = args.trim();
    
    if args.is_empty() {
        return CommandResult::Toggle(ToggleSetting::Thinking);
    }
    
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let subcommand = parts[0].to_lowercase();
    let value = parts.get(1).map(|s| s.trim());
    
    match subcommand.as_str() {
        "on" | "enable" => {
            // Force enable
            CommandResult::Toggle(ToggleSetting::Thinking)  // Will toggle, may need different handling
        }
        "off" | "disable" => {
            // Force disable
            CommandResult::Toggle(ToggleSetting::Thinking)  // Will toggle, may need different handling
        }
        "budget" => {
            if let Some(val) = value {
                if let Ok(budget) = val.parse::<u32>() {
                    return CommandResult::SetThinkingBudget(budget);
                }
            }
            CommandResult::Error("Usage: /thinking budget <number>".to_string())
        }
        "effort" => {
            if let Some(val) = value {
                let effort = val.to_lowercase();
                if matches!(effort.as_str(), "low" | "medium" | "high") {
                    return CommandResult::SetReasoningEffort(effort);
                }
            }
            CommandResult::Error("Usage: /thinking effort <low|medium|high>".to_string())
        }
        "cost" => CommandResult::ShowThinkingCost,
        "config" => CommandResult::ShowThinkingConfig,
        "reset" => CommandResult::ResetThinkingSettings,
        _ => CommandResult::Error(format!("Unknown /thinking subcommand: {}", subcommand)),
    }
}
```

#### Step 7.3: Handle new command results in app.rs

**Location**: `handle_command_result` in `src/tui/app.rs`

**Add handlers**:
```rust
CommandResult::SetThinkingBudget(budget) => {
    self.state.session_thinking_budget = Some(budget);
    self.state.status_message = Some(format!("Thinking budget set to {} tokens", budget));
}
CommandResult::SetReasoningEffort(effort) => {
    self.state.session_reasoning_effort = Some(effort.clone());
    self.state.status_message = Some(format!("Reasoning effort set to {}", effort));
}
CommandResult::ShowThinkingCost => {
    let db = crate::llm::models_db();
    let caps = db.get_capabilities(
        &self.agent_bridge.provider_name(),
        &self.agent_bridge.model_name(),
    ).await;
    
    if caps.reasoning {
        let budget = self.state.session_thinking_budget.unwrap_or(10_000);
        let cost = caps.estimate_thinking_cost(budget);
        self.state.status_message = Some(format!(
            "Estimated cost: ${:.4} for {} tokens (${:.2}/M)",
            cost,
            budget,
            caps.reasoning_cost
        ));
    } else {
        self.state.status_message = Some("Current model doesn't support thinking".to_string());
    }
}
CommandResult::ShowThinkingConfig => {
    let db = crate::llm::models_db();
    let defaults = db.get_thinking_defaults(
        &self.agent_bridge.provider_name(),
        &self.agent_bridge.model_name(),
    ).await;
    
    let msg = format!(
        "Model: {} | Supported: {} | Budget: {} | Type: {:?}",
        self.agent_bridge.model_name(),
        defaults.supported,
        defaults.suggested_budget,
        defaults.param_type,
    );
    self.state.status_message = Some(msg);
}
CommandResult::ResetThinkingSettings => {
    self.state.session_thinking_budget = None;
    self.state.session_reasoning_effort = None;
    self.state.status_message = Some("Thinking settings reset to defaults".to_string());
}
```

#### Step 7.4: Add session state for thinking overrides

**Location**: `AppState` struct in `src/tui/app.rs`

**Add fields**:
```rust
pub struct AppState {
    // ... existing fields ...
    
    /// Session-level thinking budget override
    pub session_thinking_budget: Option<u32>,
    /// Session-level reasoning effort override  
    pub session_reasoning_effort: Option<String>,
}
```

---

## Testing Checklist

### Unit Tests

- [ ] `ModelCost::reasoning_cost_per_million()` returns correct values
- [ ] `ModelCapabilities::estimate_thinking_cost()` calculates correctly
- [ ] `ThinkingSettings::resolve()` applies priority correctly
- [ ] `ThinkingBlock` scroll methods work correctly
- [ ] `ModelFilter` filtering works correctly
- [ ] Markdown rendering in thinking blocks works

### Integration Tests

- [ ] Claude provider sends `thinking.budget_tokens` when enabled
- [ ] OpenAI provider sends `reasoning_effort` when enabled
- [ ] Gemini provider sends `thinkingConfig.thinkingBudget` when enabled
- [ ] `/thinking` toggle actually enables/disables thinking API calls
- [ ] Model picker shows 🧠 indicator for reasoning models
- [ ] Config overrides are respected

### Manual Tests

1. **Basic thinking toggle**:
   ```
   /thinking
   > "Thinking enabled (~$0.15/request)"
   /thinking
   > "Thinking disabled"
   ```

2. **Model picker filter**:
   - Press Tab to cycle filters
   - Verify 🧠 icons show for reasoning models

3. **Thinking block scrolling**:
   - Long thinking content should show scroll indicators
   - j/k should scroll within block when focused

4. **Cost estimation**:
   ```
   /thinking cost
   > "Estimated cost: $0.15 for 10000 tokens ($15/M)"
   ```

---

## Appendix: API Reference

### models.dev Response Structure

```json
{
  "anthropic": {
    "models": {
      "claude-sonnet-4-20250514": {
        "id": "claude-sonnet-4-20250514",
        "name": "Claude Sonnet 4",
        "reasoning": true,
        "tool_call": true,
        "cost": {
          "input": 3,
          "output": 15,
          "reasoning": 15
        },
        "limit": {
          "context": 200000,
          "output": 64000
        }
      }
    }
  }
}
```

### Example config.toml

```toml
[thinking]
enabled = true
max_budget_tokens = 100000
fallback_reasoning_effort = "medium"
max_visible_lines = 6
auto_collapse = false

[thinking.models."claude-sonnet-4-20250514"]
budget_tokens = 20000

[thinking.models."o1"]
reasoning_effort = "high"

[thinking.models."o3-mini"]
reasoning_effort = "low"

[thinking.models."gpt-4o"]
disabled = true
```

### Provider API Parameter Mapping

| Provider | ThinkingSettings Field | API Parameter |
|----------|------------------------|---------------|
| Claude | `budget_tokens` | `thinking.budget_tokens` |
| OpenAI | `reasoning_effort` | `reasoning_effort` |
| Gemini | `budget_tokens` | `thinkingConfig.thinkingBudget` |

### Cost Reference (per million tokens)

| Model | Input | Output | Reasoning |
|-------|-------|--------|-----------|
| Claude Sonnet 4 | $3 | $15 | $15 |
| Claude Opus 4 | $15 | $75 | $75 |
| OpenAI o1 | $15 | $60 | $60 |
| OpenAI o3-mini | $1.1 | $4.4 | $4.4 |
| Gemini Thinking | free | free | free |

---

## Summary

This plan provides a comprehensive implementation guide for adding thinking/reasoning support to tark. Key benefits:

1. **Unified abstraction** via `ThinkingSettings`
2. **Smart defaults** from models.dev
3. **User control** via config overrides
4. **Cost awareness** with estimates and budgets
5. **Enhanced UI** with filters and scrolling

Follow the phases in order, as later phases depend on earlier ones. Run tests after each phase to ensure correctness.

