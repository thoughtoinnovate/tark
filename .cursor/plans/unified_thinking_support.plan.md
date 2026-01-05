# Unified Thinking/Reasoning Support with models.dev Integration

## Key Discovery: models.dev Already Has Reasoning Cost Data

The models.dev API already provides a **`cost.reasoning`** field for models with separate thinking/reasoning token pricing! We should extend our existing `ModelCost` struct to capture this.

### models.dev Schema for Reasoning Models

```json
{
  "id": "o1",
  "reasoning": true,
  "cost": {
    "input": 15,
    "output": 60,
    "reasoning": 60,  // <-- Separate reasoning token cost!
    "cache_read": 7.5
  }
}
```

---

## Implementation Plan: Abstract Config Layer via models.dev

### Phase 1: Extend ModelCost to Include Reasoning

Update `src/llm/models_db.rs`:

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
    /// Reasoning/thinking token cost (if different from output)
    #[serde(default)]
    pub reasoning: Option<f64>,
}
```

This automatically syncs with models.dev since you're already fetching from their API!

### Phase 2: Per-Model Smart Defaults from models.dev

Add to `src/llm/models_db.rs`:

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

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ThinkingParamType {
    #[default]
    None,
    /// Claude: budget_tokens
    BudgetTokens,
    /// OpenAI o1/o3: reasoning_effort (low/medium/high)
    ReasoningEffort,
    /// Gemini: thinkingBudget
    ThinkingBudget,
}

impl ModelsDbManager {
    /// Get smart thinking defaults for a specific model from models.dev
    pub async fn get_thinking_defaults(
        &self, 
        provider: &str, 
        model_id: &str
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
                suggested_budget: 0,      // Uses effort level, not tokens
                cost_per_1k: 0.06,        // ~$60/M for o1
                param_type: ThinkingParamType::ReasoningEffort,
            },
            "anthropic" | "claude" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 10_000,
                cost_per_1k: 0.015,       // ~$15/M
                param_type: ThinkingParamType::BudgetTokens,
            },
            "google" | "gemini" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 8_192,
                cost_per_1k: 0.0,         // Included in output
                param_type: ThinkingParamType::ThinkingBudget,
            },
            _ => ModelThinkingDefaults::default(),
        }
    }
}
```

### Phase 3: Add ThinkingConfig with Per-Model Overrides

Add to `src/config/mod.rs`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThinkingConfig {
    /// Enable thinking by default
    pub enabled: bool,
    /// Maximum token budget allowed (cost protection)
    pub max_budget_tokens: u32,
    /// Fallback reasoning effort for OpenAI o1/o3: "low", "medium", "high"
    pub fallback_reasoning_effort: String,
    /// Per-model overrides (model_id -> settings)
    #[serde(default)]
    pub models: HashMap<String, ModelThinkingOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelThinkingOverride {
    /// Override token budget (for Claude, Gemini)
    pub budget_tokens: Option<u32>,
    /// Override reasoning effort (for OpenAI o1/o3)
    pub reasoning_effort: Option<String>,
    /// Disable thinking for this model even if supported
    pub disabled: Option<bool>,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            enabled: false,                    // Opt-in (cost protection)
            max_budget_tokens: 50_000,         // ~$0.75 cap
            fallback_reasoning_effort: "medium".to_string(),
            models: HashMap::new(),
        }
    }
}
```

### Example Config File with Per-Model Overrides

```toml
# ~/.config/tark/config.toml

[thinking]
enabled = true
max_budget_tokens = 100000
fallback_reasoning_effort = "medium"

# Per-model overrides
[thinking.models."claude-sonnet-4-20250514"]
budget_tokens = 20000    # More thinking for complex tasks

[thinking.models."o1"]
reasoning_effort = "high"  # Maximum reasoning

[thinking.models."o3-mini"]
reasoning_effort = "low"   # Faster, cheaper

[thinking.models."gemini-2.0-flash-thinking-exp"]
budget_tokens = 16000

[thinking.models."gpt-4o"]
disabled = true           # Don't use thinking prompt injection
```

### Phase 3: Add Cost Estimation Helpers

Add to `src/llm/models_db.rs`:

```rust
impl ModelCost {
    /// Get the cost for reasoning tokens (falls back to output cost)
    pub fn reasoning_cost_per_million(&self) -> f64 {
        self.reasoning.unwrap_or(self.output)
    }
}

