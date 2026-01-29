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

fn maybe_init_debug_logger(working_dir: &std::path::Path) -> anyhow::Result<()> {
    if tark_cli::is_debug_logging_enabled() {
        return Ok(());
    }
    let debug_config = tark_cli::DebugLoggerConfig {
        log_dir: working_dir.join(".tark").join("debug"),
        max_file_size: 10 * 1024 * 1024,
        max_rotated_files: 3,
    };
    tark_cli::init_debug_logger(debug_config)?;
    Ok(())
}

#[derive(Parser)]
#[command(name = "tark")]
#[command(author, version, about = "Tark - AI-powered CLI agent with LSP server", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Enable debug logging (writes to .tark/debug)
    #[arg(long, global = true)]
    debug: bool,

    /// Enable remote channel mode for a plugin (e.g., discord). Can be used as a flag with --plugin.
    #[arg(long, global = true, num_args(0..=1), value_name = "PLUGIN", default_missing_value = "__flag__")]
    remote: Option<String>,

    /// Remote plugin ID (alias for --remote <plugin>)
    #[arg(long, global = true, value_name = "PLUGIN")]
    plugin: Option<String>,

    /// Start in a specific agent mode for remote sessions
    #[arg(long, global = true, value_name = "MODE")]
    mode: Option<RemoteModeArg>,

    /// Start with a specific trust level for remote sessions (build mode only)
    #[arg(long, global = true, value_name = "TRUST")]
    trust: Option<RemoteTrustArg>,

    /// Remote interface: tui (main), cli (headless), dash (remote monitor)
    #[arg(long, global = true, value_name = "INTERFACE")]
    interface: Option<RemoteInterface>,

    /// Enable full remote debug logging (otherwise error-only)
    #[arg(long, global = true)]
    remote_debug: bool,

    /// LLM provider to use (openai, claude, copilot, gemini, openrouter, ollama)
    #[arg(long, global = true)]
    provider: Option<String>,

    /// Model to use (e.g., gpt-4o, claude-sonnet-4, gemini-2.0-flash-exp)
    #[arg(long, global = true)]
    model: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum RemoteInterface {
    Tui,
    Cli,
    Dash,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum RemoteModeArg {
    Ask,
    Plan,
    Build,
}

impl RemoteModeArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Plan => "plan",
            Self::Build => "build",
        }
    }
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum RemoteTrustArg {
    Manual,
    Balanced,
    Careful,
}

