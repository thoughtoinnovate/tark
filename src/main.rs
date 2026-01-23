use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Import from library crate
use tark_cli::*;

// Re-export debug logging utilities for use within the binary
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tark_cli::debug_logger::{DebugLogEntry, DebugLogger, DebugLoggerConfig};

/// Global debug logger instance
static TARK_DEBUG_LOGGER: OnceLock<DebugLogger> = OnceLock::new();

/// Fast path check for whether debug logging is enabled.
/// This atomic bool avoids the overhead of checking OnceLock on every log call.
static DEBUG_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Initialize the global debug logger
pub fn init_debug_logger(config: DebugLoggerConfig) -> anyhow::Result<()> {
    let logger = DebugLogger::new(config)?;
    TARK_DEBUG_LOGGER
        .set(logger)
        .map_err(|_| anyhow::anyhow!("Debug logger already initialized"))?;
    // Set the fast-path flag
    DEBUG_LOGGING_ENABLED.store(true, Ordering::Release);
    Ok(())
}

/// Fast check if debug logging is enabled (zero-cost when disabled)
///
/// Use this for early-bail in hot paths before constructing log entries.
#[inline(always)]
pub fn is_debug_logging_enabled() -> bool {
    DEBUG_LOGGING_ENABLED.load(Ordering::Relaxed)
}

/// Get the global debug logger (if initialized)
pub fn debug_logger() -> Option<&'static DebugLogger> {
    TARK_DEBUG_LOGGER.get()
}

/// Log a debug entry to the global logger (if enabled)
pub fn debug_log(entry: DebugLogEntry) {
    if let Some(logger) = debug_logger() {
        logger.log(entry);
    }
}

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

        /// Enable debug logging to tark-debug.log
        #[arg(long)]
        debug: bool,
    },

    /// NEW: Interactive TUI mode (TDD implementation)
    Tui {
        /// Working directory for file operations
        #[arg(short, long)]
        cwd: Option<String>,

        /// LLM provider to use
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,

        /// Enable debug logging
        #[arg(long)]
        debug: bool,
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
        #[command(subcommand)]
        command: Option<AuthCommands>,

        /// Provider to authenticate (copilot, openai, claude, gemini, openrouter, ollama)
        #[arg(conflicts_with = "command")]
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

    /// Plugin management commands
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },
}

/// Auth subcommands
#[derive(Subcommand)]
enum AuthCommands {
    /// Logout from a provider (clear stored tokens)
    Logout {
        /// Provider to logout from (copilot, gemini)
        provider: String,
    },

    /// Show authentication status for all providers
    Status,
}

/// Plugin subcommands
#[derive(Subcommand)]
enum PluginCommands {
    /// List installed plugins
    List,

    /// Show plugin details
    Info {
        /// Plugin ID
        plugin_id: String,
    },

    /// Install a plugin from a git repository
    Add {
        /// Git repository URL or local path
        url: String,

        /// Branch or tag (default: main)
        #[arg(short, long, default_value = "main")]
        branch: String,

        /// Subdirectory path within the repository (for monorepos)
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Uninstall a plugin
    Remove {
        /// Plugin ID
        plugin_id: String,
    },

    /// Enable a disabled plugin
    Enable {
        /// Plugin ID
        plugin_id: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin ID
        plugin_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    // For TUI/Chat modes, default to quieter logging to avoid cluttering the interface
    // For other modes (LSP, Serve), use info level for debugging
    let is_tui_mode = matches!(cli.command, Commands::Tui { .. } | Commands::Chat { .. });

    let filter = if cli.verbose {
        "tark_cli=debug,tower_lsp=debug"
    } else if is_tui_mode {
        // TUI mode: only show warnings and errors by default
        "tark_cli=warn,tower_lsp=warn"
    } else {
        // Non-TUI modes (LSP, Serve, etc.): keep info level
        "tark_cli=info,tower_lsp=warn"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // Initialize models.dev cache with a persistent global cache directory.
    // This enables fast, dynamic model pickers and pricing/limits lookups across runs.
    if let Ok(global) = storage::GlobalStorage::new() {
        let cache_dir = global.root().join("cache");
        if std::fs::create_dir_all(&cache_dir).is_ok() {
            llm::init_models_db(cache_dir);
            llm::models_db().preload();
        }
    }

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
            debug,
        } => {
            let working_dir = cwd.unwrap_or_else(|| ".".to_string());

            // Determine if we're running in standalone mode or connected to Neovim
            let is_standalone = socket.is_none();

            if is_standalone {
                tracing::debug!("Starting TUI chat in standalone mode, cwd: {}", working_dir);
            } else {
                tracing::debug!(
                    "Starting TUI chat with Neovim integration, socket: {:?}, cwd: {}",
                    socket,
                    working_dir
                );
            }

            // Run the TUI application
            transport::cli::run_tui_chat(message, &working_dir, socket, provider, model, debug)
                .await?;
        }
        Commands::Complete { file, line, col } => {
            transport::cli::run_complete(&file, line, col).await?;
        }
        Commands::Auth { command, provider } => match command {
            Some(AuthCommands::Logout { provider }) => {
                transport::cli::run_auth_logout(&provider).await?;
            }
            Some(AuthCommands::Status) => {
                transport::cli::run_auth_status().await?;
            }
            None => {
                transport::cli::run_auth(provider.as_deref()).await?;
            }
        },
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
        Commands::Plugin { command } => match command {
            PluginCommands::List => {
                transport::plugin_cli::run_plugin_list().await?;
            }
            PluginCommands::Info { plugin_id } => {
                transport::plugin_cli::run_plugin_info(&plugin_id).await?;
            }
            PluginCommands::Add { url, branch, path } => {
                transport::plugin_cli::run_plugin_add(&url, &branch, path.as_deref()).await?;
            }
            PluginCommands::Remove { plugin_id } => {
                transport::plugin_cli::run_plugin_remove(&plugin_id).await?;
            }
            PluginCommands::Enable { plugin_id } => {
                transport::plugin_cli::run_plugin_enable(&plugin_id).await?;
            }
            PluginCommands::Disable { plugin_id } => {
                transport::plugin_cli::run_plugin_disable(&plugin_id).await?;
            }
        },
        Commands::Tui {
            cwd,
            provider,
            model,
            debug,
        } => {
            let working_dir = cwd.unwrap_or_else(|| ".".to_string());
            tracing::debug!(
                "Starting NEW TUI (TDD implementation), cwd: {}",
                working_dir
            );
            transport::cli::run_tui_new(&working_dir, provider, model, debug).await?;
        }
    }

    Ok(())
}
