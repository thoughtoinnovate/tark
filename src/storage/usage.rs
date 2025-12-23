//! Usage tracking and billing storage
//!
//! Stores token usage and costs in SQLite database (.tark/usage.db)
//! Fetches pricing from models.dev API

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Usage tracker with SQLite backend
pub struct UsageTracker {
    db_path: PathBuf,
    db: Arc<Mutex<Connection>>,
    pricing: Arc<Mutex<PricingCache>>,
}

impl UsageTracker {
    /// Create a new usage tracker
    pub fn new(workspace_dir: impl AsRef<Path>) -> Result<Self> {
        let db_path = workspace_dir.as_ref().join("usage.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                host TEXT NOT NULL,
                username TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                mode TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cost_usd REAL NOT NULL,
                request_type TEXT NOT NULL,
                estimated BOOLEAN DEFAULT 0,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            )",
            [],
        )?;

        // Create indices
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logs_session ON usage_logs(session_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON usage_logs(timestamp)",
            [],
        )?;

        Ok(Self {
            db_path,
            db: Arc::new(Mutex::new(conn)),
            pricing: Arc::new(Mutex::new(PricingCache::new())),
        })
    }

    /// Create a new session
    pub fn create_session(&self, host: &str, username: &str) -> Result<Session> {
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            name: None,
            host: host.to_string(),
            username: username.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let conn = self.db.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, host, username, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &session.id,
                &session.name,
                &session.host,
                &session.username,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;

        Ok(session)
    }

    /// Log usage
    pub fn log_usage(&self, log: UsageLog) -> Result<()> {
        let conn = self.db.lock().unwrap();
        conn.execute(
            "INSERT INTO usage_logs 
             (session_id, timestamp, provider, model, mode, input_tokens, output_tokens, 
              cost_usd, request_type, estimated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                &log.session_id,
                Utc::now().to_rfc3339(),
                &log.provider,
                &log.model,
                &log.mode,
                log.input_tokens,
                log.output_tokens,
                log.cost_usd,
                &log.request_type,
                log.estimated,
            ],
        )?;
        Ok(())
    }

    /// Get summary statistics
    pub fn get_summary(&self) -> Result<UsageSummary> {
        let conn = self.db.lock().unwrap();

        let (total_cost, total_tokens, log_count): (f64, i64, i64) = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0), 
                        COALESCE(SUM(input_tokens + output_tokens), 0),
                        COUNT(*)
                 FROM usage_logs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let session_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;

        let db_size = std::fs::metadata(&self.db_path)?.len();

        Ok(UsageSummary {
            total_cost,
            total_tokens: total_tokens as u64,
            session_count: session_count as u64,
            log_count: log_count as u64,
            db_size_bytes: db_size,
            db_size_human: format_bytes(db_size),
        })
    }

    /// Get usage by model
    pub fn get_usage_by_model(&self) -> Result<Vec<ModelUsage>> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT model, provider,
                    SUM(input_tokens) as input,
                    SUM(output_tokens) as output,
                    SUM(cost_usd) as cost,
                    COUNT(*) as count
             FROM usage_logs
             GROUP BY model, provider
             ORDER BY cost DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ModelUsage {
                model: row.get(0)?,
                provider: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cost: row.get(4)?,
                request_count: row.get(5)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to collect model usage")
    }

    /// Get usage by mode (TAB vs Agent breakdown)
    pub fn get_usage_by_mode(&self) -> Result<Vec<ModeUsage>> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT request_type, mode,
                    SUM(input_tokens + output_tokens) as tokens,
                    SUM(cost_usd) as cost,
                    COUNT(*) as count
             FROM usage_logs
             GROUP BY request_type, mode
             ORDER BY cost DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ModeUsage {
                request_type: row.get(0)?,
                mode: row.get(1)?,
                tokens: row.get(2)?,
                cost: row.get(3)?,
                request_count: row.get(4)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to collect mode usage")
    }

    /// Get all sessions
    pub fn get_sessions(&self) -> Result<Vec<SessionWithStats>> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, s.host, s.username, s.created_at, s.updated_at,
                    COUNT(l.id) as log_count,
                    COALESCE(SUM(l.input_tokens + l.output_tokens), 0) as total_tokens,
                    COALESCE(SUM(l.cost_usd), 0) as total_cost
             FROM sessions s
             LEFT JOIN usage_logs l ON s.id = l.session_id
             GROUP BY s.id
             ORDER BY s.created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SessionWithStats {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                username: row.get(3)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
                log_count: row.get(6)?,
                total_tokens: row.get(7)?,
                total_cost: row.get(8)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to collect sessions")
    }

    /// Get session logs
    pub fn get_session_logs(&self, session_id: &str) -> Result<Vec<UsageLogEntry>> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, provider, model, mode, 
                    input_tokens, output_tokens, cost_usd, request_type, estimated
             FROM usage_logs
             WHERE session_id = ?1
             ORDER BY timestamp DESC",
        )?;

        let rows = stmt.query_map([session_id], |row| {
            Ok(UsageLogEntry {
                id: row.get(0)?,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                provider: row.get(2)?,
                model: row.get(3)?,
                mode: row.get(4)?,
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                cost_usd: row.get(7)?,
                request_type: row.get(8)?,
                estimated: row.get(9)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to collect session logs")
    }

    /// Cleanup old logs
    pub async fn cleanup(&self, req: CleanupRequest) -> Result<CleanupResponse> {
        let size_before = std::fs::metadata(&self.db_path)?.len();

        let deleted_logs = if let Some(days) = req.older_than_days {
            let cutoff = Utc::now() - chrono::Duration::days(days as i64);
            let conn = self.db.lock().unwrap();
            conn.execute(
                "DELETE FROM usage_logs WHERE timestamp < ?1",
                params![cutoff.to_rfc3339()],
            )?
        } else if let Some(session_ids) = req.session_ids {
            let conn = self.db.lock().unwrap();
            let mut deleted = 0;
            for session_id in session_ids {
                deleted += conn.execute(
                    "DELETE FROM usage_logs WHERE session_id = ?1",
                    params![session_id],
                )?;
            }
            deleted
        } else if req.delete_all == Some(true) {
            let conn = self.db.lock().unwrap();
            conn.execute("DELETE FROM usage_logs", [])?
        } else {
            0
        };

        // Delete sessions with no logs
        let deleted_sessions = {
            let conn = self.db.lock().unwrap();
            conn.execute(
                "DELETE FROM sessions WHERE id NOT IN (SELECT DISTINCT session_id FROM usage_logs)",
                [],
            )?
        };

        // VACUUM to reclaim space
        {
            let conn = self.db.lock().unwrap();
            conn.execute("VACUUM", [])?;
        }

        let size_after = std::fs::metadata(&self.db_path)?.len();
        let freed_bytes = size_before.saturating_sub(size_after);

        Ok(CleanupResponse {
            deleted_logs: deleted_logs as u64,
            deleted_sessions: deleted_sessions as u64,
            freed_bytes,
            freed_human: format_bytes(freed_bytes),
            new_db_size_human: format_bytes(size_after),
        })
    }

    /// Calculate cost for a request
    pub async fn calculate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> f64 {
        let pricing = self.pricing.lock().unwrap();
        pricing.calculate_cost(provider, model, input_tokens, output_tokens)
    }

    /// Fetch pricing data from models.dev
    pub async fn fetch_pricing(&self) -> Result<()> {
        // Check if we need to fetch (without holding lock)
        let needs_fetch = {
            let pricing = self.pricing.lock().unwrap();
            match (pricing.fetched_at, &pricing.data) {
                (Some(fetched_at), Some(_)) => fetched_at.elapsed() >= pricing.cache_duration,
                _ => true,
            }
        };

        if !needs_fetch {
            return Ok(());
        }

        // Fetch without holding the lock
        let response = reqwest::get("https://models.dev/api.json")
            .await
            .context("Failed to fetch models.dev API")?;

        let data = response
            .json::<ModelsDevData>()
            .await
            .context("Failed to parse models.dev response")?;

        // Update cache
        let mut pricing = self.pricing.lock().unwrap();
        pricing.data = Some(data);
        pricing.fetched_at = Some(Instant::now());

        Ok(())
    }
}

/// Pricing cache from models.dev
pub struct PricingCache {
    data: Option<ModelsDevData>,
    fetched_at: Option<Instant>,
    cache_duration: Duration,
}

impl Default for PricingCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PricingCache {
    pub fn new() -> Self {
        Self {
            data: None,
            fetched_at: None,
            cache_duration: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Fetch pricing from models.dev
    pub async fn fetch(&mut self) -> Result<()> {
        // Check cache
        if let (Some(_data), Some(fetched_at)) = (&self.data, self.fetched_at) {
            if fetched_at.elapsed() < self.cache_duration {
                return Ok(());
            }
        }

        // Fetch from models.dev
        let response = reqwest::get("https://models.dev/api.json")
            .await
            .context("Failed to fetch models.dev API")?;

        self.data = Some(
            response
                .json::<ModelsDevData>()
                .await
                .context("Failed to parse models.dev response")?,
        );
        self.fetched_at = Some(Instant::now());

        Ok(())
    }

    /// Calculate cost for a request
    pub fn calculate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> f64 {
        // Try to get from models.dev data
        if let Some(ref data) = self.data {
            if let Some(provider_data) = data.get(provider) {
                if let Some(models) = &provider_data.models {
                    if let Some(model_data) = models.get(model) {
                        if let Some(cost) = &model_data.cost {
                            let input_cost = cost.input * (input_tokens as f64) / 1_000_000.0;
                            let output_cost = cost.output * (output_tokens as f64) / 1_000_000.0;
                            return input_cost + output_cost;
                        }
                    }
                }
            }
        }

        // Fallback to default pricing
        let default_pricing = Self::default_pricing();
        if let Some(pricing) = default_pricing.get(&(provider, model)) {
            let input_cost = pricing.input * (input_tokens as f64) / 1_000_000.0;
            let output_cost = pricing.output * (output_tokens as f64) / 1_000_000.0;
            return input_cost + output_cost;
        }

        // Unknown model = free (e.g., Ollama)
        0.0
    }

    fn default_pricing() -> HashMap<(&'static str, &'static str), ModelPricing> {
        [
            (
                ("openai", "gpt-4o"),
                ModelPricing {
                    input: 5.0,
                    output: 15.0,
                },
            ),
            (
                ("openai", "gpt-4o-mini"),
                ModelPricing {
                    input: 0.15,
                    output: 0.60,
                },
            ),
            (
                ("anthropic", "claude-sonnet-4-20250514"),
                ModelPricing {
                    input: 3.0,
                    output: 15.0,
                },
            ),
            (
                ("anthropic", "claude-3-5-sonnet"),
                ModelPricing {
                    input: 3.0,
                    output: 15.0,
                },
            ),
        ]
        .into()
    }
}

// Data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub host: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UsageLog {
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub mode: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f64,
    pub request_type: String,
    pub estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLogEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub mode: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f64,
    pub request_type: String,
    pub estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_cost: f64,
    pub total_tokens: u64,
    pub session_count: u64,
    pub log_count: u64,
    pub db_size_bytes: u64,
    pub db_size_human: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: f64,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeUsage {
    pub request_type: String,
    pub mode: String,
    pub tokens: u64,
    pub cost: f64,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWithStats {
    pub id: String,
    pub name: Option<String>,
    pub host: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub log_count: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanupRequest {
    pub older_than_days: Option<u32>,
    pub session_ids: Option<Vec<String>>,
    pub delete_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupResponse {
    pub deleted_logs: u64,
    pub deleted_sessions: u64,
    pub freed_bytes: u64,
    pub freed_human: String,
    pub new_db_size_human: String,
}

// models.dev API types

type ModelsDevData = HashMap<String, ProviderData>;

#[derive(Debug, Deserialize)]
struct ProviderData {
    models: Option<HashMap<String, ModelData>>,
}

#[derive(Debug, Deserialize)]
struct ModelData {
    cost: Option<CostData>,
}

#[derive(Debug, Deserialize)]
struct CostData {
    input: f64,
    output: f64,
}

#[derive(Debug, Clone)]
struct ModelPricing {
    input: f64,
    output: f64,
}

// Utilities

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_session() {
        let tmp = TempDir::new().unwrap();
        let tracker = UsageTracker::new(tmp.path()).unwrap();

        let session = tracker.create_session("test-host", "test-user").unwrap();
        assert!(!session.id.is_empty());
        assert_eq!(session.host, "test-host");
        assert_eq!(session.username, "test-user");
    }

    #[test]
    fn test_log_usage() {
        let tmp = TempDir::new().unwrap();
        let tracker = UsageTracker::new(tmp.path()).unwrap();
        let session = tracker.create_session("host", "user").unwrap();

        tracker
            .log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                mode: "agent-build".into(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                request_type: "chat".into(),
                estimated: false,
            })
            .unwrap();

        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_tokens, 150);
        assert_eq!(summary.log_count, 1);
    }

    #[test]
    fn test_get_usage_by_model() {
        let tmp = TempDir::new().unwrap();
        let tracker = UsageTracker::new(tmp.path()).unwrap();
        let session = tracker.create_session("host", "user").unwrap();

        // Log usage for two models
        tracker
            .log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                mode: "agent-build".into(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                request_type: "chat".into(),
                estimated: false,
            })
            .unwrap();

        tracker
            .log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                mode: "agent-plan".into(),
                input_tokens: 200,
                output_tokens: 100,
                cost_usd: 0.002,
                request_type: "chat".into(),
                estimated: false,
            })
            .unwrap();

        let models = tracker.get_usage_by_model().unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.model == "gpt-4o"));
        assert!(models.iter().any(|m| m.model == "claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_get_usage_by_mode() {
        let tmp = TempDir::new().unwrap();
        let tracker = UsageTracker::new(tmp.path()).unwrap();
        let session = tracker.create_session("host", "user").unwrap();

        // Log chat and completion usage
        tracker
            .log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                mode: "agent-build".into(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                request_type: "chat".into(),
                estimated: false,
            })
            .unwrap();

        tracker
            .log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                mode: "completion".into(),
                input_tokens: 50,
                output_tokens: 20,
                cost_usd: 0.0005,
                request_type: "fim".into(),
                estimated: false,
            })
            .unwrap();

        let modes = tracker.get_usage_by_mode().unwrap();
        assert_eq!(modes.len(), 2);
        assert!(modes.iter().any(|m| m.request_type == "chat"));
        assert!(modes.iter().any(|m| m.request_type == "fim"));
    }

    #[tokio::test]
    async fn test_cleanup_older_than() {
        let tmp = TempDir::new().unwrap();
        let tracker = UsageTracker::new(tmp.path()).unwrap();
        let session = tracker.create_session("host", "user").unwrap();

        // Log some usage
        for _ in 0..5 {
            tracker
                .log_usage(UsageLog {
                    session_id: session.id.clone(),
                    provider: "openai".into(),
                    model: "gpt-4o".into(),
                    mode: "agent-build".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cost_usd: 0.001,
                    request_type: "chat".into(),
                    estimated: false,
                })
                .unwrap();
        }

        let summary_before = tracker.get_summary().unwrap();
        assert_eq!(summary_before.log_count, 5);

        // Cleanup logs older than 0 days (all logs)
        let cleanup_result = tracker
            .cleanup(CleanupRequest {
                older_than_days: Some(0),
                session_ids: None,
                delete_all: None,
            })
            .await
            .unwrap();

        assert_eq!(cleanup_result.deleted_logs, 5);
        assert_eq!(cleanup_result.deleted_sessions, 1);

        let summary_after = tracker.get_summary().unwrap();
        assert_eq!(summary_after.log_count, 0);
    }

    #[test]
    fn test_pricing_cache_calculate_cost() {
        let pricing = PricingCache::new();

        // Test with default pricing (gpt-4o: $5/M input, $15/M output)
        let cost = pricing.calculate_cost("openai", "gpt-4o", 1000, 500);
        // Expected: (1000 * 5 + 500 * 15) / 1_000_000 = 0.0125
        assert!((cost - 0.0125).abs() < 0.0001);
    }

    #[test]
    fn test_pricing_cache_ollama_free() {
        let pricing = PricingCache::new();
        let cost = pricing.calculate_cost("ollama", "llama3", 10000, 5000);
        assert_eq!(cost, 0.0); // Ollama is free
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(100), "100 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }
}