impl RemoteTrustArg {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Balanced => "balanced",
            Self::Careful => "careful",
        }
    }
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

    /// NEW: Interactive TUI mode (TDD implementation)
    Tui {
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

    /// Show remote sessions (by id or 'all')
    Show {
        /// Session ID or 'all'
        target: String,
    },

    /// Stop a remote session (or 'all')
    Stop {
        /// Session ID or 'all'
        target: String,
    },

    /// Resume a remote session (or 'all')
    Resume {
        /// Session ID or 'all'
        target: String,
    },

    /// Plugin management commands
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Policy database management
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
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

/// Policy subcommands
#[derive(Subcommand)]
enum PolicyCommands {
    /// Verify policy database integrity
    Verify {
        /// Force reseed builtin policy from embedded configs
        #[arg(long)]
        fix: bool,

        /// Working directory (default: current directory)
        #[arg(long)]
        cwd: Option<String>,
    },
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

    /// Update a plugin from its recorded source
    Update {
        /// Plugin ID (omit with --all)
        plugin_id: Option<String>,

        /// Update all plugins with recorded sources
        #[arg(long)]
        all: bool,
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

    /// Run OAuth authentication for a plugin
    Auth {
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
    let is_tui_mode = matches!(cli.command, Some(Commands::Tui { .. }))
        || (cli.command.is_none()
            && cli.remote.is_some()
            && cli.interface.as_ref() == Some(&RemoteInterface::Tui));

    let filter = if cli.verbose {
        "tark_cli=debug,tower_lsp=debug"
    } else if is_tui_mode {
        // TUI mode: suppress ALL stderr logging to avoid corrupting the display
        // Use --verbose (-v) to enable debug logging, or --debug for file-based logging
        "off"
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

    let remote_flag = cli.remote.as_deref() == Some("__flag__");
    let mut remote_plugin = cli
        .remote
        .as_deref()
        .filter(|val| *val != "__flag__")
        .map(|val| val.to_string());
    if let Some(plugin) = cli.plugin.as_ref() {
        if let Some(existing) = remote_plugin.as_ref() {
            if existing != plugin {
                anyhow::bail!("--remote {} and --plugin {} conflict", existing, plugin);
            }
        }
        remote_plugin = Some(plugin.clone());
    }
    if remote_flag && remote_plugin.is_none() {
        anyhow::bail!("--remote requires --plugin <id> (or use --remote <plugin>)");
    }

    if let Some(remote_plugin) = remote_plugin.as_deref() {
        let interface = cli
            .interface
            .clone()
            .ok_or_else(|| anyhow::anyhow!("--remote requires --interface <tui|cli|dash>"))?;

        if let Some(mode) = cli.mode.as_ref() {
            std::env::set_var("TARK_REMOTE_MODE_OVERRIDE", mode.as_str());
        }
        if let Some(trust) = cli.trust.as_ref() {
            std::env::set_var("TARK_REMOTE_TRUST_OVERRIDE", trust.as_str());
        }
        let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
        if cli.debug {
            maybe_init_debug_logger(&working_dir)?;
        }
        if matches!(interface, RemoteInterface::Cli) {
            transport::remote::run_remote_headless(
                working_dir,
                remote_plugin,
                cli.remote_debug,
                cli.provider.clone(),
                cli.model.clone(),
            )
            .await?;
        } else if matches!(interface, RemoteInterface::Tui) {
            std::env::set_var("TARK_FORCE_QUIT_ON_CTRL_C", "1");
            std::env::set_var("TARK_REMOTE_ENABLED", "1");
            std::env::set_var("TARK_REMOTE_PLUGIN", remote_plugin);
            let (server_handle, _project_root) = transport::remote::start_remote_runtime(
                working_dir.clone(),
                remote_plugin,
                cli.remote_debug,
                cli.provider.clone(),
                cli.model.clone(),
            )
            .await?;
            let result = transport::cli::run_tui_new(
                &working_dir.to_string_lossy(),
                cli.provider.clone(),
                cli.model.clone(),
                cli.debug,
            )
            .await;
            server_handle.abort();
            result?;
        } else {
            transport::remote::run_remote_tui(
                working_dir,
                remote_plugin,
                cli.remote_debug,
                cli.provider.clone(),
                cli.model.clone(),
            )
            .await?;
        }
        return Ok(());
    }

    if cli.command.is_none() {
        anyhow::bail!("No command specified. Use --help for options.");
    }

    let global_provider = cli.provider.clone();
    let global_model = cli.model.clone();
    let command = cli.command;

    match command.unwrap() {
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
            transport::http::run_http_server(&host, port, working_dir, None, None, None, None)
                .await?;
        }
        Commands::Start { port } => {
            tracing::info!("Starting LSP + HTTP servers");
            let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
            // Run HTTP server in background, LSP on stdio
            let http_handle = tokio::spawn(async move {
                if let Err(e) = transport::http::run_http_server(
                    "127.0.0.1",
                    port,
                    working_dir,
                    None,
                    None,
                    None,
                    None,
                )
                .await
                {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            lsp::run_lsp_server().await?;
            http_handle.abort();
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
        Commands::Show { target } => {
            transport::cli::run_remote_show(&target).await?;
        }
        Commands::Stop { target } => {
            transport::cli::run_remote_stop(&target).await?;
        }
        Commands::Resume { target } => {
            transport::cli::run_remote_resume(&target).await?;
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
            PluginCommands::Update { plugin_id, all } => {
                if all {
                    transport::plugin_cli::run_plugin_update_all().await?;
                } else if let Some(plugin_id) = plugin_id {
                    transport::plugin_cli::run_plugin_update(&plugin_id).await?;
                } else {
                    anyhow::bail!("Specify a plugin ID or use --all");
                }
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
            PluginCommands::Auth { plugin_id } => {
                transport::plugin_cli::run_plugin_auth(&plugin_id).await?;
            }
        },
        Commands::Policy { command } => match command {
            PolicyCommands::Verify { fix, cwd } => {
                let working_dir = cwd
                    .as_ref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
                transport::cli::run_policy_verify(&working_dir, fix).await?;
            }
        },
        Commands::Tui { cwd } => {
            let working_dir = cwd.unwrap_or_else(|| ".".to_string());
            if cli.debug {
                maybe_init_debug_logger(&std::path::PathBuf::from(&working_dir))?;
            }
            tracing::debug!(
                "Starting NEW TUI (TDD implementation), cwd: {}",
                working_dir
            );
            transport::cli::run_tui_new(
                &working_dir,
                global_provider.clone(),
                global_model.clone(),
                cli.debug,
            )
            .await?;
        }
    }

    Ok(())
}
