//! CLI transport for direct terminal interaction

use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::config::Config;
use crate::llm;
use crate::tools::ToolRegistry;
use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

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
    let mut agent = ChatAgent::new(provider, tools).with_max_iterations(config.agent.max_iterations);

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

