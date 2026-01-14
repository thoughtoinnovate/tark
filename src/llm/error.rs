//! Typed errors for LLM operations
//!
//! Provides structured error types to enable intelligent handling of common
//! failure modes (auth expired, rate limiting, etc.) without string matching.

use thiserror::Error;

/// LLM operation errors with typed variants
///
/// Enables callers to distinguish between different failure modes:
/// - `Unauthorized` (401) - token expired/invalid; can retry after refresh
/// - `RateLimited` (429) - quota exceeded; can retry after delay
/// - `BadRequest` (400) - malformed request; caller error
/// - `ServiceError` (5xx) - server-side issue; can retry
/// - `Network` - connection/timeout; can retry
/// - `Other` - catch-all for unhandled errors
#[derive(Debug, Error)]
pub enum LlmError {
    /// Authentication token is expired or invalid (HTTP 401)
    ///
    /// For providers with refreshable auth (e.g., OAuth), the caller
    /// can refresh credentials and retry once.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Rate limit exceeded (HTTP 429)
    ///
    /// The caller should implement exponential backoff retry.
    /// The inner string may contain quota reset time if available.
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Malformed request (HTTP 400)
    ///
    /// This indicates a bug in the provider implementation or
    /// the caller passed invalid parameters. Should not retry.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Server-side error (HTTP 5xx)
    ///
    /// Transient server issues. Can retry with backoff.
    #[error("Service error: {0}")]
    ServiceError(String),

    /// Network connectivity issue (connection refused, timeout, etc.)
    ///
    /// Can retry with backoff.
    #[error("Network error: {0}")]
    Network(String),

    /// Other errors not fitting the above categories
    ///
    /// Wraps `anyhow::Error` for compatibility with existing code.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl LlmError {
    /// Check if this error is retryable (after a delay or auth refresh)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::Unauthorized(_)
                | LlmError::RateLimited(_)
                | LlmError::ServiceError(_)
                | LlmError::Network(_)
        )
    }

    /// Check if this error indicates an auth issue that can be fixed by refreshing credentials
    pub fn needs_auth_refresh(&self) -> bool {
        matches!(self, LlmError::Unauthorized(_))
    }

    /// Check if this error indicates a rate limit that requires waiting
    pub fn needs_rate_limit_wait(&self) -> bool {
        matches!(self, LlmError::RateLimited(_))
    }

    /// Convert HTTP status code and error text into typed LlmError
    pub fn from_http_status(status: reqwest::StatusCode, error_text: String) -> Self {
        match status.as_u16() {
            401 => LlmError::Unauthorized(error_text),
            429 => LlmError::RateLimited(error_text),
            400 => LlmError::BadRequest(error_text),
            500..=599 => LlmError::ServiceError(error_text),
            _ => LlmError::Other(anyhow::anyhow!("HTTP {}: {}", status, error_text)),
        }
    }

    /// Convert network/connection errors into typed LlmError
    pub fn from_network_error(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            LlmError::Network(format!("Request timeout: {}", e))
        } else if e.is_connect() {
            LlmError::Network(format!("Connection failed: {}", e))
        } else if let Some(status) = e.status() {
            // HTTP error with status code
            let error_text = e.to_string();
            Self::from_http_status(status, error_text)
        } else {
            LlmError::Other(e.into())
        }
    }
}

// Note: We don't implement From<LlmError> for anyhow::Error because
// thiserror::Error already provides Into<anyhow::Error> via the standard
// Error trait. Use .into() or ? to convert LlmError to anyhow::Error.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unauthorized_is_retryable() {
        let err = LlmError::Unauthorized("token expired".to_string());
        assert!(err.is_retryable());
        assert!(err.needs_auth_refresh());
        assert!(!err.needs_rate_limit_wait());
    }

    #[test]
    fn test_rate_limited_is_retryable() {
        let err = LlmError::RateLimited("quota exceeded".to_string());
        assert!(err.is_retryable());
        assert!(!err.needs_auth_refresh());
        assert!(err.needs_rate_limit_wait());
    }

    #[test]
    fn test_bad_request_not_retryable() {
        let err = LlmError::BadRequest("invalid parameter".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_from_http_status() {
        let err = LlmError::from_http_status(
            reqwest::StatusCode::UNAUTHORIZED,
            "Invalid token".to_string(),
        );
        assert!(matches!(err, LlmError::Unauthorized(_)));

        let err = LlmError::from_http_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded".to_string(),
        );
        assert!(matches!(err, LlmError::RateLimited(_)));

        let err =
            LlmError::from_http_status(reqwest::StatusCode::BAD_REQUEST, "Bad request".to_string());
        assert!(matches!(err, LlmError::BadRequest(_)));

        let err = LlmError::from_http_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "Server error".to_string(),
        );
        assert!(matches!(err, LlmError::ServiceError(_)));
    }

    #[test]
    fn test_error_display() {
        let err = LlmError::Unauthorized("token expired".to_string());
        assert_eq!(err.to_string(), "Unauthorized: token expired");

        let err = LlmError::RateLimited("quota exceeded".to_string());
        assert_eq!(err.to_string(), "Rate limited: quota exceeded");
    }

    #[test]
    fn test_convert_to_anyhow() {
        let llm_err = LlmError::Unauthorized("test".to_string());
        let anyhow_err: anyhow::Error = llm_err.into();
        assert!(anyhow_err.to_string().contains("Unauthorized"));
    }
}
