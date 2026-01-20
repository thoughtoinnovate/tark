//! Tokenizer implementations
//!
//! Provides token counting for context management. This module starts with
//! an approximate tokenizer and can be extended with provider-specific
//! implementations (tiktoken for OpenAI, etc.) in the future.

use super::traits::Tokenizer;

/// Approximate tokenizer using character-based estimation
///
/// This provides a reasonable approximation for token counts across different
/// providers. The ratio of ~4 characters per token is based on empirical
/// observations across OpenAI, Claude, and Gemini models.
///
/// For production use with specific providers, consider implementing:
/// - `TiktokenAdapter` for OpenAI (using tiktoken-rs)
/// - `ClaudeTokenizer` for Anthropic
/// - `GeminiTokenizer` for Google
pub struct ApproximateTokenizer {
    max_context: usize,
}

impl ApproximateTokenizer {
    /// Create a new approximate tokenizer with the given max context window
    pub fn new(max_context: usize) -> Self {
        Self { max_context }
    }

    /// Create with a default 8k context window
    pub fn default_8k() -> Self {
        Self::new(8000)
    }

    /// Create with a 32k context window
    pub fn default_32k() -> Self {
        Self::new(32000)
    }

    /// Create with a 128k context window
    pub fn default_128k() -> Self {
        Self::new(128000)
    }
}

impl Tokenizer for ApproximateTokenizer {
    fn count_tokens(&self, text: &str) -> usize {
        // Approximate: 1 token ≈ 4 characters (3.5-4.5 in practice)
        // Using ceiling division to avoid underestimating
        text.len().div_ceil(4)
    }

    fn count_message_tokens(&self, role: &str, content: &str) -> usize {
        // Message structure overhead: {"role": "...", "content": "..."}
        // Approximate 20 tokens for JSON structure
        let structure_overhead = 20;
        let role_tokens = self.count_tokens(role);
        let content_tokens = self.count_tokens(content);

        structure_overhead + role_tokens + content_tokens
    }

    fn max_context_tokens(&self) -> usize {
        self.max_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approximate_tokenizer_basic() {
        let tokenizer = ApproximateTokenizer::new(8000);

        // Empty string
        assert_eq!(tokenizer.count_tokens(""), 0);

        // Short text (4 chars = 1 token)
        assert_eq!(tokenizer.count_tokens("test"), 1);

        // Longer text
        let text = "Hello, world!"; // 13 chars
        assert_eq!(tokenizer.count_tokens(text), 4); // (13+3)/4 = 4
    }

    #[test]
    fn test_message_tokens() {
        let tokenizer = ApproximateTokenizer::new(8000);

        let tokens = tokenizer.count_message_tokens("user", "Hello");
        // 20 (overhead) + 1 (role "user") + 2 (content "Hello" = 5 chars)
        assert_eq!(tokens, 23);
    }

    #[test]
    fn test_max_context() {
        let tokenizer_8k = ApproximateTokenizer::default_8k();
        assert_eq!(tokenizer_8k.max_context_tokens(), 8000);

        let tokenizer_128k = ApproximateTokenizer::default_128k();
        assert_eq!(tokenizer_128k.max_context_tokens(), 128000);
    }
}
