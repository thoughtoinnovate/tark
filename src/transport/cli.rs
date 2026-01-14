//! CLI transport for direct terminal interaction

// Allow dead code for intentionally unused API methods that are part of the public interface
#![allow(dead_code)]

use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::config::Config;
use crate::llm;
use crate::storage::usage::{CleanupRequest, UsageTracker};
use crate::storage::TarkStorage;
use crate::tools::ToolRegistry;
use crate::tui::{EditorBridge, EditorBridgeConfig, TuiApp, TuiConfig};
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tabled::{settings::Style, Table, Tabled};

/// Run TUI chat mode
///
/// This starts the full Terminal UI for chat, optionally connecting to Neovim
/// via Unix socket for editor integration.
///
/// # Arguments
/// * `initial_message` - Optional initial message to send
/// * `working_dir` - Working directory for file operations
/// * `socket_path` - Optional Unix socket path for Neovim integration
pub async fn run_tui_chat(
    _initial_message: Option<String>,
    working_dir: &str,
    socket_path: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    debug: bool,
) -> Result<()> {
    let working_dir = PathBuf::from(working_dir).canonicalize()?;

    // Load TUI configuration
    let tui_config = TuiConfig::load().unwrap_or_default();

    // Create the TUI application with provider/model overrides from CLI
    // This ensures the correct provider is used when creating the AgentBridge
    let mut app =
        TuiApp::with_provider_override(tui_config, provider.clone(), model.clone(), debug)?;

    // Determine effective provider for logging
    let effective_provider = provider.unwrap_or_else(|| {
        let config = Config::load().unwrap_or_default();
        config.llm.default_provider.clone()
    });
    tracing::info!("Using provider: {}", effective_provider);

    // Determine standalone mode
    let is_standalone = socket_path.is_none();
    app.state.editor_connected = !is_standalone;

    // If socket path provided, attempt to connect to Neovim
    if let Some(ref socket) = socket_path {
        let bridge_config = EditorBridgeConfig {
            socket_path: PathBuf::from(socket),
            ..Default::default()
        };
        let bridge = EditorBridge::new(bridge_config);

        // Try to connect (non-blocking, will fall back to standalone if fails)
        match bridge.connect().await {
            Ok(()) => {
                tracing::info!("Connected to Neovim at {}", socket);
                app.state.editor_connected = true;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to connect to Neovim: {}. Running in standalone mode.",
                    e
                );
                app.state.editor_connected = false;
            }
        }
    }

    // Update status message based on mode
    if app.state.editor_connected {
        app.state.status_message = Some(format!(
            "Connected to Neovim | Working dir: {}",
            working_dir.display()
        ));
    } else {
        app.state.status_message = Some(format!(
            "Standalone mode | Working dir: {}",
            working_dir.display()
        ));
    }

    // Run the TUI event loop
    app.run().await?;

    Ok(())
}