impl ModelCapabilities {
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

### Phase 4: Add ThinkingSettings Runtime Type

Add to `src/llm/types.rs`:

```rust
/// Per-request thinking settings (resolved from model defaults + config overrides)
#[derive(Debug, Clone, Default)]
pub struct ThinkingSettings {
    pub enabled: bool,
    pub budget_tokens: Option<u32>,        // Claude, Gemini
    pub reasoning_effort: Option<String>,  // OpenAI o1/o3
    pub param_type: ThinkingParamType,     // Which API param to use
}

impl ThinkingSettings {
    /// Resolve settings: models.dev defaults -> config overrides -> runtime
    pub async fn resolve(
        provider: &str,
        model_id: &str,
        config: &ThinkingConfig,
    ) -> Self {
        let models_db = crate::llm::models_db();
        let defaults = models_db.get_thinking_defaults(provider, model_id).await;
        
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
}
```

### Phase 5: Update LlmProvider Trait

```rust
async fn chat(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    thinking: Option<&ThinkingSettings>,
) -> Result<LlmResponse>;
```

### Phase 6: Update Provider Implementations

| Provider | Config Field Used | API Parameter |
|----------|-------------------|---------------|
| Claude | `budget_tokens` | `thinking.budget_tokens` |
| OpenAI | `reasoning_effort` | `reasoning_effort` |
| Gemini | `budget_tokens` | `thinkingConfig.thinkingBudget` |

### Phase 7: UI Updates

1. **Model Picker**: Add 🧠 indicator for `reasoning: true` models
2. **Show cost estimate**: Use `cost.reasoning` when available
3. **/thinking command**: Support `budget`, `effort`, `cost` subcommands

---

## Architecture: Smart Defaults + Config Overrides

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

### Resolution Flow Example

```
User selects: claude-sonnet-4-20250514
         │
         ▼
1. models.dev: { reasoning: true, limit.output: 64000 }
         │
         ▼
2. ModelThinkingDefaults {
     suggested_budget: 16000 (64000/4),
     param_type: BudgetTokens
   }
         │
         ▼
3. config.toml: [thinking.models."claude-sonnet-4-20250514"]
                budget_tokens = 20000  (override!)
         │
         ▼
4. ThinkingSettings {
     enabled: true,
     budget_tokens: 20000,  (from config override)
     param_type: BudgetTokens
   }
         │
         ▼
5. Claude API: { thinking: { type: "enabled", budget_tokens: 20000 } }
```

---

## Pricing Reference (from models.dev)

| Model | Input | Output | Reasoning | Notes |
|-------|-------|--------|-----------|-------|
| Claude Sonnet 4 | $3/M | $15/M | $15/M | Same as output |
| OpenAI o1 | $15/M | $60/M | $60/M | Same as output |
| OpenAI o3-mini | $1.1/M | $4.4/M | $4.4/M | Affordable |
| Qwen3-32b | $0.7/M | $2.8/M | $8.4/M | 3x output cost! |
| xAI Grok-4 | $3/M | $15/M | $15/M | Same as output |

---

## Default Values Summary

### Global Config Defaults

| Setting | Default | Rationale |
|---------|---------|-----------|
| `enabled` | `false` | Cost protection - opt-in |
| `max_budget_tokens` | `50,000` | ~$0.75 safety cap |
| `fallback_reasoning_effort` | `"medium"` | OpenAI recommendation |

### Smart Per-Model Defaults (from models.dev)

| Model | Param Type | Suggested Budget | Est. Cost |
|-------|------------|------------------|-----------|
| Claude Sonnet 4 | budget_tokens | 16,000 (64K/4) | ~$0.24 |
| Claude Opus 4 | budget_tokens | 8,000 (32K/4) | ~$0.60 |
| OpenAI o1 | reasoning_effort | "medium" | varies |
| OpenAI o3-mini | reasoning_effort | "medium" | varies |
| Gemini Thinking | thinkingBudget | 8,192 | free |

### Example Config Overrides

```toml
[thinking]
enabled = true
max_budget_tokens = 100000

[thinking.models."claude-sonnet-4-20250514"]
budget_tokens = 25000     # More thinking for complex tasks

[thinking.models."o1"]
reasoning_effort = "high" # Maximum reasoning

[thinking.models."o3-mini"]
reasoning_effort = "low"  # Faster, cheaper for simple tasks

[thinking.models."gpt-4o"]
disabled = true           # This model doesn't support thinking
```

---

## Model Picker with Thinking Filter

### Current State
- `list_available_models()` in `agent_bridge.rs` already uses models.dev
- Returns `(model_id, display_name, description)` tuples
- `capability_summary()` already includes "reasoning" text

### Proposed Enhancement

#### 1. Return Rich Model Data

Change return type in `src/tui/agent_bridge.rs`:

```rust
/// Model info for picker display
#[derive(Debug, Clone)]
pub struct PickerModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub reasoning: bool,           // From models.dev
    pub cost_input: f64,           // $/M tokens
    pub cost_output: f64,          // $/M tokens  
    pub cost_reasoning: Option<f64>, // $/M tokens (if different)
}

/// List available models with full metadata
pub async fn list_available_models_full(&self) -> Vec<PickerModelInfo> {
    let models_db = crate::llm::models_db();
    if let Ok(models) = models_db.list_models(&self.provider_name).await {
        models.into_iter().map(|m| PickerModelInfo {
            id: m.id,
            name: m.name,
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
```

#### 2. Add Filter Tabs to Model Picker

Update model picker UI in `src/tui/app.rs`:

```
┌─────────────────────────────────────────┐
│  Select Model                           │
├─────────────────────────────────────────┤
│  [All] [🧠 Thinking] [Standard]         │  ← Tab filter
├─────────────────────────────────────────┤
│  🧠 Claude Sonnet 4         $3/$15/M    │
│  🧠 Claude 3.7 Sonnet       $3/$15/M    │
│     Claude 3.5 Haiku        $1/$5/M     │
│  🧠 Claude Opus 4           $15/$75/M   │
└─────────────────────────────────────────┘
```

#### 3. Enhance PickerItem with Reasoning Flag

Add to `src/tui/widgets/picker.rs`:

```rust
pub struct PickerItem {
    // ... existing fields ...
    /// Whether this model supports reasoning/thinking
    pub has_reasoning: bool,
    /// Cost info for display
    pub cost_info: Option<String>,
}
```

#### 4. Add Filter State to Picker

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelFilter {
    All,
    ThinkingOnly,
    StandardOnly,
}

pub struct Picker {
    // ... existing fields ...
    model_filter: ModelFilter,
}
```

#### 5. Keyboard Shortcuts for Filter

- `Tab` or `1/2/3` - Cycle through filter modes
- Type to search (existing functionality)

### Visual Design

```
All Models:
  🧠 o1               Advanced reasoning ($15/$60/M)
  🧠 o3-mini          Fast reasoning ($1.1/$4.4/M)
     gpt-4o           Most capable ($2.5/$10/M)
     gpt-4o-mini      Fast and affordable ($0.15/$0.6/M)

Thinking Only (filtered):
  🧠 o1               Advanced reasoning ($15/$60/M)
  🧠 o3-mini          Fast reasoning ($1.1/$4.4/M)

Standard Only (filtered):
     gpt-4o           Most capable ($2.5/$10/M)
     gpt-4o-mini      Fast and affordable ($0.15/$0.6/M)
```

---

## TODOs

### Phase 1: Extend models.dev Integration
1. [ ] Add `reasoning: Option<f64>` to `ModelCost` struct in `models_db.rs`
2. [ ] Add `ModelThinkingDefaults` struct with `suggested_budget`, `cost_per_1k`, `param_type`
3. [ ] Add `ThinkingParamType` enum (BudgetTokens, ReasoningEffort, ThinkingBudget)
4. [ ] Add `get_thinking_defaults(provider, model)` method to `ModelsDbManager`
5. [ ] Add `fallback_thinking_defaults()` for unknown models
6. [ ] Add `reasoning_cost` field to `ModelCapabilities`
7. [ ] Add `estimate_thinking_cost()` helper method

### Phase 2: Configuration Layer with Per-Model Overrides
8. [ ] Add `ThinkingConfig` struct to `src/config/mod.rs`
9. [ ] Add `ModelThinkingOverride` struct (budget_tokens, reasoning_effort, disabled)
10. [ ] Add `models: HashMap<String, ModelThinkingOverride>` to ThinkingConfig
11. [ ] Add `thinking: ThinkingConfig` to main `Config` struct
12. [ ] Add `ThinkingSettings` type to `src/llm/types.rs`
13. [ ] Implement `ThinkingSettings::resolve()` with priority: config override > model defaults

### Phase 3: Provider Integration
14. [ ] Update `LlmProvider` trait to accept `ThinkingSettings`
15. [ ] Wire Claude provider to use `budget_tokens` from resolved settings
16. [ ] Wire OpenAI provider to use `reasoning_effort` from resolved settings
17. [ ] Add `thinkingBudget` support to Gemini provider
18. [ ] Update OpenRouter to pass through thinking settings

### Phase 4: Model Picker Enhancement
19. [ ] Create `PickerModelInfo` struct with reasoning flag and cost info
20. [ ] Add `list_available_models_full()` to AgentBridge
21. [ ] Add 🧠 icon to PickerItem for reasoning models
22. [ ] Add `ModelFilter` enum (All, ThinkingOnly, StandardOnly)
23. [ ] Add filter state to Picker widget
24. [ ] Add Tab key handler to cycle filter modes
25. [ ] Show thinking cost estimates in model descriptions
26. [ ] Show per-model override indicator if configured

### Phase 5: Thinking Block UI Enhancement
27. [ ] Add `max_visible_lines` config to ThinkingBlock (default: 6)
28. [ ] Add `scroll_offset` state for internal scrolling
29. [ ] Add scroll indicators (↑ more above / ↓ more below)
30. [ ] Add keyboard navigation (j/k or ↑/↓) when block is focused
31. [ ] Show line count and scroll position in header
32. [ ] Ensure fixed height doesn't pollute conversation

### Phase 6: Commands
33. [ ] Enhance `/thinking` command with subcommands:
    - `/thinking` - Toggle on/off
    - `/thinking budget <N>` - Set session budget override
    - `/thinking effort <low|medium|high>` - Set session effort override
    - `/thinking cost` - Show estimated cost for current model
    - `/thinking config` - Show resolved settings for current model
    - `/thinking reset` - Clear session overrides

---

## Thinking Block UI Design

### Fixed Height Scrollable Box

```
╭── 🧠 Thinking (42 lines) [3/42] ────────────────╮
│ ↑ 2 more lines above                            │
├─────────────────────────────────────────────────┤
│ Let me analyze this step by step...             │
│ First, I need to understand the requirements:   │
│ 1. The user wants to implement thinking support │
│ 2. Different providers have different APIs      │
│ 3. We need a unified abstraction layer          │
│ 4. Cost tracking is important                   │
├─────────────────────────────────────────────────┤
│ ↓ 35 more lines below                           │
╰─────────────────────────────────────────────────╯
```

### ThinkingBlock Struct Enhancements

```rust
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub id: String,
    pub content: Vec<String>,
    pub expanded: bool,
    pub is_streaming: bool,
    
