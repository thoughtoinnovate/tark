//! GitHub Copilot LLM provider implementation with Device Flow OAuth
//!
//! SECURITY: GitHub tokens are ONLY sent to official GitHub endpoints.
//! The access token is stored locally and never sent to third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, ContentPart, LlmProvider, LlmResponse, Message, MessageContent,
    RefactoringSuggestion, Role, StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage,
    ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Official GitHub OAuth device flow endpoint
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
/// Official GitHub OAuth token endpoint
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// GitHub Copilot token exchange endpoint
const GITHUB_COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
/// Official GitHub Copilot API endpoint
const COPILOT_API_URL: &str = "https://api.githubcopilot.com/chat/completions";
/// Official GitHub Copilot client ID (used by copilot.vim and others)
const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Token storage structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CopilotToken {
    access_token: String,
    token_type: String,
    expires_at: u64, // Unix timestamp
}

/// Guard to ensure the pending auth file is cleaned up on success or failure
struct AuthFileGuard {
    path: PathBuf,
}

impl Drop for AuthFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl CopilotToken {
    /// Extract SKU from token string (free_limited_copilot, copilot_enterprise, etc.)
    fn get_sku(&self) -> Option<String> {
        self.access_token
            .split(';')
            .find(|part| part.starts_with("sku="))
            .and_then(|part| part.strip_prefix("sku="))
            .map(|s| s.to_string())
    }

    /// Get available models based on subscription tier
    /// Tries to fetch from models.dev first, then falls back to hardcoded list based on SKU
    pub async fn available_models(&self) -> Vec<String> {
        // Try to fetch from models.dev first
        let models_db = crate::llm::models_db::models_db();
        if let Ok(models) = models_db.list_models("github").await {
            if !models.is_empty() {
                // Return model IDs from models.dev
                let model_ids: Vec<String> = models.into_iter().map(|m| m.id).collect();
                tracing::debug!("Fetched {} Copilot models from models.dev", model_ids.len());
                return model_ids;
            }
        }

        // Fallback to hardcoded list based on subscription tier
        let sku = self.get_sku().unwrap_or_default();

        match sku.as_str() {
            "free_limited_copilot" => {
                // Free tier typically has access to GPT-4o mini or limited GPT-4o
                vec!["gpt-4o".to_string()]
            }
            "copilot_enterprise" | "copilot_business" => {
                // Paid tiers have access to more models
                vec![
                    "gpt-4o".to_string(),
                    "gpt-4".to_string(),
                    "claude-3.5-sonnet".to_string(), // If enabled
                    "o1".to_string(),                // If enabled
                ]
            }
            _ => {
                // Default fallback
                vec!["gpt-4o".to_string()]
            }
        }
    }
}

/// Copilot token response from GitHub API
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: u64,
}

impl CopilotToken {
    /// Check if token is expired or will expire in the next 5 minutes
    fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Consider expired if within 5 minutes of expiry
        self.expires_at <= now + 300
    }
}

/// Device code response from GitHub
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Access token response from GitHub
#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

/// Error response from token polling
#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

pub struct CopilotProvider {
    client: reqwest::Client,
    token: Option<CopilotToken>,
    token_path: PathBuf,
    model: String,
    max_tokens: usize,
    auth_timeout_secs: u64,
}

impl CopilotProvider {
    pub fn new() -> Result<Self> {
        let token_path = Self::token_path()?;
        let token = Self::load_token(&token_path).ok();

        Ok(Self {
            client: reqwest::Client::new(),
            token,
            token_path,
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            auth_timeout_secs: 1800, // 30 minutes default
        })
    }

    /// Get available models based on user's subscription
    /// Fetches dynamically from models.dev API with fallback to hardcoded list
    pub async fn available_models(&self) -> Vec<String> {
        // If we have a token, use it to get subscription-aware models
        if let Some(ref token) = self.token {
            return token.available_models().await;
        }

        // No token - try to fetch all GitHub models from models.dev
        let models_db = crate::llm::models_db::models_db();
        if let Ok(models) = models_db.list_models("github").await {
            if !models.is_empty() {
                let model_ids: Vec<String> = models.into_iter().map(|m| m.id).collect();
                tracing::debug!(
                    "Fetched {} Copilot models from models.dev (no token)",
                    model_ids.len()
                );
                return model_ids;
            }
        }

        // Final fallback
        vec!["gpt-4o".to_string()]
    }