/// Check if LLM is properly configured for a specific provider
/// Returns Ok(()) if configured, Err(message) with helpful guidance if not
fn check_llm_configuration_for_provider(provider: &str) -> Result<(), String> {
    match provider.to_lowercase().as_str() {
        "openai" | "gpt" => {
            if std::env::var("OPENAI_API_KEY").is_err() {
                return Err("OpenAI API key not configured.\n\n\
                    To use OpenAI, set the OPENAI_API_KEY environment variable:\n\
                    \n\
                    export OPENAI_API_KEY=\"your-api-key-here\"\n\
                    \n\
                    Get your API key from: https://platform.openai.com/api-keys"
                    .to_string());
            }
        }
        "claude" | "anthropic" => {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                return Err("Anthropic API key not configured.\n\n\
                    To use Claude, set the ANTHROPIC_API_KEY environment variable:\n\
                    \n\
                    export ANTHROPIC_API_KEY=\"your-api-key-here\"\n\
                    \n\
                    Get your API key from: https://console.anthropic.com/settings/keys"
                    .to_string());
            }
        }
        "copilot" | "github" => {
            // Check if Copilot token exists
            let token_path = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tark")
                .join("copilot_token.json");
            if !token_path.exists() {
                return Err("GitHub Copilot not authenticated.\n\n\
                    Run 'tark auth copilot' to authenticate with GitHub Copilot."
                    .to_string());
            }
        }
        "gemini" | "google" => {
            // Check for API key first
            if std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok() {
                return Ok(());
            }
            // Check for OAuth token
            let token_path = dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tark")
                .join("tokens")
                .join("gemini.json");
            if token_path.exists() {
                return Ok(());
            }
            // Check for ADC
            if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
                return Ok(());
            }
            return Err("Google Gemini not configured.\n\n\
                Option 1 - OAuth (recommended for personal use):\n\
                  tark auth gemini\n\
                \n\
                Option 2 - API Key:\n\
                  export GEMINI_API_KEY=\"your-api-key-here\"\n\
                  Get your API key from: https://aistudio.google.com/apikey\n\
                \n\
                Option 3 - Application Default Credentials (Google Cloud):\n\
                  gcloud auth application-default login"
                .to_string());
        }
        "openrouter" => {
            if std::env::var("OPENROUTER_API_KEY").is_err() {
                return Err("OpenRouter API key not configured.\n\n\
                    To use OpenRouter, set the OPENROUTER_API_KEY environment variable:\n\
                    \n\
                    export OPENROUTER_API_KEY=\"your-api-key-here\"\n\
                    \n\
                    Get your API key from: https://openrouter.ai/keys"
                    .to_string());
            }
        }
        "ollama" | "local" => {
            // Ollama doesn't require an API key, but we could check if it's running
            // For now, assume it's configured if selected
        }
        _ => {
            return Err(format!(
                "Unknown LLM provider: '{}'\n\n\
                Supported providers:\n\
                - openai (requires OPENAI_API_KEY)\n\
                - claude (requires ANTHROPIC_API_KEY)\n\
                - copilot (requires 'tark auth copilot')\n\
                - gemini (requires GOOGLE_API_KEY)\n\
                - openrouter (requires OPENROUTER_API_KEY)\n\
                - ollama (local, no API key needed)\n\
                \n\
                Configure in ~/.config/tark/config.toml or set default_provider",
                provider
            ));
        }
    }

    Ok(())
}

/// Check if LLM is properly configured using config's default provider
/// Returns Ok(()) if configured, Err(message) with helpful guidance if not
fn check_llm_configuration(config: &Config) -> Result<(), String> {
    check_llm_configuration_for_provider(&config.llm.default_provider)
}

/// Run interactive chat mode
pub async fn run_chat(initial_message: Option<String>, working_dir: &str) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let working_dir = PathBuf::from(working_dir).canonicalize()?;

    println!("tark chat mode");
    println!("Working directory: {}", working_dir.display());
    println!("Type 'exit' or 'quit' to exit, 'clear' to clear history\n");

    // Create LLM provider
    let provider = llm::create_provider(&config.llm.default_provider)?;
    let provider = Arc::from(provider);

    // Create tool registry
    let tools = ToolRegistry::with_defaults(working_dir, config.tools.shell_enabled);

    // Create agent
    let mut agent =
        ChatAgent::new(provider, tools).with_max_iterations(config.agent.max_iterations);

    // Handle initial message if provided
    if let Some(msg) = initial_message {
        println!("> {}\n", msg);
        match agent.chat(&msg).await {
            Ok(response) => {
                println!("{}\n", response.text);
                if response.tool_calls_made > 0 {
                    println!("(Used {} tool calls)\n", response.tool_calls_made);
                }
            }
            Err(e) => {
                eprintln!("Error: {}\n", e);
            }
        }
    }

    // Interactive loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        match input.to_lowercase().as_str() {
            "exit" | "quit" => {
                println!("Goodbye!");
                break;
            }
            "clear" => {
                agent.reset();
                println!("Conversation cleared.\n");
                continue;
            }
            _ => {}
        }

        match agent.chat(input).await {
            Ok(response) => {
                println!("\n{}\n", response.text);
                if response.tool_calls_made > 0 {
                    println!("(Used {} tool calls)\n", response.tool_calls_made);
                }
            }
            Err(e) => {
                eprintln!("Error: {}\n", e);
            }
        }
    }

    Ok(())
}

