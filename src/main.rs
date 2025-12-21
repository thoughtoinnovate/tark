use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent;
mod completion;
mod config;
mod diagnostics;
mod llm;
mod lsp;
mod storage;
mod tools;
mod transport;

#[derive(Parser)]
#[command(name = "tark")]
#[command(author, version, about = "Tark (तर्क) - AI-powered CLI agent with LSP server", long_about = None)]
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
    },

    /// Start both LSP and HTTP servers
    Start {
        /// Port for HTTP server
        #[arg(short, long, default_value = "8765")]
        port: u16,
    },

    /// Interactive chat mode with the AI agent
    Chat {
        /// Initial message to send
        message: Option<String>,

        /// Working directory for file operations
        #[arg(short, long)]
        cwd: Option<String>,
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
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    match cli.command {
        Commands::Lsp => {
            tracing::info!("Starting LSP server on stdio");
            lsp::run_lsp_server().await?;
        }
        Commands::Serve { port, host } => {
            tracing::info!("Starting HTTP server on {}:{}", host, port);
            transport::http::run_http_server(&host, port).await?;
        }
        Commands::Start { port } => {
            tracing::info!("Starting LSP + HTTP servers");
            // Run HTTP server in background, LSP on stdio
            let http_handle = tokio::spawn(async move {
                if let Err(e) = transport::http::run_http_server("127.0.0.1", port).await {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            lsp::run_lsp_server().await?;
            http_handle.abort();
        }
        Commands::Chat { message, cwd } => {
            let working_dir = cwd.unwrap_or_else(|| ".".to_string());
            tracing::info!("Starting chat mode in {}", working_dir);
            transport::cli::run_chat(message, &working_dir).await?;
        }
        Commands::Complete { file, line, col } => {
            transport::cli::run_complete(&file, line, col).await?;
        }
    }

    Ok(())
}
