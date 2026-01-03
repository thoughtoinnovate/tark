//! GitHub Copilot LLM provider implementation with Device Flow OAuth
//!
//! SECURITY: GitHub tokens are ONLY sent to official GitHub endpoints.
//! The access token is stored locally and never sent to third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role,
    StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Official GitHub OAuth device flow endpoint
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
/// Official GitHub OAuth token endpoint
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
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

        // Step 2: Display user instructions
        println!("\n╭─────────────────────────────────────────────────────╮");
        println!("│  GitHub Copilot Authentication Required            │");
        println!("╰─────────────────────────────────────────────────────╯");
        println!("\n📋 Please follow these steps:");
        println!("   1. Visit: {}", device_response.verification_uri);
        println!("   2. Enter code: {}", device_response.user_code);
        println!("   3. Authorize Tark to use GitHub Copilot");
        println!("\n⏳ Waiting for authorization...\n");

        // Step 3: Poll for token
        let token_response = self
            .poll_for_token(
                &device_response.device_code,
                device_response.interval,
                device_response.expires_in,
            )
            .await?;

        println!("✅ Successfully authenticated with GitHub Copilot!\n");

        // Calculate expiry timestamp
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            + token_response.expires_in.unwrap_or(28800); // Default 8 hours

        Ok(CopilotToken {
            access_token: token_response.access_token,
            token_type: token_response.token_type,
            expires_at,
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
        
        tracing::info!(
            "Waiting for GitHub Device Flow authorization (timeout: {}s = {} minutes)",
            timeout_secs,
            timeout_secs / 60
        );

        loop {
            // Check timeout
            if start.elapsed() > timeout {
                anyhow::bail!("Device flow authentication timed out");
            }

            // Wait before polling
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

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
    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAiMessage> {
        messages
            .iter()
            .filter_map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => return None, // Copilot doesn't support tool role directly
                };

                let content = msg.content.as_text()?.to_string();

                Some(OpenAiMessage {
                    role: role.to_string(),
                    content,
                })
            })
            .collect()
    }

    async fn send_request(&mut self, request: CopilotRequest) -> Result<CopilotResponse> {
        let token = self.ensure_token().await?;

        let response = self
            .client
            .post(COPILOT_API_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "Tark/0.1.0")
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
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        // Note: Copilot API doesn't support tools in the same way, so we ignore them
        let mut provider = Self {
            client: self.client.clone(),
            token: self.token.clone(),
            token_path: self.token_path.clone(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            auth_timeout_secs: self.auth_timeout_secs,
        };

        let copilot_messages = provider.convert_messages(messages);

        let request = CopilotRequest {
            model: provider.model.clone(),
            max_tokens: Some(provider.max_tokens),
            messages: copilot_messages,
            stream: None,
        };

        let response = provider.send_request(request).await?;

        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        if let Some(choice) = response.choices.first() {
            Ok(LlmResponse::Text {
                text: choice.message.content.clone(),
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
        _tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;

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

        let request = CopilotRequest {
            model: provider.model.clone(),
            max_tokens: Some(provider.max_tokens),
            messages: copilot_messages,
            stream: Some(true),
        };

        let response = provider
            .client
            .post(COPILOT_API_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "Tark/0.1.0")
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

        while let Some(chunk_result) = stream.next().await {
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
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    let event = StreamEvent::TextDelta(content.clone());
                                    builder.process(&event);
                                    callback(event);
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
                OpenAiMessage {
                    role: "system".to_string(),
                    content: system,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: user_content,
                },
            ],
            stream: None,
        };

        let response = provider.send_request(request).await?;

        let text = if let Some(choice) = response.choices.first() {
            choice.message.content.trim().to_string()
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
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
    usage: Option<CopilotUsage>,
}

#[derive(Debug, Deserialize)]
struct CopilotChoice {
    message: OpenAiMessage,
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
