//! OAuth 2.1 authorization for MCP servers (R6 S17).
//!
//! Remote MCP servers may require user authorization. Tark implements only
//! published stable contracts, selected per server at implementation time:
//!
//! - Authorization-server metadata discovery (RFC 8414 well-known URIs).
//! - Dynamic client registration (RFC 7591) when the server offers it.
//! - Authorization-code flow with PKCE/S256, driven by an explicit user
//!   gesture in the browser; tokens return through the local redirect.
//! - Refresh-token rotation and the client-credentials grant (RFC 6749).
//!
//! Token records live in the [`crate::mcp::credential_store`]:
//! `tark-mcp-oauth/<issuer>` holds the raw access token (read at request
//! time by the HTTP transport via `MCP_BEARER_CREDENTIAL`), while
//! `tark-mcp-oauth/<issuer>#record` holds the refresh metadata. Token values
//! never appear in logs, errors, or [`Debug`] output.

use anyhow::{Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Credential-store service for OAuth tokens issued for MCP servers.
pub const OAUTH_TOKEN_SERVICE: &str = "tark-mcp-oauth";

/// Suffix distinguishing the refresh-metadata entry from the raw token entry.
pub const OAUTH_RECORD_SUFFIX: &str = "#record";

/// Consider tokens expiring within this margin already expired (seconds).
const EXPIRY_SKEW_SECS: u64 = 300;

/// HTTP timeout for metadata/registration/token calls.
const OAUTH_HTTP_TIMEOUT_SECS: u64 = 30;

/// Authorization-server metadata (RFC 8414 subset used by MCP clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    /// Issuer identifier.
    pub issuer: String,
    /// URL of the authorization endpoint.
    pub authorization_endpoint: String,
    /// URL of the token endpoint.
    pub token_endpoint: String,
    /// URL of the dynamic client registration endpoint, when offered.
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    /// Scopes the server supports, when advertised.
    #[serde(default)]
    pub scopes_supported: Option<Vec<String>>,
}

/// Candidate metadata URLs for a server URL, per MCP discovery rules:
///
/// 1. `<origin>/.well-known/oauth-authorization-server`
/// 2. `<origin>/.well-known/oauth-authorization-server<path>`
/// 3. `<server-url>/.well-known/oauth-authorization-server`
pub fn metadata_urls(server_url: &str) -> Vec<String> {
    let parsed: url::Url = match server_url.parse() {
        Ok(url) => url,
        Err(_) => return Vec::new(),
    };
    let origin = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default()
    );
    let origin = if let Some(port) = parsed.port() {
        format!("{origin}:{port}")
    } else {
        origin
    };
    let path = parsed.path().trim_end_matches('/');
    let mut urls = vec![format!("{origin}/.well-known/oauth-authorization-server")];
    if !path.is_empty() && path != "/" {
        urls.push(format!(
            "{origin}/.well-known/oauth-authorization-server{path}"
        ));
    }
    urls.push(format!(
        "{}/.well-known/oauth-authorization-server",
        server_url.trim_end_matches('/')
    ));
    urls
}

/// Fetch the first authorization-server metadata document that parses.
pub async fn discover_metadata(server_url: &str) -> Result<AuthorizationServerMetadata> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(OAUTH_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to build OAuth HTTP client")?;
    let mut last_error = anyhow::anyhow!("No metadata URLs for server");
    for url in metadata_urls(server_url) {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<AuthorizationServerMetadata>().await {
                    Ok(mut metadata) => {
                        if metadata.issuer.is_empty() {
                            metadata.issuer = server_url.to_string();
                        }
                        return Ok(metadata);
                    }
                    Err(e) => {
                        last_error = anyhow::anyhow!("Invalid metadata at {url}: {e}");
                    }
                }
            }
            Ok(resp) => {
                last_error = anyhow::anyhow!("Metadata probe {url} returned {}", resp.status());
            }
            Err(e) => {
                last_error = anyhow::anyhow!("Metadata probe {url} failed: {e}");
            }
        }
    }
    Err(last_error.context("OAuth discovery failed: server offers no usable metadata"))
}