/// Run one-shot completion
pub async fn run_complete(file: &str, line: usize, col: usize) -> Result<()> {
    let config = Config::load().unwrap_or_default();

    // Read file content
    let file_path = PathBuf::from(file).canonicalize()?;
    let content = std::fs::read_to_string(&file_path)?;

    // Create LLM provider
    let provider = llm::create_provider(&config.llm.default_provider)?;
    let provider = Arc::from(provider);

    // Create completion engine
    let engine = CompletionEngine::new(provider)
        .with_cache_size(config.completion.cache_size)
        .with_context_lines(
            config.completion.context_lines_before,
            config.completion.context_lines_after,
        );

    // Build request
    let request = CompletionRequest {
        file_path: file_path.clone(),
        file_content: content,
        cursor_line: line.saturating_sub(1), // Convert to 0-indexed
        cursor_col: col,
        related_files: vec![],
        lsp_context: None, // CLI mode doesn't have LSP context
    };

    // Get completion
    match engine.complete(&request).await {
        Ok(response) => {
            println!("{}", response.completion);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Display usage statistics
pub async fn run_usage(
    working_dir: &PathBuf,
    format: &str,
    cleanup_days: Option<u32>,
) -> Result<()> {
    // Initialize storage to get .tark directory
    let storage = TarkStorage::new(working_dir)
        .context("Failed to initialize storage. Make sure you're in a tark workspace.")?;

    let tracker =
        UsageTracker::new(storage.project_root()).context("Failed to initialize usage tracker")?;

    // Handle cleanup if requested
    if let Some(days) = cleanup_days {
        println!(
            "{}",
            format!("Cleaning up logs older than {} days...", days).yellow()
        );
        let result = tracker
            .cleanup(CleanupRequest {
                older_than_days: Some(days),
                session_ids: None,
                delete_all: None,
            })
            .await?;

        println!(
            "{} {} logs, {} sessions, freed {}",
            "✓".green(),
            result.deleted_logs,
            result.deleted_sessions,
            result.freed_human
        );
        println!("New database size: {}", result.new_db_size_human);
        return Ok(());
    }

    // Get summary statistics
    let summary = tracker.get_summary()?;
    let models = tracker.get_usage_by_model()?;
    let modes = tracker.get_usage_by_mode()?;

    match format {
        "json" => {
            // JSON output
            let output = serde_json::json!({
                "summary": summary,
                "by_model": models,
                "by_mode": modes,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            // Table output
            println!("\n{}", "=== TARK USAGE SUMMARY ===".bold().cyan());
            println!();

            // Summary section
            #[derive(Tabled)]
            struct SummaryRow {
                #[tabled(rename = "Metric")]
                metric: String,
                #[tabled(rename = "Value")]
                value: String,
            }

            let summary_data = vec![
                SummaryRow {
                    metric: "Total Cost".to_string(),
                    value: format!("${:.4}", summary.total_cost),
                },
                SummaryRow {
                    metric: "Total Tokens".to_string(),
                    value: format!("{}", summary.total_tokens),
                },
                SummaryRow {
                    metric: "Sessions".to_string(),
                    value: format!("{}", summary.session_count),
                },
                SummaryRow {
                    metric: "Requests".to_string(),
                    value: format!("{}", summary.log_count),
                },
                SummaryRow {
                    metric: "Database Size".to_string(),
                    value: summary.db_size_human.clone(),
                },
            ];

            let mut table = Table::new(summary_data);
            table.with(Style::rounded());
            println!("{}", table);
            println!();

            // Models section
            if !models.is_empty() {
                println!("{}", "=== USAGE BY MODEL ===".bold().cyan());
                println!();

                #[derive(Tabled)]
                struct ModelRow {
                    #[tabled(rename = "Provider")]
                    provider: String,
                    #[tabled(rename = "Model")]
                    model: String,
                    #[tabled(rename = "Requests")]
                    requests: u64,
                    #[tabled(rename = "Input Tokens")]
                    input_tokens: String,
                    #[tabled(rename = "Output Tokens")]
                    output_tokens: String,
                    #[tabled(rename = "Cost")]
                    cost: String,
                }

                let model_data: Vec<ModelRow> = models
                    .iter()
                    .map(|m| ModelRow {
                        provider: m.provider.clone(),
                        model: m.model.clone(),
                        requests: m.request_count,
                        input_tokens: format_number(m.input_tokens),
                        output_tokens: format_number(m.output_tokens),
                        cost: format!("${:.4}", m.cost),
                    })
                    .collect();

                let mut table = Table::new(model_data);
                table.with(Style::rounded());
                println!("{}", table);
                println!();
            }

            // Modes section
            if !modes.is_empty() {
                println!("{}", "=== USAGE BY MODE ===".bold().cyan());
                println!();

                #[derive(Tabled)]
                struct ModeRow {
                    #[tabled(rename = "Type")]
                    request_type: String,
                    #[tabled(rename = "Mode")]
                    mode: String,
                    #[tabled(rename = "Requests")]
                    requests: u64,
                    #[tabled(rename = "Tokens")]
                    tokens: String,
                    #[tabled(rename = "Cost")]
                    cost: String,
                }

                let mode_data: Vec<ModeRow> = modes
                    .iter()
                    .map(|m| ModeRow {
                        request_type: m.request_type.clone(),
                        mode: m.mode.clone(),
                        requests: m.request_count,
                        tokens: format_number(m.tokens),
                        cost: format!("${:.4}", m.cost),
                    })
                    .collect();

                let mut table = Table::new(mode_data);
                table.with(Style::rounded());
                println!("{}", table);
                println!();
            }

            // Help text
            println!("{}", "Tip:".bold());
            println!("  • View JSON: tark usage --format json");
            println!("  • Cleanup old logs: tark usage --cleanup 30");
        }
    }

    Ok(())
}

/// Run authentication for LLM providers
pub async fn run_auth(provider: Option<&str>) -> Result<()> {
    use colored::Colorize;
    use std::io::{self, Write};

    println!("{}", "=== Tark Authentication ===".bold().cyan());
    println!();

    let provider = if let Some(p) = provider {
        p.to_string()
    } else {
        // Interactive provider selection
        println!("Select a provider to authenticate:");
        println!();
        println!("  1. {} - GitHub Copilot (Device Flow)", "copilot".green());
        println!("  2. {} - OpenAI GPT models", "openai".green());
        println!("  3. {} - Anthropic Claude", "claude".green());
        println!("  4. {} - Google Gemini", "gemini".green());
        println!("  5. {} - OpenRouter (200+ models)", "openrouter".green());
        println!("  6. {} - Local Ollama", "ollama".green());
        println!();
        print!("Enter choice (1-6): ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => "copilot".to_string(),
            "2" => "openai".to_string(),
            "3" => "claude".to_string(),
            "4" => "gemini".to_string(),
            "5" => "openrouter".to_string(),
            "6" => "ollama".to_string(),
            _ => anyhow::bail!("Invalid choice"),
        }
    };

    println!();
    println!("{} {}", "Authenticating with:".bold(), provider.green());
    println!();

    match provider.as_str() {
        "copilot" | "github" => {
            use crate::llm::CopilotProvider;

            println!("Checking for existing token...");

            // Create provider and trigger authentication
            let mut provider = CopilotProvider::new()?;

            println!("Initiating authentication flow...");
            println!();

            // Call ensure_token to trigger Device Flow if needed
            // This will display the URL and code to the user if authentication is required
            let token = provider.ensure_token().await?;

            println!();
            println!("✅ Successfully authenticated with GitHub Copilot!");
            println!("Token saved to: ~/.config/tark/copilot_token.json");
            println!("Token preview: {}...", &token[..token.len().min(20)]);
            println!();
            println!("You can now use GitHub Copilot as your provider:");
            println!("  tark chat --provider copilot");
            println!("  Or within TUI: /model → Select 'GitHub Copilot'");
        }
        "openai" | "gpt" => {
            if std::env::var("OPENAI_API_KEY").is_ok() {
                println!("✅ OPENAI_API_KEY is already set");
            } else {
                println!("{}", "OpenAI API Key Required".bold());
                println!();
                println!("Please set your API key:");
                println!("  export OPENAI_API_KEY=\"your-api-key-here\"");
                println!();
                println!("Get your API key at: https://platform.openai.com/api-keys");
            }
        }
        "claude" | "anthropic" => {
            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                println!("✅ ANTHROPIC_API_KEY is already set");
            } else {
                println!("{}", "Anthropic API Key Required".bold());
                println!();
                println!("Please set your API key:");
                println!("  export ANTHROPIC_API_KEY=\"your-api-key-here\"");
                println!();
                println!("Get your API key at: https://console.anthropic.com/settings/keys");
            }
        }
        "gemini" | "google" => {
            // Check for API key
            if std::env::var("GEMINI_API_KEY").is_ok() {
                println!("✅ GEMINI_API_KEY is already set");
                println!();
                println!("You can use Gemini with your API key:");
                println!("  tark chat --provider gemini");
            } else if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
                println!("✅ Using Application Default Credentials");
                println!();
                println!("GOOGLE_APPLICATION_CREDENTIALS is set");
            } else {
                // Check if gemini-oauth plugin has credentials
                let oauth_creds_path = dirs::home_dir()
                    .map(|h| h.join(".gemini").join("oauth_creds.json"))
                    .unwrap_or_default();

                if oauth_creds_path.exists() {
                    println!("✅ Gemini CLI OAuth credentials found");
                    println!();
                    println!("Use the gemini-oauth plugin provider:");
                    println!("  tark chat --provider gemini-oauth");
                    println!();
                    println!("Or set GEMINI_API_KEY for direct API access.");
                } else {
                    println!("{}", "Gemini Authentication Options".bold());
                    println!();
                    println!("Option 1 - API Key (recommended for simplicity):");
                    println!("  export GEMINI_API_KEY=\"your-api-key\"");
                    println!("  Get key: https://aistudio.google.com/apikey");
                    println!();
                    println!("Option 2 - OAuth via Gemini CLI (for Cloud Code Assist):");
                    println!("  npm install -g @google/gemini-cli");
                    println!("  gemini auth login");
                    println!("  tark chat --provider gemini-oauth");
                }
            }
        }
        "openrouter" => {
            if std::env::var("OPENROUTER_API_KEY").is_ok() {
                println!("✅ OPENROUTER_API_KEY is already set");
            } else {
                println!("{}", "OpenRouter API Key Required".bold());
                println!();
                println!("Please set your API key:");
                println!("  export OPENROUTER_API_KEY=\"your-api-key-here\"");
                println!();
                println!("Get your API key at: https://openrouter.ai/keys");
                println!();
                println!(
                    "💡 Tip: OpenRouter provides access to 200+ models, many with free tiers!"
                );
            }
        }
        "ollama" | "local" => {
            println!("{}", "Ollama Local Setup".bold());
            println!();
            println!("Ollama runs models locally on your machine.");
            println!();
            println!("Setup steps:");
            println!("  1. Install Ollama: https://ollama.ai/download");
            println!("  2. Start Ollama: ollama serve");
            println!("  3. Pull a model: ollama pull codellama");
            println!();
            println!("No API key needed!");
        }
        _ => {
            anyhow::bail!("Unknown provider: {}", provider);
        }
    }

    Ok(())
}

/// Format large numbers with K/M suffix
fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Logout from an LLM provider (clear stored tokens)
pub async fn run_auth_logout(provider: &str) -> Result<()> {
    use colored::Colorize;

    println!("{}", "=== Tark Logout ===".bold().cyan());
    println!();

    match provider.to_lowercase().as_str() {
        "gemini" | "google" => {
            // Clear plugin storage if exists
            let plugin_storage = dirs::data_local_dir()
                .map(|d| {
                    d.join("tark")
                        .join("plugins")
                        .join("gemini-oauth")
                        .join("data")
                        .join("storage.json")
                })
                .unwrap_or_default();

            if plugin_storage.exists() {
                std::fs::remove_file(&plugin_storage)?;
                println!("✅ Cleared gemini-oauth plugin credentials");
            }

            // Clear native token if exists
            let token_path = dirs::data_local_dir()
                .map(|d| d.join("tark").join("tokens").join("gemini.json"))
                .unwrap_or_default();

            if token_path.exists() {
                std::fs::remove_file(&token_path)?;
                println!("✅ Cleared Gemini OAuth token");
            }

            println!();
            println!("To re-authenticate with Gemini CLI OAuth:");
            println!("  gemini auth login");
            println!();
            println!("Or set an API key:");
            println!("  export GEMINI_API_KEY=\"your-key\"");
        }
        "copilot" | "github" => {
            // Remove Copilot token
            if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
                let token_path = proj_dirs.config_dir().join("copilot_token.json");
                if token_path.exists() {
                    std::fs::remove_file(&token_path)?;
                    println!("✅ Logged out from GitHub Copilot");
                } else {
                    println!("No Copilot token found");
                }
            }
        }
        _ => {
            println!(
                "Provider '{}' does not use stored authentication.",
                provider
            );
            println!();
            println!("To change API keys, update your environment variables.");
        }
    }

    Ok(())
}

/// Check authentication status for all providers
pub async fn run_auth_status() -> Result<()> {
    use colored::Colorize;

    println!("{}", "=== Tark Authentication Status ===".bold().cyan());
    println!();

    // Check Gemini
    {
        let has_api_key = std::env::var("GEMINI_API_KEY").is_ok();
        let has_adc = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok();
        let has_oauth = dirs::home_dir()
            .map(|h| h.join(".gemini").join("oauth_creds.json").exists())
            .unwrap_or(false);

        let status_str = if has_api_key {
            "✅ API Key".green()
        } else if has_adc {
            "✅ ADC".green()
        } else if has_oauth {
            "✅ OAuth (via Gemini CLI)".green()
        } else {
            "❌ Not authenticated".red()
        };
        println!("  Gemini:     {}", status_str);
    }

    // Check Copilot
    {
        let token_exists = directories::ProjectDirs::from("", "", "tark")
            .map(|p| p.config_dir().join("copilot_token.json").exists())
            .unwrap_or(false);
        if token_exists {
            println!("  Copilot:    {}", "✅ Authenticated".green());
        } else {
            println!("  Copilot:    {}", "❌ Not authenticated".red());
        }
    }

    // Check OpenAI
    if std::env::var("OPENAI_API_KEY").is_ok() {
        println!("  OpenAI:     {}", "✅ API Key set".green());
    } else {
        println!("  OpenAI:     {}", "❌ OPENAI_API_KEY not set".red());
    }

    // Check Claude
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        println!("  Claude:     {}", "✅ API Key set".green());
    } else {
        println!("  Claude:     {}", "❌ ANTHROPIC_API_KEY not set".red());
    }

    // Check OpenRouter
    if std::env::var("OPENROUTER_API_KEY").is_ok() {
        println!("  OpenRouter: {}", "✅ API Key set".green());
    } else {
        println!("  OpenRouter: {}", "❌ OPENROUTER_API_KEY not set".red());
    }

    println!();
    println!("Authenticate with: tark auth <provider>");
    println!("Logout with: tark auth logout <provider>");

    Ok(())
}
