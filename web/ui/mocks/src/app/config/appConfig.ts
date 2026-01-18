/**
 * Application Configuration
 * 
 * This file contains configurable settings for the TUI agent application.
 * Change these values to customize the agent's identity and branding.
 * 
 * @ratatui-state: These should be stored in a Config struct
 * @ratatui-pattern:
 * ```rust
 * pub struct AppConfig {
 *     pub agent_name: String,
 *     pub agent_name_short: String,
 *     pub version: String,
 *     pub default_path: String,
 *     pub user_name: String,
 *     pub user_icon: String,
 * }
 * 
 * impl Default for AppConfig {
 *     fn default() -> Self {
 *         Self {
 *             agent_name: "Innodrupe Terminal".to_string(),
 *             agent_name_short: "Innodrupe".to_string(),
 *             version: "2.1.0".to_string(),
 *             default_path: "~/projects".to_string(),
 *             // Detect username from environment
 *             user_name: std::env::var("USER")
 *                 .or_else(|_| std::env::var("USERNAME"))
 *                 .unwrap_or_else(|_| "You".to_string()),
 *             user_icon: "👤".to_string(),
 *         }
 *     }
 * }
 * ```
 */

export interface AppConfig {
  /** Full name displayed in terminal header (e.g., "Innodrupe Terminal") */
  agentName: string;
  
  /** Short name used in messages and labels (e.g., "Innodrupe") */
  agentNameShort: string;
  
  /** Version string (e.g., "2.1.0") */
  version: string;
  
  /** Default working directory path shown in header */
  defaultPath: string;
  
  /** Icon/emoji shown next to agent name in header */
  headerIcon: string;
  
  /** Icon/emoji shown next to agent messages */
  agentIcon: string;
  
  /** 
   * User's display name shown in user messages
   * In Ratatui: Auto-detected from $USER or $USERNAME environment variable
   * Falls back to "You" if not detected
   */
  userName: string;
  
  /** Icon/emoji shown next to user messages */
  userIcon: string;
}

/**
 * Default application configuration
 * 
 * CUSTOMIZE THIS to change the agent's identity:
 * - agentName: The full name shown in the terminal header
 * - agentNameShort: The short name shown in agent message labels
 * - version: Your agent's version number
 * - defaultPath: The path shown in the terminal header
 * - userName: The user's display name (auto-detected in Ratatui from $USER/$USERNAME)
 * 
 * @ratatui-note: In Rust implementation, userName should be auto-detected:
 * ```rust
 * let user_name = std::env::var("USER")
 *     .or_else(|_| std::env::var("USERNAME"))
 *     .unwrap_or_else(|_| "You".to_string());
 * ```
 */
export const defaultAppConfig: AppConfig = {
  agentName: "Innodrupe Terminal",
  agentNameShort: "Innodrupe",
  version: "2.1.0",
  defaultPath: "~/innodrupe/core/engine",
  headerIcon: "🖥",
  agentIcon: "🤖",
  // In Ratatui: Auto-detect from $USER or $USERNAME env var
  // For demo, we use "You" - agent will replace with detected username
  userName: "You",
  userIcon: "👤",
};

/**
 * Example alternative configurations
 * Uncomment and modify to use a different identity
 */

// export const defaultAppConfig: AppConfig = {
//   agentName: "CodePilot Terminal",
//   agentNameShort: "CodePilot",
//   version: "1.0.0",
//   defaultPath: "~/workspace",
//   headerIcon: "🚀",
//   agentIcon: "🤖",
//   userName: "You",  // Will be auto-detected in Ratatui
//   userIcon: "👤",
// };

// export const defaultAppConfig: AppConfig = {
//   agentName: "DevAssist Terminal",
//   agentNameShort: "DevAssist",
//   version: "3.0.0",
//   defaultPath: "~/dev",
//   headerIcon: "⚡",
//   agentIcon: "🧙",
//   userName: "You",  // Will be auto-detected in Ratatui
//   userIcon: "👨‍💻",
// };