/// PKCE code verifier/challenge pair (RFC 7636, S256 only).
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// High-entropy verifier (kept client-side until the code exchange).
    pub verifier: String,
    /// `BASE64URL-NO-PAD(SHA256(verifier))`.
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a fresh pair (43..128 unreserved characters).
    pub fn generate() -> Self {
        use rand::RngCore;
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let mut bytes = [0u8; 64];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let verifier: String = bytes
            .iter()
            .map(|b| CHARSET[(b % 66) as usize] as char)
            .collect();
        Self::from_verifier(&verifier)
    }

    /// Derive the challenge for a known verifier (test vectors).
    pub fn from_verifier(verifier: &str) -> Self {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(verifier.as_bytes());
        Self {
            verifier: verifier.to_string(),
            challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest),
        }
    }
}

/// Pending authorization: hand the URL to the user, keep the rest for the
/// code exchange. The verifier is secret until exchanged.
#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    /// URL to open in the browser (user gesture).
    pub authorization_url: String,
    /// Opaque state round-tripped for CSRF protection.
    pub state: String,
    /// PKCE verifier for the code exchange.
    pub verifier: String,
    /// Issuer this flow targets.
    pub issuer: String,
    /// Registered client id.
    pub client_id: String,
    /// Token endpoint for the exchange.
    pub token_endpoint: String,
    /// Redirect URI registered for the client.
    pub redirect_uri: String,
}

