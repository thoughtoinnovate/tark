//! Google Gemini LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official Google endpoints.
//! The GEMINI_API_KEY is never sent to any third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role,
    StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

/// Official Google Gemini API endpoint
const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl GeminiProvider {
    pub fn new() -> Result<Self> {
        let api_key =
            env::var("GEMINI_API_KEY").context("GEMINI_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "gemini-2.0-flash-exp".to_string(),
            max_tokens: 8192,
        })
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if let Some(text) = msg.content.as_text() {
                        system_instruction = Some(text.to_string());
                    }
                }
                Role::User => {
                    if let Some(text) = msg.content.as_text() {
                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts: vec![GeminiPart::Text {
                                text: text.to_string(),
                            }],
                        });
                    }
                }
                Role::Assistant => {
                    if let Some(text) = msg.content.as_text() {
                        contents.push(GeminiContent {
                            role: "model".to_string(),
                            parts: vec![GeminiPart::Text {
                                text: text.to_string(),
                            }],
                        });
                    }
                }
                Role::Tool => {
                    // Gemini handles tool results differently, skip for now
                    continue;
                }
            }
        }

        (system_instruction, contents)
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<GeminiFunctionDeclaration> {
        tools
            .iter()
            .map(|t| GeminiFunctionDeclaration {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    async fn send_request(&self, request: GeminiRequest) -> Result<GeminiResponse> {
        let url = format!(
            "{}/{}:generateContent?key={}",
            GEMINI_API_BASE, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error ({}): {}", status, error_text);
        }

        response
            .json::<GeminiResponse>()
            .await
            .context("Failed to parse Gemini API response")
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supports_native_thinking(&self) -> bool {
        self.model.contains("thinking")
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let response = self.send_request(request).await?;

        let usage = response.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        });

        if let Some(candidate) = response.candidates.first() {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for part in &candidate.content.parts {
                match part {
                    GeminiPart::Text { text } => {
                        text_parts.push(text.clone());
                    }
                    GeminiPart::FunctionCall { function_call } => {
                        tool_calls.push(ToolCall {
                            id: format!("gemini_{}", function_call.name), // Gemini doesn't provide IDs
                            name: function_call.name.clone(),
                            arguments: function_call.args.clone(),
                        });
                    }
                }
            }

            if tool_calls.is_empty() {
                Ok(LlmResponse::Text {
                    text: text_parts.join("\n"),
                    usage,
                })
            } else if text_parts.is_empty() {
                Ok(LlmResponse::ToolCalls {
                    calls: tool_calls,
                    usage,
                })
            } else {
                Ok(LlmResponse::Mixed {
                    text: Some(text_parts.join("\n")),
                    tool_calls,
                    usage,
                })
            }
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
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let url = format!(
            "{}/{}:streamGenerateContent?key={}&alt=sse",
            GEMINI_API_BASE, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "Gemini API error ({}): {}",
                status, error_text
            )));
            anyhow::bail!("Gemini API error ({}): {}", status, error_text);
        }

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

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
                    "Stream timeout - no response from Gemini for {} seconds",
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
                    if let Ok(chunk) = serde_json::from_str::<GeminiStreamChunk>(json_str) {
                        if let Some(candidate) = chunk.candidates.first() {
                            for part in &candidate.content.parts {
                                match part {
                                    GeminiPart::Text { text } => {
                                        if !text.is_empty() {
                                            let event = StreamEvent::TextDelta(text.clone());
                                            builder.process(&event);
                                            callback(event);
                                        }
                                    }
                                    GeminiPart::FunctionCall { function_call } => {
                                        let id = format!("gemini_{}", function_call.name);
                                        let event = StreamEvent::ToolCallStart {
                                            id: id.clone(),
                                            name: function_call.name.clone(),
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let args_str = serde_json::to_string(&function_call.args)
                                            .unwrap_or_default();
                                        let event = StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_delta: args_str,
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let event = StreamEvent::ToolCallComplete { id };
                                        callback(event);
                                    }
                                }
                            }
                        }

                        if let Some(usage) = chunk.usage_metadata {
                            builder.usage = Some(TokenUsage {
                                input_tokens: usage.prompt_token_count,
                                output_tokens: usage.candidates_token_count,
                                total_tokens: usage.total_token_count,
                            });
                        }
                    }
                }
            }
        }

        callback(StreamEvent::Done);
        Ok(builder.build())
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        let system = format!(
            "You are a code completion engine. Complete the code where <CURSOR> is placed. \
             Output ONLY the completion text. Language: {language}"
        );

        let user_content = format!("{prefix}<CURSOR>{suffix}");

        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user".to_string(),
                parts: vec![GeminiPart::Text { text: user_content }],
            }],
            system_instruction: Some(GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text: system }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(256),
                temperature: Some(0.2),
            }),
            tools: None,
        };

        let response = self.send_request(request).await?;

        let text = if let Some(candidate) = response.candidates.first() {
            candidate
                .content
                .parts
                .iter()
                .filter_map(|p| {
                    if let GeminiPart::Text { text } = p {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string()
        } else {
            String::new()
        };

        let usage = response.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
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
            // Try to extract JSON from markdown
            if let Some(json_start) = text.find('[') {
                if let Some(json_end) = text.rfind(']') {
                    let json_str = &text[json_start..=json_end];
                    if let Ok(suggestions) =
                        serde_json::from_str::<Vec<RefactoringSuggestion>>(json_str)
                    {
                        return Ok(suggestions);
                    }
                }
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
            if let Some(json_start) = text.find('[') {
                if let Some(json_end) = text.rfind(']') {
                    let json_str = &text[json_start..=json_end];
                    if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(json_str) {
                        return Ok(issues);
                    }
                }
            }
        }
        Ok(Vec::new())
    }
}

// Gemini API types

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTools>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct GeminiTools {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: u32,
    #[serde(rename = "totalTokenCount")]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiStreamChunk {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemini_response() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello, world!"}]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert!(response.usage_metadata.is_some());
        let usage = response.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 5);
        assert_eq!(usage.total_token_count, 15);
    }

    #[test]
    fn test_parse_gemini_function_call() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "test_function",
                            "args": {"key": "value"}
                        }
                    }]
                }
            }]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        let part = &response.candidates[0].content.parts[0];
        match part {
            GeminiPart::FunctionCall { function_call } => {
                assert_eq!(function_call.name, "test_function");
            }
            _ => panic!("Expected FunctionCall"),
        }
    }
}
