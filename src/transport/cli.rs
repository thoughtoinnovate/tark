//! CLI transport for direct terminal interaction

use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::config::Config;
use crate::llm;
use crate::storage::usage::{CleanupRequest, UsageTracker};
use crate::storage::TarkStorage;
use crate::tools::ToolRegistry;
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tabled::{settings::Style, Table, Tabled};

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