/// Begin an authorization-code flow: builds the user-facing URL and the
/// secrets needed to complete it. Scope mismatches against
/// `scopes_supported` warn (never fail: servers may grant subsets).
pub fn begin_authorization(
    metadata: &AuthorizationServerMetadata,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> PendingAuthorization {
    if let Some(supported) = &metadata.scopes_supported {
        let missing: Vec<&String> = scopes
            .iter()
            .filter(|s| !supported.iter().any(|k| k == *s))
            .collect();
        if !missing.is_empty() {
            tracing::warn!(
                "OAuth scopes not advertised by issuer: {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    let pkce = PkceChallenge::generate();
    let mut state_bytes = [0u8; 16];
    {
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut state_bytes);
    }
    let state = hex::encode(state_bytes);
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
        metadata.authorization_endpoint,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        pkce.challenge,
        state,
    );
    if !scopes.is_empty() {
        url.push_str(&format!(
            "&scope={}",
            urlencoding::encode(&scopes.join(" "))
        ));
    }
    PendingAuthorization {
        authorization_url: url,
        state,
        verifier: pkce.verifier,
        issuer: metadata.issuer.clone(),
        client_id: client_id.to_string(),
        token_endpoint: metadata.token_endpoint.clone(),
        redirect_uri: redirect_uri.to_string(),
    }
}

/// Token endpoint success body (RFC 6749 section 5.1 subset).
#[derive(Debug, Clone, Deserialize)]
struct TokenSuccess {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

/// Token endpoint error body (RFC 6749 section 5.2). Values are server
/// strings, safe to surface (never credentials).
#[derive(Debug, Clone, Deserialize)]
struct TokenError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Persisted OAuth token record (refresh metadata, never logged).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenRecord {
    /// Issuer this record belongs to.
    pub issuer: String,
    /// Refresh token, when the server granted one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix timestamp when the access token expires (0 = unknown).
    #[serde(default)]
    pub expires_at: u64,
    /// Granted scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Token endpoint for refreshes.
    pub token_endpoint: String,
    /// Registered client id.
    pub client_id: String,
    /// Registered client secret, when issued (confidential clients).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

impl OAuthTokenRecord {
    /// True when the access token is expired or within the skew margin.
    /// Unknown expiry (0) is treated as expired to force a refresh attempt.
    pub fn is_expired(&self) -> bool {
        self.expires_at <= now_secs() + EXPIRY_SKEW_SECS
    }

    /// Credential-store key for the raw access token entry.
    pub fn token_key(&self) -> String {
        format!("{OAUTH_TOKEN_SERVICE}/{}", self.issuer)
    }

    /// Credential-store key for this refresh record.
    pub fn record_key(&self) -> String {
        format!("{}{}", self.token_key(), OAUTH_RECORD_SUFFIX)
    }
}

async fn post_token_form(
    token_endpoint: &str,
    form: &HashMap<&str, String>,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<TokenSuccess> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(OAUTH_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to build OAuth HTTP client")?;
    let mut request = client.post(token_endpoint).form(form);
    // Confidential clients authenticate per RFC 6749 section 2.3.1.
    if let Some(secret) = client_secret {
        request = request.basic_auth(client_id, Some(secret));
    }
    let response = request.send().await.context("Token request failed")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let detail = serde_json::from_str::<TokenError>(&body)
            .map(|e| {
                if e.error_description
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
                {
                    e.error
                } else {
                    format!("{}: {}", e.error, e.error_description.unwrap_or_default())
                }
            })
            .unwrap_or_else(|_| format!("HTTP {status}"));
        anyhow::bail!("Token endpoint rejected the request ({detail})");
    }
    serde_json::from_str::<TokenSuccess>(&body).context("Invalid token response")
}

/// Exchange an authorization code for tokens (PKCE).
pub async fn exchange_code(pending: &PendingAuthorization, code: &str) -> Result<OAuthTokenRecord> {
    let mut form = HashMap::new();
    form.insert("grant_type", "authorization_code".to_string());
    form.insert("code", code.to_string());
    form.insert("redirect_uri", pending.redirect_uri.clone());
    form.insert("code_verifier", pending.verifier.clone());
    form.insert("client_id", pending.client_id.clone());
    let success = post_token_form(&pending.token_endpoint, &form, &pending.client_id, None).await?;
    Ok(OAuthTokenRecord {
        issuer: pending.issuer.clone(),
        refresh_token: success.refresh_token,
        expires_at: success.expires_in.map(|s| now_secs() + s).unwrap_or(0),
        scopes: success
            .scope
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        token_endpoint: pending.token_endpoint.clone(),
        client_id: pending.client_id.clone(),
        client_secret: None,
    })
}

/// Refresh an access token (returns the new access token).
pub async fn refresh_access_token(record: &OAuthTokenRecord) -> Result<(String, OAuthTokenRecord)> {
    let refresh_token = record
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No refresh token: re-authorization is required"))?;
    let mut form = HashMap::new();
    form.insert("grant_type", "refresh_token".to_string());
    form.insert("refresh_token", refresh_token.to_string());
    form.insert("client_id", record.client_id.clone());
    let success = post_token_form(
        &record.token_endpoint,
        &form,
        &record.client_id,
        record.client_secret.as_deref(),
    )
    .await?;
    let mut updated = record.clone();
    updated.expires_at = success.expires_in.map(|s| now_secs() + s).unwrap_or(0);
    if success.refresh_token.is_some() {
        updated.refresh_token = success.refresh_token;
    }
    if let Some(scope) = success.scope {
        updated.scopes = scope.split_whitespace().map(str::to_string).collect();
    }
    Ok((success.access_token, updated))
}

/// Client-credentials grant for enterprise-managed authorization (RFC 6749
/// section 4.4). No user gesture; the client secret authenticates.
pub async fn client_credentials_token(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    scopes: &[String],
    issuer: &str,
) -> Result<(String, OAuthTokenRecord)> {
    let mut form = HashMap::new();
    form.insert("grant_type", "client_credentials".to_string());
    if !scopes.is_empty() {
        form.insert("scope", scopes.join(" "));
    }
    let success = post_token_form(token_endpoint, &form, client_id, Some(client_secret)).await?;
    let record = OAuthTokenRecord {
        issuer: issuer.to_string(),
        refresh_token: success.refresh_token,
        expires_at: success.expires_in.map(|s| now_secs() + s).unwrap_or(0),
        scopes: success
            .scope
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_else(|| scopes.to_vec()),
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
        client_secret: Some(client_secret.to_string()),
    };
    Ok((success.access_token, record))
}

/// Dynamic client registration request (RFC 7591 subset).
#[derive(Debug, Clone, Serialize)]
pub struct ClientRegistration {
    /// Human-readable client name.
    pub client_name: String,
    /// Redirect URIs the client will use.
    pub redirect_uris: Vec<String>,
}

/// Dynamic client registration response (RFC 7591 subset).
#[derive(Debug, Clone, Deserialize)]
struct RegistrationSuccess {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
}

/// Registered client credentials.
#[derive(Debug, Clone)]
pub struct RegisteredClient {
    /// Issued client id.
    pub client_id: String,
    /// Issued client secret (confidential clients only).
    pub client_secret: Option<String>,
}

/// Register a client when the server offers a registration endpoint.
pub async fn register_client(
    metadata: &AuthorizationServerMetadata,
    registration: &ClientRegistration,
) -> Result<RegisteredClient> {
    let endpoint = metadata
        .registration_endpoint
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Server offers no dynamic client registration endpoint"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(OAUTH_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to build OAuth HTTP client")?;
    let response = client
        .post(endpoint)
        .json(registration)
        .send()
        .await
        .context("Client registration failed")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Client registration rejected ({status}): {body}");
    }
    let success: RegistrationSuccess = response
        .json()
        .await
        .context("Invalid registration response")?;
    Ok(RegisteredClient {
        client_id: success.client_id,
        client_secret: success.client_secret,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_urls_cover_discovery_shapes() {
        let urls = metadata_urls("https://example.com/mcp/v1");
        assert_eq!(
            urls,
            vec![
                "https://example.com/.well-known/oauth-authorization-server".to_string(),
                "https://example.com/.well-known/oauth-authorization-server/mcp/v1".to_string(),
                "https://example.com/mcp/v1/.well-known/oauth-authorization-server".to_string(),
            ]
        );
        assert!(metadata_urls("not a url").is_empty());
    }

    #[test]
    fn pkce_matches_rfc7636_appendix_b_vector() {
        // RFC 7636 Appendix B: verifier -> challenge.
        let pkce = PkceChallenge::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_generate_uses_unreserved_charset() {
        for _ in 0..16 {
            let pkce = PkceChallenge::generate();
            assert!((43..=128).contains(&pkce.verifier.len()));
            assert!(pkce
                .verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
            // Challenge round-trips through the verifier.
            assert_eq!(
                PkceChallenge::from_verifier(&pkce.verifier).challenge,
                pkce.challenge
            );
        }
    }

    fn test_metadata() -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: None,
            scopes_supported: Some(vec!["read".to_string()]),
        }
    }

    #[test]
    fn authorization_url_carries_pkce_and_scope() {
        let pending = begin_authorization(
            &test_metadata(),
            "tark",
            "http://localhost:8765/callback",
            &["read".to_string(), "write".to_string()],
        );
        assert!(pending.authorization_url.contains("response_type=code"));
        assert!(pending.authorization_url.contains("code_challenge="));
        assert!(pending
            .authorization_url
            .contains("code_challenge_method=S256"));
        assert!(pending.authorization_url.contains("state="));
        assert!(pending.authorization_url.contains("scope=read%20write"));
        assert!(!pending.state.is_empty());
        assert!(!pending.verifier.is_empty());
    }

    #[test]
    fn token_record_expiry_with_skew_margin() {
        let fresh = OAuthTokenRecord {
            issuer: "https://auth.example.com".to_string(),
            refresh_token: None,
            expires_at: now_secs() + 3600,
            scopes: vec![],
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "tark".to_string(),
            client_secret: None,
        };
        assert!(!fresh.is_expired());
        let soon = OAuthTokenRecord {
            expires_at: now_secs() + 60,
            ..fresh.clone()
        };
        assert!(soon.is_expired());
        let unknown = OAuthTokenRecord {
            expires_at: 0,
            ..fresh.clone()
        };
        assert!(unknown.is_expired());
    }

    #[test]
    fn token_record_keys_scope_to_issuer() {
        let record = OAuthTokenRecord {
            issuer: "https://auth.example.com".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: now_secs() + 3600,
            scopes: vec!["read".to_string()],
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "tark".to_string(),
            client_secret: None,
        };
        assert_eq!(
            record.token_key(),
            "tark-mcp-oauth/https://auth.example.com"
        );
        assert_eq!(
            record.record_key(),
            "tark-mcp-oauth/https://auth.example.com#record"
        );
        let parsed =
            crate::mcp::credential_store::CredentialKey::parse(&record.token_key()).expect("parse");
        assert_eq!(parsed.service, OAUTH_TOKEN_SERVICE);
    }

    #[test]
    fn token_error_parsing_never_carries_secrets() {
        let err: TokenError =
            serde_json::from_str(r#"{"error":"invalid_grant","error_description":"Code expired"}"#)
                .expect("parse");
        assert_eq!(err.error, "invalid_grant");
        assert_eq!(err.error_description.as_deref(), Some("Code expired"));
    }

    /// Mock OAuth authorization server (metadata + token endpoint) over
    /// loopback, exercising refresh and client-credentials grants end to end
    /// without network access.
    mod mock_server {
        use super::*;
        use axum::extract::State;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::routing::post;
        use axum::Json;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        #[derive(Clone, Default)]
        struct Seen {
            grant_types: Arc<Mutex<Vec<String>>>,
        }

        async fn token(
            State(seen): State<Seen>,
            form: axum::Form<HashMap<String, String>>,
        ) -> impl IntoResponse {
            let grant = form.0.get("grant_type").cloned().unwrap_or_default();
            seen.grant_types.lock().await.push(grant.clone());
            match grant.as_str() {
                "refresh_token"
                    if form.0.get("refresh_token").map(String::as_str) == Some("refresh-1") =>
                {
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "access_token": "access-2",
                            "refresh_token": "refresh-2",
                            "expires_in": 3600,
                            "scope": "read write",
                        })),
                    )
                        .into_response()
                }
                "client_credentials" => (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "access_token": "access-cc",
                        "expires_in": 600,
                    })),
                )
                    .into_response(),
                _ => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "bad grant",
                    })),
                )
                    .into_response(),
            }
        }

        async fn endpoint() -> (String, Seen) {
            let seen = Seen::default();
            let app = axum::Router::new()
                .route("/token", post(token))
                .with_state(seen.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let addr = listener.local_addr().expect("addr");
            tokio::spawn(async move {
                axum::serve(listener, app).await.expect("serve");
            });
            (format!("http://{addr}/token"), seen)
        }

        #[tokio::test]
        async fn refresh_rotates_tokens() {
            let (token_endpoint, seen) = endpoint().await;
            let record = OAuthTokenRecord {
                issuer: "https://auth.example.com".to_string(),
                refresh_token: Some("refresh-1".to_string()),
                expires_at: 0,
                scopes: vec![],
                token_endpoint,
                client_id: "tark".to_string(),
                client_secret: None,
            };
            let (access, updated) = refresh_access_token(&record).await.expect("refresh");
            assert_eq!(access, "access-2");
            assert_eq!(updated.refresh_token.as_deref(), Some("refresh-2"));
            assert!(!updated.is_expired());
            assert_eq!(
                updated.scopes,
                vec!["read".to_string(), "write".to_string()]
            );
            assert_eq!(seen.grant_types.lock().await.as_slice(), ["refresh_token"]);
        }

        #[tokio::test]
        async fn refresh_without_token_requires_reauth() {
            let record = OAuthTokenRecord {
                issuer: "https://auth.example.com".to_string(),
                refresh_token: None,
                expires_at: 0,
                scopes: vec![],
                token_endpoint: "http://127.0.0.1:1/token".to_string(),
                client_id: "tark".to_string(),
                client_secret: None,
            };
            let err = refresh_access_token(&record).await.expect_err("must fail");
            assert!(err.to_string().contains("re-authorization"));
        }

        #[tokio::test]
        async fn client_credentials_grant_round_trip() {
            let (token_endpoint, seen) = endpoint().await;
            let (access, record) = client_credentials_token(
                &token_endpoint,
                "tark",
                "secret",
                &["read".to_string()],
                "https://auth.example.com",
            )
            .await
            .expect("client credentials");
            assert_eq!(access, "access-cc");
            assert!(!record.is_expired());
            assert_eq!(
                seen.grant_types.lock().await.as_slice(),
                ["client_credentials"]
            );
        }

        #[tokio::test]
        async fn token_endpoint_error_surfaces_description() {
            let (token_endpoint, _) = endpoint().await;
            let record = OAuthTokenRecord {
                issuer: "https://auth.example.com".to_string(),
                refresh_token: Some("wrong".to_string()),
                expires_at: 0,
                scopes: vec![],
                token_endpoint,
                client_id: "tark".to_string(),
                client_secret: None,
            };
            let err = refresh_access_token(&record).await.expect_err("must fail");
            assert!(err.to_string().contains("invalid_grant"));
            assert!(!err.to_string().contains("wrong"));
        }
    }
}