    // New fields for scrolling
    pub scroll_offset: usize,        // Current scroll position
    pub max_visible_lines: usize,    // Fixed height (default: 6)
    pub is_focused: bool,            // For keyboard navigation
}

impl ThinkingBlock {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: Vec::new(),
            expanded: true,
            is_streaming: false,
            scroll_offset: 0,
            max_visible_lines: 6,  // Show only 6 lines at a time
            is_focused: false,
        }
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
        &self.content[start..end]
    }

    /// Check if there's content above the visible area
    pub fn has_content_above(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Check if there's content below the visible area
    pub fn has_content_below(&self) -> bool {
        self.scroll_offset + self.max_visible_lines < self.content.len()
    }

    /// Get scroll position indicator text
    pub fn scroll_position(&self) -> String {
        if self.content.is_empty() {
            return String::new();
        }
        let current = self.scroll_offset + 1;
        let total = self.content.len();
        format!("[{}/{}]", current, total)
    }
}
```

### Render with Fixed Height

```rust
pub fn render_lines(&self) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let border_style = Style::default().fg(Color::DarkGray);
    let header_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
    let thinking_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC);
    let scroll_style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM);

    // Header with line count and scroll position
    let header = format!(
        "╭── {} ({} lines) {} ",
        self.display_header(),
        self.content.len(),
        self.scroll_position()
    );
    lines.push(Line::from(Span::styled(header, header_style)));

    if self.expanded {
        // Show "more above" indicator
        if self.has_content_above() {
            lines.push(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(
                    format!("↑ {} more lines above", self.scroll_offset),
                    scroll_style
                ),
            ]));
        }

        // Render only visible content (fixed height)
        for line in self.visible_content() {
            lines.push(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(line.clone(), thinking_style),
            ]));
        }

        // Show "more below" indicator
        let lines_below = self.content.len().saturating_sub(
            self.scroll_offset + self.max_visible_lines
        );
        if lines_below > 0 {
            lines.push(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(
                    format!("↓ {} more lines below", lines_below),
                    scroll_style
                ),
            ]));
        }

        // Closing border
        lines.push(Line::from(Span::styled(
            "╰────────────────────────────────────────────────",
            border_style,
        )));
    }

    lines
}
```

### Keyboard Navigation

When a thinking block is focused (click or Tab to focus):
- `j` or `↓` - Scroll down
- `k` or `↑` - Scroll up  
- `g` - Jump to top
- `G` - Jump to bottom
- `Enter` or `Space` - Toggle expand/collapse
- `Esc` - Unfocus

### Config Option

```toml
[thinking]
enabled = true
max_visible_lines = 6    # Height of thinking block (default: 6)
auto_collapse = false    # Collapse after response complete
```

