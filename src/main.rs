use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent;
mod completion;
mod config;
mod diagnostics;
mod llm;
mod lsp;
mod services;
mod storage;
mod tools;
mod transport;
mod tui;

#[derive(Parser)]
#[command(name = "tark")]
#[command(author, version, about = "Tark - AI-powered CLI agent with LSP server", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start LSP server (stdio - for editor integration)
    Lsp,

    /// Start HTTP server for ghost text completions and chat API
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8765")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Working directory for file operations (default: current directory)
        #[arg(long)]
        cwd: Option<String>,
    },

    /// Start both LSP and HTTP servers
    Start {
        /// Port for HTTP server
        #[arg(short, long, default_value = "8765")]
        port: u16,
    },

    /// Interactive chat mode with the AI agent (TUI)
    Chat {
        /// Initial message to send
        message: Option<String>,

        /// Working directory for file operations
        #[arg(short, long)]
        cwd: Option<String>,

        /// Unix socket path for Neovim integration
        /// When provided, connects to Neovim for editor features
        #[arg(long)]
        socket: Option<String>,

        /// LLM provider to use (openai, claude, copilot, gemini, openrouter, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use (e.g., gpt-4o, claude-sonnet-4, gemini-2.0-flash-exp)
        #[arg(short, long)]
        model: Option<String>,
    },

    /// Get a one-shot completion for a file position
    Complete {
        /// File path
        #[arg(short, long)]
        file: String,

        /// Line number (1-indexed)
        #[arg(short, long)]
        line: usize,

        /// Column number (0-indexed)
        #[arg(short, long)]
        col: usize,
    },

    /// Authenticate with LLM providers
    Auth {
        /// Provider to authenticate (copilot, openai, claude, gemini, openrouter, ollama)
        provider: Option<String>,
    },

    /// Show usage statistics and costs
    Usage {
        /// Output format (table, json)
        #[arg(short, long, default_value = "table")]
        format: String,

        /// Working directory (default: current directory)
        #[arg(long)]
        cwd: Option<String>,

        /// Cleanup logs older than N days
        #[arg(long)]
        cleanup: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        "tark_cli=debug,tower_lsp=debug"
    } else {
        "tark_cli=info,tower_lsp=warn"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    match cli.command {
        Commands::Lsp => {
            tracing::info!("Starting LSP server on stdio");
            lsp::run_lsp_server().await?;
        }
        Commands::Serve { port, host, cwd } => {
            let working_dir = cwd
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
            tracing::info!(
                "Starting HTTP server on {}:{}, cwd: {:?}",
                host,
                port,
                working_dir
            );
            transport::http::run_http_server(&host, port, working_dir).await?;
        }
        Commands::Start { port } => {
            tracing::info!("Starting LSP + HTTP servers");
            let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
            // Run HTTP server in background, LSP on stdio
            let http_handle = tokio::spawn(async move {
                if let Err(e) =
                    transport::http::run_http_server("127.0.0.1", port, working_dir).await
                {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            lsp::run_lsp_server().await?;
            http_handle.abort();
        }
        Commands::Chat {
            message,
            cwd,
            socket,
            provider,
            model,
        } => {
            let working_dir = cwd.unwrap_or_else(|| ".".to_string());

            // Determine if we're running in standalone mode or connected to Neovim
            let is_standalone = socket.is_none();

            if is_standalone {
                tracing::info!("Starting TUI chat in standalone mode, cwd: {}", working_dir);
            } else {
                tracing::info!(
                    "Starting TUI chat with Neovim integration, socket: {:?}, cwd: {}",
                    socket,
                    working_dir
                );
            }

            // Run the TUI application
            transport::cli::run_tui_chat(message, &working_dir, socket, provider, model).await?;
        }
        Commands::Complete { file, line, col } => {
            transport::cli::run_complete(&file, line, col).await?;
        }
        Commands::Auth { provider } => {
            transport::cli::run_auth(provider.as_deref()).await?;
        }
        Commands::Usage {
            format,
            cwd,
            cleanup,
        } => {
            let working_dir = cwd
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
            transport::cli::run_usage(&working_dir, &format, cleanup).await?;
        }
    }

    Ok(())
}
