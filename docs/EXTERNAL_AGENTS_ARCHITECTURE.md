# External Agent Integration Architecture

> **Status**: RFC / Proposal (not implemented)
>
> This document describes a potential future architecture. Module paths and file names below are illustrative and may not exist in the current codebase.

## Overview

Tark serves as a **master orchestration layer** that can integrate with external CLI agents while providing a unified interface, consistent UX, and powerful orchestration capabilities.

## Architecture Layers

### Layer 1: Provider Adapters (Direct Integration)

Wrap external CLI agents as tark LLM providers for full feature parity.

```
┌─────────────────────────────────────────┐
│           Tark Core                     │
│  (Modes, Tools, Sessions, Queue)        │
├─────────────────────────────────────────┤
│        LlmProvider Trait                │
├──────────┬──────────┬──────────┬────────┤
│ OpenAI   │ Claude   │ Ollama   │ Gemini │
│ Native   │ Native   │ Native   │ CLI    │
└──────────┴──────────┴──────────┴────────┘
                                   ↓
                            ┌─────────────┐
                            │ gemini-cli  │
                            └─────────────┘
```

**When to use:**
- Agent has good CLI interface
- Want full tark features (modes, tools, tracking)
- Agent is conversation-based

**Implementation:**

```rust
// src/llm/adapters/mod.rs
pub trait CliAdapter {
    fn command_path(&self) -> &Path;
    fn convert_messages(&self, messages: &[Message]) -> Result<String>;
    fn parse_response(&self, output: String) -> Result<LlmResponse>;
}

// src/llm/adapters/gemini_cli.rs
pub struct GeminiCliAdapter {
    cli_path: PathBuf,
    model: String,
}

impl LlmProvider for GeminiCliAdapter {
    async fn chat(&mut self, messages: Vec<Message>) -> Result<LlmResponse> {
        let prompt = self.convert_messages(&messages)?;
        
        let output = tokio::process::Command::new(&self.cli_path)
            .arg("chat")
            .arg("--model").arg(&self.model)
            .arg("--prompt").arg(&prompt)
            .output()
            .await?;
        
        self.parse_response(String::from_utf8(output.stdout)?)
    }
}
```

### Layer 2: Specialized Tools (Capability Extension)

Expose unique agent capabilities as tools that any provider can invoke.

```
┌─────────────────────────────────────────┐
│      Tark Agent (Any Provider)          │
├─────────────────────────────────────────┤
│          Tool Registry                  │
├────────┬────────┬────────┬──────────────┤
│ read   │ write  │ grep   │ copilot_     │
│ _file  │ _file  │        │ suggest      │
└────────┴────────┴────────┴──────────────┘
                              ↓
                     ┌────────────────────┐
                     │ gh copilot suggest │
                     └────────────────────┘
```

**When to use:**
- Agent has a specific superpower
- Want to use across all providers
- Single-shot operation (not conversational)

**Implementation:**

```rust
// src/tools/external/copilot_suggest.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotSuggestTool;

#[async_trait]
impl Tool for CopilotSuggestTool {
    fn name(&self) -> &str {
        "copilot_suggest"
    }

    fn description(&self) -> &str {
        "Get GitHub Copilot code suggestions for a given context"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "cursor_position": {
                    "type": "number",
                    "description": "Cursor position in the file"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let file_path = args["file_path"].as_str()
            .ok_or_else(|| anyhow!("file_path required"))?;
        
        // Read file content
        let content = tokio::fs::read_to_string(file_path).await?;
        
        // Call GitHub Copilot CLI
        let output = tokio::process::Command::new("gh")
            .args(&["copilot", "suggest", "-t", "shell"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?
            .wait_with_output()
            .await?;
        
        let suggestions = String::from_utf8(output.stdout)?;
        
        Ok(ToolResult::success(suggestions))
    }
}
```

### Layer 3: Agent Orchestration (Meta-Agent)

Tark coordinates multiple agents for complex workflows.

```
┌────────────────────────────────────────────┐
│         Tark Orchestrator                  │
│  ┌──────────────────────────────────────┐  │
│  │  Agent Router                        │  │
│  │  - Classify request type             │  │
│  │  - Route to best agent               │  │
│  └──────────────────────────────────────┘  │
│  ┌──────────────────────────────────────┐  │
│  │  Multi-Agent Coordinator             │  │
│  │  - Parallel execution                │  │
│  │  - Consensus building                │  │
│  └──────────────────────────────────────┘  │
│  ┌──────────────────────────────────────┐  │
│  │  Pipeline Builder                    │  │
│  │  - Sequential workflows              │  │
│  │  - Agent chaining                    │  │
│  └──────────────────────────────────────┘  │
└────────────────────────────────────────────┘
         ↓           ↓           ↓
    ┌────────┐  ┌────────┐  ┌────────┐
    │ Claude │  │Copilot │  │   Q    │
    │  Plan  │  │  Code  │  │Security│
    └────────┘  └────────┘  └────────┘
```

**When to use:**
- Complex multi-step workflows
- Need best-in-class for each task
- Aggregate multiple perspectives

**Implementation:**