    pub fn with_auth_timeout(mut self, timeout_secs: u64) -> Self {
        self.auth_timeout_secs = timeout_secs;
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Get the token file path
    fn token_path() -> Result<PathBuf> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir)?;
            Ok(config_dir.join("copilot_token.json"))
        } else {
            Ok(PathBuf::from("copilot_token.json"))
        }
    }

    /// Load token from disk
    fn load_token(path: &PathBuf) -> Result<CopilotToken> {
        let content = std::fs::read_to_string(path).context("Failed to read Copilot token file")?;
        let token: CopilotToken =
            serde_json::from_str(&content).context("Failed to parse Copilot token")?;
        Ok(token)
    }

    /// Save token to disk
    fn save_token(path: &PathBuf, token: &CopilotToken) -> Result<()> {
        let content = serde_json::to_string_pretty(token)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Ensure we have a valid token, initiating Device Flow if needed
    pub async fn ensure_token(&mut self) -> Result<String> {
        // Check if we have a token and it's not expired
        if let Some(ref token) = self.token {
            if !token.is_expired() {
                return Ok(token.access_token.clone());
            }
            tracing::info!("GitHub Copilot token expired, re-authenticating...");
        }

        // Check disk for token
        let file_exists = self.token_path.exists();
        if file_exists {
            if let Ok(tok) = Self::load_token(&self.token_path) {
                if !tok.is_expired() {
                    self.token = Some(tok.clone());
                    return Ok(tok.access_token);
                }
            }
        }

        // Need to authenticate
        tracing::info!("Starting GitHub Copilot Device Flow authentication...");
        let new_token = self.device_flow_auth().await?;

        // Save token
        Self::save_token(&self.token_path, &new_token)?;
        self.token = Some(new_token.clone());

        Ok(new_token.access_token)
    }

    /// Perform Device Flow authentication
    async fn device_flow_auth(&self) -> Result<CopilotToken> {
        // Step 1: Request device code
        let device_response = self.request_device_code().await?;

        // Track auth file path so we can clean it up on success or failure
        let mut auth_file_path: Option<PathBuf> = None;

        // Step 2: Write auth info to file for TUI to pick up
        // TUI checks this file and shows auth dialog
        if let Some(home) = dirs::home_dir() {
            let tark_dir = home.join(".tark");
            let _ = std::fs::create_dir_all(&tark_dir);
            let auth_file = tark_dir.join("copilot_auth_pending.txt");
            if let Ok(mut f) = std::fs::File::create(&auth_file) {
                use std::io::Write;
                let _ = writeln!(f, "Visit: {}", device_response.verification_uri);
                let _ = writeln!(f, "Enter code: {}", device_response.user_code);
                let _ = f.flush();
            }
            auth_file_path = Some(auth_file);
        }

        // Ensure the file is cleaned up no matter what happens
        let _auth_guard = auth_file_path
            .as_ref()
            .map(|path| AuthFileGuard { path: path.clone() });

        // Display auth info to user (both for CLI and logging)
        // Format the code with dashes for readability (e.g., "ABCD-EFGH")
        let formatted_code = if device_response.user_code.len() == 8 {
            format!(
                "{}-{}",
                &device_response.user_code[..4],
                &device_response.user_code[4..]
            )
        } else {
            device_response.user_code.clone()
        };

        // Print to stdout for CLI visibility
        eprintln!();
        eprintln!("🔐 GitHub Copilot Authentication");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!();
        eprintln!("1. Visit this URL in your browser:");
        eprintln!("   {}", device_response.verification_uri);
        eprintln!();
        eprintln!("2. Enter this code:");
        eprintln!("   {}", formatted_code);
        eprintln!();
        eprintln!("Waiting for authorization...");
        eprintln!();

        tracing::info!(
            "GitHub Copilot auth: visit {} and enter code {}",
            device_response.verification_uri,
            formatted_code
        );

        // Step 3: Poll for token
        let token_response = self
            .poll_for_token(
                &device_response.device_code,
                device_response.interval,
                device_response.expires_in,
            )
            .await?;

        // Clear progress indicator and show success
        eprint!("\r"); // Clear the progress line
        eprintln!("✅ Authorization received!");
        tracing::info!("✅ Successfully authenticated with GitHub!\n");

        eprintln!("📝 Exchanging for Copilot token...");
        tracing::info!("📝 Exchanging for Copilot token...");

        // Clean up auth pending file (guard will also run on drop)
        if let Some(path) = auth_file_path.as_ref() {
            let _ = std::fs::remove_file(path);
        }

        // Step 4: Exchange OAuth token for Copilot token
        let copilot_token = self.get_copilot_token(&token_response.access_token).await?;

        eprintln!("✅ Successfully obtained Copilot token!");
        tracing::info!("✅ Successfully obtained Copilot token!\n");

        Ok(copilot_token)
    }

    /// Exchange GitHub OAuth token for Copilot token
    async fn get_copilot_token(&self, github_token: &str) -> Result<CopilotToken> {
        let response = self
            .client
            .get(GITHUB_COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", github_token))
            .header("Accept", "application/json")
            .header("User-Agent", "Tark/0.3.0")
            .header("Editor-Version", "Tark/0.3.0")
            .header("Editor-Plugin-Version", "copilot/0.3.0")
            .send()
            .await
            .context("Failed to get Copilot token")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get Copilot token ({}): {}", status, text);
        }

        let copilot_response: CopilotTokenResponse = response
            .json()
            .await
            .context("Failed to parse Copilot token response")?;

        Ok(CopilotToken {
            access_token: copilot_response.token,
            token_type: "bearer".to_string(),
            expires_at: copilot_response.expires_at,
        })
    }

    /// Request device code from GitHub
    async fn request_device_code(&self) -> Result<DeviceCodeResponse> {
        let params = [("client_id", COPILOT_CLIENT_ID), ("scope", "read:user")];

        let response = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .context("Failed to request device code from GitHub")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitHub device code request failed ({}): {}", status, text);
        }

        response
            .json::<DeviceCodeResponse>()
            .await
            .context("Failed to parse device code response")
    }

    /// Poll for access token
    async fn poll_for_token(
        &self,
        device_code: &str,
        interval: u64,
        expires_in: u64,
    ) -> Result<AccessTokenResponse> {
        let start = std::time::Instant::now();
        // Use the longer of GitHub's expires_in or our configured timeout
        let timeout_secs = expires_in.max(self.auth_timeout_secs);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let mut poll_count = 0u32;

        tracing::info!(
            "Waiting for GitHub Device Flow authorization (timeout: {}s = {} minutes)",
            timeout_secs,
            timeout_secs / 60
        );

        loop {
            // Check timeout
            if start.elapsed() > timeout {
                eprintln!();
                eprintln!(
                    "❌ Authentication timed out after {} minutes",
                    timeout_secs / 60
                );
                anyhow::bail!("Device flow authentication timed out");
            }

            // Wait before polling
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

            // Show progress every 5 polls (roughly every 15-25 seconds depending on interval)
            poll_count += 1;
            if poll_count.is_multiple_of(5) {
                let elapsed = start.elapsed().as_secs();
                eprint!("\r⏳ Still waiting... ({}s elapsed)  ", elapsed);
                use std::io::Write;
                let _ = std::io::stderr().flush();
            }

            // Poll for token
            let params = [
                ("client_id", COPILOT_CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ];

            let response = self
                .client
                .post(GITHUB_TOKEN_URL)
                .header("Accept", "application/json")
                .form(&params)
                .send()
                .await
                .context("Failed to poll for access token")?;

            let status = response.status();
            let text = response.text().await?;

            // GitHub returns 200 OK even for errors (authorization_pending, etc.)
            // So we need to check for error field first
            if let Ok(error) = serde_json::from_str::<TokenError>(&text) {
                match error.error.as_str() {
                    "authorization_pending" => {
                        // Still waiting for user to authorize
                        continue;
                    }
                    "slow_down" => {
                        // We're polling too fast, wait longer
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                    "expired_token" => {
                        anyhow::bail!("Device code expired before authorization");
                    }
                    "access_denied" => {
                        anyhow::bail!("User denied authorization");
                    }
                    _ => {
                        anyhow::bail!(
                            "GitHub OAuth error: {} - {:?}",
                            error.error,
                            error.error_description
                        );
                    }
                }
            }

            // If we get here, it's a successful response (no error field)
            if status.is_success() {
                // Parse as access token response
                let token: AccessTokenResponse =
                    serde_json::from_str(&text).context("Failed to parse access token response")?;
                return Ok(token);
            }

            anyhow::bail!(
                "Unexpected response from GitHub (status {}): {}",
                status,
                text
            );
        }
    }

    /// Convert our messages to OpenAI format (Copilot uses OpenAI-compatible API)
    fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
        messages
            .iter()
            .filter_map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                // Handle different message content types
                match &msg.content {
                    MessageContent::Text(text) => Some(CopilotMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                        tool_call_id: msg.tool_call_id.clone(),
                    }),
                    MessageContent::Parts(parts) => {
                        // Check if this is an assistant message with tool calls
                        let tool_calls: Vec<CopilotToolCall> = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolUse { id, name, input } = p {
                                    Some(CopilotToolCall {
                                        id: id.clone(),
                                        call_type: "function".to_string(),
                                        function: CopilotFunctionCall {
                                            name: name.clone(),
                                            arguments: serde_json::to_string(input)
                                                .unwrap_or_default(),
                                        },
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect();

                        if !tool_calls.is_empty() && msg.role == Role::Assistant {
                            // Assistant message with tool calls - no content
                            Some(CopilotMessage {
                                role: role.to_string(),
                                content: None,
                                tool_calls: Some(tool_calls),
                                tool_call_id: None,
                            })
                        } else {
                            // Extract text from parts
                            let text: String = parts
                                .iter()
                                .filter_map(|p| {
                                    if let ContentPart::Text { text } = p {
                                        Some(text.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("");

                            if text.is_empty() {
                                None
                            } else {
                                Some(CopilotMessage {
                                    role: role.to_string(),
                                    content: Some(text),
                                    tool_calls: None,
                                    tool_call_id: msg.tool_call_id.clone(),
                                })
                            }
                        }
                    }
                }
            })
            .collect()
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<CopilotTool> {
        tools
            .iter()
            .map(|t| CopilotTool {
                tool_type: "function".to_string(),
                function: CopilotFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    async fn send_request(&mut self, request: CopilotRequest) -> Result<CopilotResponse> {
        let token = self.ensure_token().await?;

        let response = self
            .client
            .post(COPILOT_API_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", "Tark/0.3.0")
            .header("Editor-Version", "Tark/0.3.0")
            .header("Editor-Plugin-Version", "copilot/0.3.0")
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to GitHub Copilot API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("GitHub Copilot API error ({}): {}", status, error_text);
        }

        response
            .json::<CopilotResponse>()
            .await
            .context("Failed to parse Copilot API response")
    }
}

#[async_trait]
impl LlmProvider for CopilotProvider {
    fn name(&self) -> &str {
        "copilot"
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let mut provider = Self {
            client: self.client.clone(),
            token: self.token.clone(),
            token_path: self.token_path.clone(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            auth_timeout_secs: self.auth_timeout_secs,
        };

        let copilot_messages = provider.convert_messages(messages);

        let mut request = CopilotRequest {
            model: provider.model.clone(),
            max_tokens: Some(provider.max_tokens),
            messages: copilot_messages,
            stream: None,
            tools: None,
            tool_choice: None,
        };

        // Add tools if provided
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(provider.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let response = provider.send_request(request).await?;

        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        if let Some(choice) = response.choices.first() {
            // Check if the response contains tool calls
            if let Some(ref tool_calls) = choice.message.tool_calls {
                if !tool_calls.is_empty() {
                    let calls: Vec<super::types::ToolCall> = tool_calls
                        .iter()
                        .map(|tc| super::types::ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: serde_json::from_str(&tc.function.arguments)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                        })
                        .collect();

                    return Ok(LlmResponse::ToolCalls { calls, usage });
                }
            }

            Ok(LlmResponse::Text {
                text: choice.message.content.clone().unwrap_or_default(),
                usage,
            })
        } else {
            Ok(LlmResponse::Text {
                text: String::new(),
                usage,
            })
        }
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;
        use std::collections::HashMap;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let mut provider = Self {
            client: self.client.clone(),
            token: self.token.clone(),
            token_path: self.token_path.clone(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            auth_timeout_secs: self.auth_timeout_secs,
        };

        let token = provider.ensure_token().await?;
        let copilot_messages = provider.convert_messages(messages);

        let mut request = CopilotRequest {
            model: provider.model.clone(),
            max_tokens: Some(provider.max_tokens),
            messages: copilot_messages,
            stream: Some(true),
            tools: None,
            tool_choice: None,
        };

        // Add tools if provided
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(provider.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let response = provider
            .client
            .post(COPILOT_API_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", "Tark/0.3.0")
            .header("Editor-Version", "Tark/0.3.0")
            .header("Editor-Plugin-Version", "copilot/0.3.0")
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Copilot API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "Copilot API error ({}): {}",
                status, error_text
            )));
            anyhow::bail!("Copilot API error ({}): {}", status, error_text);
        }

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // Track tool calls being built up from streaming chunks
        let mut tool_call_builders: HashMap<usize, (String, String, String)> = HashMap::new(); // index -> (id, name, arguments)

        let mut last_activity_at = std::time::Instant::now();
        loop {
            // Check for user interrupt frequently so Ctrl+C/Esc+Esc are responsive
            if let Some(check) = interrupt_check {
                if check() {
                    return Ok(builder.build());
                }
            }

            // Enforce per-chunk timeout: if we haven't received any bytes recently, abort.
            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                anyhow::bail!(
                    "Stream timeout - no response from Copilot for {} seconds",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                );
            }

            // Use a short poll interval so interrupts can be observed quickly even when
            // the server is silent, while still enforcing an overall 60s no-data timeout.
            let chunk_result = match timeout(INTERRUPT_POLL_INTERVAL, stream.next()).await {
                Ok(Some(res)) => res,
                Ok(None) => break,  // Stream ended
                Err(_) => continue, // Poll interval elapsed - re-check interrupt/timeout
            };

            last_activity_at = std::time::Instant::now();
            let chunk = chunk_result.context("Error reading stream chunk")?;
            let chunk_str = String::from_utf8_lossy(&chunk);

            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str == "[DONE]" {
                        callback(StreamEvent::Done);
                        continue;
                    }

                    if let Ok(chunk) = serde_json::from_str::<CopilotStreamChunk>(json_str) {
                        if let Some(choice) = chunk.choices.first() {
                            // Handle text content
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    let event = StreamEvent::TextDelta(content.clone());
                                    builder.process(&event);
                                    callback(event);
                                }
                            }

                            // Handle tool calls (streaming)
                            if let Some(ref tool_calls) = choice.delta.tool_calls {
                                for tc in tool_calls {
                                    let entry =
                                        tool_call_builders.entry(tc.index).or_insert_with(|| {
                                            (String::new(), String::new(), String::new())
                                        });

                                    if let Some(ref id) = tc.id {
                                        entry.0 = id.clone();
                                    }
                                    if let Some(ref func) = tc.function {
                                        if let Some(ref name) = func.name {
                                            entry.1 = name.clone();
                                        }
                                        if let Some(ref args) = func.arguments {
                                            entry.2.push_str(args);
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(usage) = chunk.usage {
                            builder.usage = Some(TokenUsage {
                                input_tokens: usage.prompt_tokens,
                                output_tokens: usage.completion_tokens,
                                total_tokens: usage.total_tokens,
                            });
                        }
                    }
                }
            }
        }

        // If we have tool calls, return them
        if !tool_call_builders.is_empty() {
            let mut calls: Vec<super::types::ToolCall> = tool_call_builders
                .into_iter()
                .map(|(_, (id, name, arguments))| super::types::ToolCall {
                    id,
                    name,
                    arguments: serde_json::from_str(&arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                })
                .collect();
            calls.sort_by(|a, b| a.id.cmp(&b.id));

            // Emit tool call events
            for call in &calls {
                callback(StreamEvent::ToolCallStart {
                    id: call.id.clone(),
                    name: call.name.clone(),
                });
                callback(StreamEvent::ToolCallDelta {
                    id: call.id.clone(),
                    arguments_delta: serde_json::to_string(&call.arguments).unwrap_or_default(),
                });
                callback(StreamEvent::ToolCallComplete {
                    id: call.id.clone(),
                });
            }

            return Ok(LlmResponse::ToolCalls {
                calls,
                usage: builder.usage,
            });
        }

        Ok(builder.build())
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        let mut provider = Self {
            client: self.client.clone(),
            token: self.token.clone(),
            token_path: self.token_path.clone(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            auth_timeout_secs: self.auth_timeout_secs,
        };

        let system = format!(
            "You are a code completion engine. Complete the code where <CURSOR> is placed. \
             Output ONLY the completion text. Language: {language}"
        );

        let user_content = format!("{prefix}<CURSOR>{suffix}");

        let request = CopilotRequest {
            model: provider.model.clone(),
            max_tokens: Some(256),
            messages: vec![
                CopilotMessage {
                    role: "system".to_string(),
                    content: Some(system),
                    tool_calls: None,
                    tool_call_id: None,
                },
                CopilotMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            stream: None,
            tools: None,
            tool_choice: None,
        };

        let response = provider.send_request(request).await?;

        let text = if let Some(choice) = response.choices.first() {
            choice
                .message
                .content
                .clone()
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            String::new()
        };

        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(CompletionResult { text, usage })
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let messages = vec![
            Message::system("You are a helpful code assistant."),
            Message::user(format!(
                "Explain this code:\n\n```\n{code}\n```\n\nContext:\n{context}"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        Ok(response
            .text()
            .unwrap_or("No explanation available.")
            .to_string())
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        let messages = vec![
            Message::system(
                r#"You are a code refactoring assistant. Return JSON array:
[{"title": "...", "description": "...", "new_code": "..."}]"#,
            ),
            Message::user(format!(
                "Suggest refactorings:\n\n```\n{code}\n```\n\nContext:\n{context}"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        if let Some(text) = response.text() {
            if let Ok(suggestions) = serde_json::from_str::<Vec<RefactoringSuggestion>>(text) {
                return Ok(suggestions);
            }
        }
        Ok(Vec::new())
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        let messages = vec![
            Message::system(
                r#"You are a code review assistant. Return JSON array:
[{"severity": "error|warning|info|hint", "message": "...", "line": 1}]"#,
            ),
            Message::user(format!(
                "Review this {language} code:\n\n```{language}\n{code}\n```"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        if let Some(text) = response.text() {
            if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(text) {
                return Ok(issues);
            }
        }
        Ok(Vec::new())
    }
}

// Copilot API types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<CopilotTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<CopilotToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CopilotToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: CopilotFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CopilotFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct CopilotTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: CopilotFunction,
}

#[derive(Debug, Serialize)]
struct CopilotFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
    usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize)]
struct CopilotChoice {
    message: CopilotMessage,
}

#[derive(Debug, Deserialize)]
struct CopilotUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct CopilotStreamChunk {
    choices: Vec<CopilotStreamChoice>,
    #[serde(default)]
    usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize)]
struct CopilotStreamChoice {
    delta: CopilotStreamDelta,
}

#[derive(Debug, Deserialize, Default)]
struct CopilotStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<CopilotStreamToolCall>>,
}

#[derive(Debug, Deserialize, Clone)]
struct CopilotStreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<CopilotStreamFunctionCall>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct CopilotStreamFunctionCall {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_expiry() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token expires in 10 minutes - should not be considered expired
        let token = CopilotToken {
            access_token: "test".to_string(),
            token_type: "bearer".to_string(),
            expires_at: now + 600,
        };
        assert!(!token.is_expired());

        // Token expires in 2 minutes - should be considered expired (within 5min buffer)
        let token = CopilotToken {
            access_token: "test".to_string(),
            token_type: "bearer".to_string(),
            expires_at: now + 120,
        };
        assert!(token.is_expired());

        // Token already expired
        let token = CopilotToken {
            access_token: "test".to_string(),
            token_type: "bearer".to_string(),
            expires_at: now - 100,
        };
        assert!(token.is_expired());
    }
}