```rust
// src/orchestration/agent_router.rs
pub struct AgentRouter {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    routing_rules: Vec<RoutingRule>,
}

pub struct RoutingRule {
    pub pattern: Regex,
    pub provider: String,
    pub confidence: f64,
}

impl AgentRouter {
    pub async fn route(&self, message: &str) -> Result<&dyn LlmProvider> {
        // Classify request
        if message.contains("security") || message.contains("vulnerability") {
            return Ok(self.providers.get("amazonq").unwrap().as_ref());
        }
        
        if message.contains("code completion") || message.contains("autocomplete") {
            return Ok(self.providers.get("copilot").unwrap().as_ref());
        }
        
        if message.contains("plan") || message.contains("design") {
            return Ok(self.providers.get("claude").unwrap().as_ref());
        }
        
        // Default to configured provider
        Ok(self.providers.get("default").unwrap().as_ref())
    }
}

// src/orchestration/multi_agent.rs
pub struct MultiAgentCoordinator {
    agents: Vec<Box<dyn LlmProvider>>,
}

impl MultiAgentCoordinator {
    pub async fn consensus(&mut self, prompt: &str) -> Result<String> {
        // Get responses from all agents in parallel
        let responses = futures::future::join_all(
            self.agents.iter_mut().map(|agent| {
                agent.chat(vec![Message::user(prompt)])
            })
        ).await;
        
        // Aggregate responses
        let combined = responses.into_iter()
            .filter_map(|r| r.ok())
            .map(|r| r.text)
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        
        // Use primary agent to synthesize
        let synthesis_prompt = format!(
            "Multiple AI assistants provided these responses:\n\n{}\n\n\
             Synthesize these into a single coherent answer:",
            combined
        );
        
        let final_response = self.agents[0]
            .chat(vec![Message::user(&synthesis_prompt)])
            .await?;
        
        Ok(final_response.text)
    }
}

// src/orchestration/pipeline.rs
pub struct AgentPipeline {
    stages: Vec<PipelineStage>,
}

pub struct PipelineStage {
    pub name: String,
    pub agent: Box<dyn LlmProvider>,
    pub prompt_template: String,
}

impl AgentPipeline {
    pub async fn execute(&mut self, initial_input: &str) -> Result<String> {
        let mut context = initial_input.to_string();
        
        for stage in &mut self.stages {
            let prompt = stage.prompt_template.replace("{input}", &context);
            let response = stage.agent.chat(vec![Message::user(&prompt)]).await?;
            context = response.text;
            
            tracing::info!("Stage '{}' completed", stage.name);
        }
        
        Ok(context)
    }
}
```

## Configuration

```toml
# .tark/config.toml

[external_agents]
enabled = true

[external_agents.gemini_cli]
type = "provider"
path = "/usr/local/bin/gemini-cli"
model = "gemini-1.5-flash"
enabled = true

[external_agents.github_copilot]
type = "tool"
path = "/usr/local/bin/gh"
enabled = true

[external_agents.amazon_q]
type = "provider"
path = "/usr/local/bin/q"
enabled = true

[orchestration]
enabled = true
default_provider = "claude"

[orchestration.routing]
security = "amazon_q"
code_completion = "github_copilot"
planning = "claude"
implementation = "copilot"
```

## User Experience

### Direct Usage
```bash
# Use external agent as provider
tark chat --provider gemini-cli "Explain this code"

# Use external tool
tark chat "Use copilot_suggest to complete this function"
```

### Neovim Integration
```lua
-- Switch to external provider
:TarkProvider gemini-cli

-- Orchestration mode
:TarkOrchestration on

-- Use specialized tool
/copilot_suggest path/to/file.ts
```

### Orchestration Workflows

**Example 1: Security-First Development**
```lua
local pipeline = {
    {stage = "security_scan", provider = "amazon_q"},
    {stage = "implementation", provider = "copilot"},
    {stage = "review", provider = "claude"},
}
```

**Example 2: Consensus Decision**
```lua
-- Get answer from multiple agents
:TarkConsensus "What's the best way to implement auth?"
-- Queries: Claude, GPT-4, Gemini
-- Returns: Synthesized answer
```

## Implementation Phases

### Phase 1: Foundation (Weeks 1-2)
- [ ] Create adapter trait and base infrastructure
- [ ] Implement first adapter (Gemini CLI)
- [ ] Add external tool support
- [ ] Configuration system

### Phase 2: Core Adapters (Weeks 3-4)
- [ ] GitHub Copilot CLI adapter
- [ ] Amazon Q adapter
- [ ] OpenCode adapter
- [ ] Test coverage for all adapters

### Phase 3: Orchestration (Weeks 5-6)
- [ ] Agent router
- [ ] Multi-agent coordinator
- [ ] Pipeline builder
- [ ] Routing configuration

### Phase 4: Polish (Week 7)
- [ ] Neovim integration
- [ ] Documentation
- [ ] Examples and tutorials
- [ ] Performance optimization

## Benefits

1. **Unified Interface**: One tool, many agents
2. **Best-in-Class**: Use the right agent for each task
3. **Feature Consistency**: All agents get tark's modes, tools, sessions
4. **Flexibility**: Mix and match as needed
5. **Future-Proof**: Easy to add new agents

## Trade-offs

1. **Complexity**: More moving parts
2. **Performance**: Adapter overhead
3. **Maintenance**: Keep adapters updated
4. **Feature Parity**: Some agent features may not map perfectly

## Security Considerations

1. **Credential Management**: Each agent may need separate auth
2. **Command Injection**: Sanitize all inputs to external CLIs
3. **Rate Limiting**: Respect each service's limits
4. **Data Privacy**: Be mindful of what's sent where

## Testing Strategy

1. **Unit Tests**: Each adapter in isolation
2. **Integration Tests**: End-to-end with mock CLIs
3. **Contract Tests**: Ensure CLI compatibility
4. **Performance Tests**: Measure adapter overhead

