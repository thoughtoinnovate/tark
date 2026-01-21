//! Models database integration with models.dev
//!
//! Provides dynamic model information including pricing, capabilities,
//! and limits from the models.dev open-source database.
//!
//! API: https://models.dev/api.json

#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Models.dev API endpoint
const MODELS_API_URL: &str = "https://models.dev/api.json";

/// Cache duration in seconds (24 hours)
const CACHE_DURATION_SECS: u64 = 86400;

/// Model cost information (per million tokens)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCost {
    /// Input token cost per million
    #[serde(default)]
    pub input: f64,
    /// Output token cost per million
    #[serde(default)]
    pub output: f64,
    /// Cache read cost per million (for prompt caching)
    #[serde(default)]
    pub cache_read: Option<f64>,
    /// Cache write cost per million (for prompt caching)
    #[serde(default)]
    pub cache_write: Option<f64>,
    /// Reasoning/thinking token cost per million (if different from output)
    #[serde(default)]
    pub reasoning: Option<f64>,
}

impl ModelCost {
    /// Get the cost for reasoning tokens (falls back to output cost)
    pub fn reasoning_cost_per_million(&self) -> f64 {
        self.reasoning.unwrap_or(self.output)
    }
}

/// Model context/output limits
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelLimits {
    /// Maximum context window size in tokens
    #[serde(default)]
    pub context: u32,
    /// Maximum output tokens
    #[serde(default)]
    pub output: u32,
}

/// Model input/output modalities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelModalities {
    /// Supported input types (text, image, audio, video, pdf)
    #[serde(default)]
    pub input: Vec<String>,
    /// Supported output types (text, image, audio, video)
    #[serde(default)]
    pub output: Vec<String>,
}

/// Complete model information from models.dev
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID (used for API calls)
    pub id: String,
    /// Human-readable model name
    pub name: String,
    /// Model family (e.g., "gpt-4o", "claude-sonnet")
    #[serde(default)]
    pub family: Option<String>,
    /// Whether model supports file/image attachments
    #[serde(default)]
    pub attachment: bool,
    /// Whether model supports reasoning/thinking mode
    #[serde(default)]
    pub reasoning: bool,
    /// Whether model supports tool/function calling
    #[serde(default)]
    pub tool_call: bool,
    /// Whether model supports temperature parameter
    #[serde(default)]
    pub temperature: bool,
    /// Whether model supports structured output
    #[serde(default)]
    pub structured_output: Option<bool>,
    /// Knowledge cutoff date (e.g., "2024-04")
    #[serde(default)]
    pub knowledge: Option<String>,
    /// Model release date
    #[serde(default)]
    pub release_date: Option<String>,
    /// Last updated date
    #[serde(default)]
    pub last_updated: Option<String>,
    /// Input/output modalities
    #[serde(default)]
    pub modalities: ModelModalities,
    /// Whether model has open weights
    #[serde(default)]
    pub open_weights: bool,
    /// Pricing information
    #[serde(default)]
    pub cost: ModelCost,
    /// Token limits
    #[serde(default)]
    pub limit: ModelLimits,
}

impl ModelInfo {
    /// Check if model supports vision (image input)
    pub fn supports_vision(&self) -> bool {
        self.attachment && self.modalities.input.iter().any(|m| m == "image")
    }

    /// Check if model supports audio input
    pub fn supports_audio_input(&self) -> bool {
        self.modalities.input.iter().any(|m| m == "audio")
    }

    /// Check if model supports video input
    pub fn supports_video_input(&self) -> bool {
        self.modalities.input.iter().any(|m| m == "video")
    }

    /// Check if model supports PDF input
    pub fn supports_pdf(&self) -> bool {
        self.modalities.input.iter().any(|m| m == "pdf")
    }

    /// Check if model can generate images
    pub fn supports_image_output(&self) -> bool {
        self.modalities.output.iter().any(|m| m == "image")
    }

    /// Check if model can generate audio
    pub fn supports_audio_output(&self) -> bool {
        self.modalities.output.iter().any(|m| m == "audio")
    }

    /// Calculate cost for given token counts
    pub fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        let input_cost = (input_tokens as f64) * self.cost.input / 1_000_000.0;
        let output_cost = (output_tokens as f64) * self.cost.output / 1_000_000.0;
        input_cost + output_cost
    }

    /// Calculate cost including cache tokens
    pub fn calculate_cost_with_cache(
        &self,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_write_tokens: u32,
    ) -> f64 {
        let mut total = self.calculate_cost(input_tokens, output_tokens);

        if let Some(cache_read_price) = self.cost.cache_read {
            total += (cache_read_tokens as f64) * cache_read_price / 1_000_000.0;
        }
        if let Some(cache_write_price) = self.cost.cache_write {
            total += (cache_write_tokens as f64) * cache_write_price / 1_000_000.0;
        }

        total
    }

    /// Get a capability summary string
    pub fn capability_summary(&self) -> String {
        let mut caps = Vec::new();

        if self.tool_call {
            caps.push("tools");
        }
        if self.reasoning {
            caps.push("reasoning");
        }
        if self.supports_vision() {
            caps.push("vision");
        }
        if self.supports_audio_input() {
            caps.push("audio");
        }
        if self.supports_pdf() {
            caps.push("pdf");
        }
        if self.structured_output.unwrap_or(false) {
            caps.push("structured");
        }

        if caps.is_empty() {
            "text".to_string()
        } else {
            caps.join(", ")
        }
    }
}

/// Provider information from models.dev
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider ID
    pub id: String,
    /// Human-readable provider name
    pub name: String,
    /// Required environment variables
    #[serde(default)]
    pub env: Vec<String>,
    /// NPM package for AI SDK
    #[serde(default)]
    pub npm: Option<String>,
    /// API base URL (for OpenAI-compatible providers)
    #[serde(default)]
    pub api: Option<String>,
    /// Documentation URL
    #[serde(default)]
    pub doc: Option<String>,
    /// Available models
    #[serde(default)]
    pub models: HashMap<String, ModelInfo>,
}

/// The complete models database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsDatabase {
    /// Providers indexed by ID
    #[serde(flatten)]
    pub providers: HashMap<String, ProviderInfo>,
}

/// Cache entry with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    data: ModelsDatabase,
    timestamp: u64,
}

/// Models database manager with caching
pub struct ModelsDbManager {
    /// Cached database
    cache: Arc<RwLock<Option<CacheEntry>>>,
    /// Cache file path
    cache_path: Option<PathBuf>,
    /// HTTP client
    client: reqwest::Client,
}

impl Default for ModelsDbManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelsDbManager {
    /// Create a new models database manager
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            cache_path: None,
            client: reqwest::Client::new(),
        }
    }

    /// Create with a cache directory
    pub fn with_cache_dir(mut self, cache_dir: PathBuf) -> Self {
        self.cache_path = Some(cache_dir.join("models_db.json"));
        self
    }

    /// Get the current timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Check if cache is valid
    fn is_cache_valid(entry: &CacheEntry) -> bool {
        let now = Self::current_timestamp();
        now - entry.timestamp < CACHE_DURATION_SECS
    }

    /// Load cache from disk
    async fn load_disk_cache(&self) -> Option<CacheEntry> {
        let path = self.cache_path.as_ref()?;
        let content = tokio::fs::read_to_string(path).await.ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save cache to disk
    async fn save_disk_cache(&self, entry: &CacheEntry) {
        if let Some(ref path) = self.cache_path {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            if let Ok(content) = serde_json::to_string(entry) {
                let _ = tokio::fs::write(path, content).await;
            }
        }
    }

    /// Fetch fresh data from the API
    async fn fetch_from_api(&self) -> Result<ModelsDatabase> {
        tracing::info!("Fetching models database from {}", MODELS_API_URL);

        let response = self
            .client
            .get(MODELS_API_URL)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        let data: ModelsDatabase = response.json().await?;
        tracing::info!("Loaded {} providers from models.dev", data.providers.len());

        Ok(data)
    }

    /// Get the models database (with caching)
    pub async fn get_database(&self) -> Result<ModelsDatabase> {
        // Check memory cache first
        {
            let cache = self.cache.read().await;
            if let Some(ref entry) = *cache {
                if Self::is_cache_valid(entry) {
                    return Ok(entry.data.clone());
                }
            }
        }

        // Check disk cache
        if let Some(entry) = self.load_disk_cache().await {
            if Self::is_cache_valid(&entry) {
                // Update memory cache
                let mut cache = self.cache.write().await;
                *cache = Some(entry.clone());
                return Ok(entry.data);
            }
        }

        // Fetch from API
        let data = self.fetch_from_api().await?;
        let entry = CacheEntry {
            data: data.clone(),
            timestamp: Self::current_timestamp(),
        };

        // Update caches
        self.save_disk_cache(&entry).await;
        let mut cache = self.cache.write().await;
        *cache = Some(entry);

        Ok(data)
    }

    /// Get model info by provider and model ID
    pub async fn get_model(&self, provider: &str, model_id: &str) -> Result<Option<ModelInfo>> {
        let db = self.get_database().await?;

        // Normalize provider name
        let provider_key = Self::normalize_provider(provider);

        if provider_key == "tark_sim" {
            // Return synthetic model for simulation
            return Ok(Some(ModelInfo {
                id: "tark_llm".to_string(),
                name: "Tark Simulation".to_string(),
                family: Some("simulation".to_string()),
                attachment: true,
                reasoning: true,
                tool_call: true,
                temperature: true,
                structured_output: Some(true),
                knowledge: Some("2024-04".to_string()),
                release_date: Some("2024-01-01".to_string()),
                last_updated: None,
                modalities: ModelModalities {
                    input: vec!["text".to_string()],
                    output: vec!["text".to_string()],
                },
                open_weights: true,
                cost: ModelCost::default(),
                limit: ModelLimits {
                    context: 8192,
                    output: 4096,
                },
            }));
        }

        // Special handling for Ollama - load from local instance
        if provider_key == "ollama" {
            match crate::llm::list_local_ollama_models().await {
                Ok(ollama_models) => {
                    // Find the requested model
                    for m in ollama_models {
                        if m.name == model_id {
                            let supports_tools = Self::ollama_model_supports_tools(&m.name);
                            return Ok(Some(ModelInfo {
                                id: m.name.clone(),
                                name: m.name.clone(),
                                family: None,
                                attachment: false,
                                reasoning: false,
                                tool_call: supports_tools,
                                temperature: true,
                                structured_output: None,
                                knowledge: None,
                                release_date: None,
                                last_updated: Some(m.modified_at),
                                modalities: ModelModalities {
                                    input: vec!["text".to_string()],
                                    output: vec!["text".to_string()],
                                },
                                open_weights: true,
                                cost: ModelCost::default(),
                                limit: ModelLimits {
                                    context: 8192,
                                    output: 4096,
                                },
                            }));
                        }
                    }
                    return Ok(None); // Model not found
                }
                Err(_) => {
                    return Ok(None); // Ollama not available
                }
            }
        }

        if let Some(provider_info) = db.providers.get(&provider_key) {
            // Try exact match first
            if let Some(model) = provider_info.models.get(model_id) {
                return Ok(Some(model.clone()));
            }

            // Try fuzzy match (model ID might be slightly different)
            let model_lower = model_id.to_lowercase();
            for (id, model) in &provider_info.models {
                if id.to_lowercase() == model_lower
                    || id.to_lowercase().contains(&model_lower)
                    || model_lower.contains(&id.to_lowercase())
                {
                    return Ok(Some(model.clone()));
                }
            }
        }

        Ok(None)
    }

    /// Get provider info
    pub async fn get_provider(&self, provider: &str) -> Result<Option<ProviderInfo>> {
        let db = self.get_database().await?;
        let provider_key = Self::normalize_provider(provider);

        if provider_key == "tark_sim" {
            let mut models = HashMap::new();
            models.insert(
                "tark_llm".to_string(),
                ModelInfo {
                    id: "tark_llm".to_string(),
                    name: "Tark Simulation".to_string(),
                    family: Some("simulation".to_string()),
                    attachment: true,
                    reasoning: true,
                    tool_call: true,
                    temperature: true,
                    structured_output: Some(true),
                    knowledge: Some("2024-04".to_string()),
                    release_date: Some("2024-01-01".to_string()),
                    last_updated: None,
                    modalities: ModelModalities {
                        input: vec!["text".to_string()],
                        output: vec!["text".to_string()],
                    },
                    open_weights: true,
                    cost: ModelCost::default(),
                    limit: ModelLimits {
                        context: 8192,
                        output: 4096,
                    },
                },
            );

            return Ok(Some(ProviderInfo {
                id: "tark_sim".to_string(),
                name: "Tark Simulation".to_string(),
                env: vec![],
                npm: None,
                api: None,
                doc: None,
                models,
            }));
        }

        // Special handling for Ollama - load from local instance
        if provider_key == "ollama" {
            match crate::llm::list_local_ollama_models().await {
                Ok(ollama_models) => {
                    let mut models = HashMap::new();
                    for m in ollama_models {
                        let supports_tools = Self::ollama_model_supports_tools(&m.name);
                        models.insert(
                            m.name.clone(),
                            ModelInfo {
                                id: m.name.clone(),
                                name: m.name.clone(),
                                family: None,
                                attachment: false,
                                reasoning: false,
                                tool_call: supports_tools,
                                temperature: true,
                                structured_output: None,
                                knowledge: None,
                                release_date: None,
                                last_updated: Some(m.modified_at),
                                modalities: ModelModalities {
                                    input: vec!["text".to_string()],
                                    output: vec!["text".to_string()],
                                },
                                open_weights: true,
                                cost: ModelCost::default(),
                                limit: ModelLimits {
                                    context: 8192,
                                    output: 4096,
                                },
                            },
                        );
                    }

                    return Ok(Some(ProviderInfo {
                        id: "ollama".to_string(),
                        name: "Ollama".to_string(),
                        env: vec![], // Ollama uses localhost by default, no env required
                        npm: None,
                        api: Some("http://localhost:11434".to_string()),
                        doc: Some("https://ollama.ai".to_string()),
                        models,
                    }));
                }
                Err(_) => {
                    // Return empty provider if Ollama not available
                    return Ok(Some(ProviderInfo {
                        id: "ollama".to_string(),
                        name: "Ollama".to_string(),
                        env: vec![], // Ollama uses localhost by default, no env required
                        npm: None,
                        api: Some("http://localhost:11434".to_string()),
                        doc: Some("https://ollama.ai".to_string()),
                        models: HashMap::new(),
                    }));
                }
            }
        }

        Ok(db.providers.get(&provider_key).cloned())
    }

    /// List all available providers
    pub async fn list_providers(&self) -> Result<Vec<String>> {
        let db = self.get_database().await?;
        let mut providers: Vec<String> = db.providers.keys().cloned().collect();
        providers.push("tark_sim".to_string());
        providers.push("ollama".to_string());
        Ok(providers)
    }

    /// List models for a provider
    pub async fn list_models(&self, provider: &str) -> Result<Vec<ModelInfo>> {
        let db = self.get_database().await?;
        let provider_key = Self::normalize_provider(provider);

        if provider_key == "tark_sim" {
            return Ok(vec![ModelInfo {
                id: "tark_llm".to_string(),
                name: "Tark Simulation".to_string(),
                family: Some("simulation".to_string()),
                attachment: true,
                reasoning: true,
                tool_call: true,
                temperature: true,
                structured_output: Some(true),
                knowledge: Some("2024-04".to_string()),
                release_date: Some("2024-01-01".to_string()),
                last_updated: None,
                modalities: ModelModalities {
                    input: vec!["text".to_string()],
                    output: vec!["text".to_string()],
                },
                open_weights: true,
                cost: ModelCost::default(),
                limit: ModelLimits {
                    context: 8192,
                    output: 4096,
                },
            }]);
        }

        // Special handling for Ollama - load models from local instance
        if provider_key == "ollama" {
            match crate::llm::list_local_ollama_models().await {
                Ok(ollama_models) => {
                    return Ok(ollama_models
                        .into_iter()
                        .map(|m| {
                            let supports_tools = Self::ollama_model_supports_tools(&m.name);
                            ModelInfo {
                                id: m.name.clone(),
                                name: m.name.clone(),
                                family: None,
                                attachment: false,
                                reasoning: false,
                                tool_call: supports_tools,
                                temperature: true,
                                structured_output: None,
                                knowledge: None,
                                release_date: None,
                                last_updated: Some(m.modified_at),
                                modalities: ModelModalities {
                                    input: vec!["text".to_string()],
                                    output: vec!["text".to_string()],
                                },
                                open_weights: true,
                                cost: ModelCost::default(), // Local models are free
                                limit: ModelLimits {
                                    context: 8192, // Default, varies by model
                                    output: 4096,
                                },
                            }
                        })
                        .collect());
                }
                Err(e) => {
                    tracing::warn!("Failed to load Ollama models: {}", e);
                    return Ok(vec![]); // Graceful degradation if Ollama not running
                }
            }
        }

        if let Some(provider_info) = db.providers.get(&provider_key) {
            Ok(provider_info.models.values().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Try to get models from cache without blocking (returns None if cache not ready)
    pub fn try_get_cached(&self, provider: &str) -> Option<Vec<ModelInfo>> {
        // Try to read from memory cache without blocking
        let cache_guard = self.cache.try_read().ok()?;
        let entry = cache_guard.as_ref()?;

        if !Self::is_cache_valid(entry) {
            return None;
        }

        let provider_key = Self::normalize_provider(provider);
        entry
            .data
            .providers
            .get(&provider_key)
            .map(|p| p.models.values().cloned().collect())
    }

    /// Try to get a specific model from the in-memory cache without blocking.
    ///
    /// Returns `None` if the cache is not ready/expired or the model isn't present.
    pub fn try_get_cached_model(&self, provider: &str, model_id: &str) -> Option<ModelInfo> {
        let cache_guard = self.cache.try_read().ok()?;
        let entry = cache_guard.as_ref()?;

        if !Self::is_cache_valid(entry) {
            return None;
        }

        let provider_key = Self::normalize_provider(provider);
        entry
            .data
            .providers
            .get(&provider_key)
            .and_then(|p| p.models.get(model_id))
            .cloned()
    }

    /// Try to get a model's context window from cache without blocking.
    ///
    /// Returns `None` if cache isn't ready or model is missing.
    pub fn try_get_cached_context_limit(&self, provider: &str, model_id: &str) -> Option<u32> {
        let model = self.try_get_cached_model(provider, model_id)?;
        if model.limit.context > 0 {
            Some(model.limit.context)
        } else {
            None
        }
    }

    /// Try to calculate request cost from cache without blocking.
    ///
    /// Returns `None` if cache isn't ready or model is missing.
    pub fn try_calculate_cost_cached(
        &self,
        provider: &str,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Option<f64> {
        let model = self.try_get_cached_model(provider, model_id)?;
        Some(model.calculate_cost(input_tokens, output_tokens))
    }

    /// Preload models database in background (call at app startup)
    pub fn preload(&self) {
        let cache = self.cache.clone();
        let cache_path = self.cache_path.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            // First try disk cache
            if let Some(ref path) = cache_path {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    if let Ok(entry) = serde_json::from_str::<CacheEntry>(&content) {
                        if Self::is_cache_valid(&entry) {
                            let mut guard = cache.write().await;
                            *guard = Some(entry);
                            tracing::debug!("Loaded models.dev from disk cache");
                            return;
                        }
                    }
                }
            }

            // Fetch from API
            match client.get(MODELS_API_URL).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(db) = resp.json::<ModelsDatabase>().await {
                        let entry = CacheEntry {
                            timestamp: Self::current_timestamp(),
                            data: db,
                        };

                        // Save to disk
                        if let Some(ref path) = cache_path {
                            if let Some(parent) = path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            if let Ok(content) = serde_json::to_string(&entry) {
                                let _ = tokio::fs::write(path, content).await;
                            }
                        }

                        // Store in memory
                        let mut guard = cache.write().await;
                        *guard = Some(entry);
                        tracing::debug!("Preloaded models.dev from API");
                    }
                }
                _ => {
                    tracing::warn!("Failed to preload models.dev");
                }
            }
        });
    }

    /// Normalize provider name to match models.dev keys (public wrapper)
    pub fn normalize_provider_name(&self, provider: &str) -> String {
        Self::normalize_provider(provider)
    }

    /// Normalize provider name to match models.dev keys
    fn normalize_provider(provider: &str) -> String {
        let lower = provider.to_lowercase();

        // Fast-path exact matches
        match lower.as_str() {
            "openai" | "gpt" => return "openai".to_string(),
            "anthropic" | "claude" => return "anthropic".to_string(),
            "google" | "gemini" => return "google".to_string(),
            "ollama" | "local" => return "ollama".to_string(),
            "tark_sim" => return "tark_sim".to_string(),
            "copilot" | "github" | "github-copilot" => return "github-copilot".to_string(),
            "openrouter" => return "openrouter".to_string(),
            "groq" => return "groq".to_string(),
            "together" | "togetherai" => return "together".to_string(),
            "fireworks" => return "fireworks".to_string(),
            "deepseek" => return "deepseek".to_string(),
            "mistral" => return "mistral".to_string(),
            "cohere" => return "cohere".to_string(),
            "perplexity" => return "perplexity".to_string(),
            "xai" | "grok" => return "xai".to_string(),
            _ => {}
        }

        // Plugin/provider aliases (best-effort). This enables models.dev lookups for
        // plugin provider IDs like "gemini-oauth" without requiring every caller
        // to special-case them.
        if lower.contains("gemini") {
            return "google".to_string();
        }
        if lower.contains("claude") {
            return "anthropic".to_string();
        }
        if lower.contains("openai") || lower.starts_with("gpt") {
            return "openai".to_string();
        }

        lower
    }

    /// Check if an Ollama model supports native tool calling
    ///
    /// Models known to support native Ollama tool calling API:
    /// - FunctionGemma (specialized for function calling)
    /// - Llama 3.1, Llama 4
    /// - Mistral Nemo
    /// - Firefunction v2
    /// - Command-R, Command-R+
    /// - Qwen 2.5, Qwen 3, Qwen 2.5 Coder
    /// - Devstral
    fn ollama_model_supports_tools(model_name: &str) -> bool {
        let lower = model_name.to_lowercase();

        // FunctionGemma - specialized for function calling
        if lower.contains("functiongemma") || lower.contains("function-gemma") {
            return true;
        }

        // Llama 3.1+ and Llama 4
        if lower.contains("llama3.1")
            || lower.contains("llama-3.1")
            || lower.contains("llama3:") && lower.contains("3.1")
            || lower.contains("llama4")
            || lower.contains("llama-4")
        {
            return true;
        }

        // Mistral Nemo
        if lower.contains("mistral-nemo") || lower.contains("mistral:nemo") {
            return true;
        }

        // Firefunction
        if lower.contains("firefunction") {
            return true;
        }

        // Command-R variants
        if lower.contains("command-r") || lower.contains("command:r") {
            return true;
        }

        // Qwen 2.5+ and Qwen 3
        if lower.contains("qwen2.5")
            || lower.contains("qwen-2.5")
            || lower.contains("qwen3")
            || lower.contains("qwen-3")
            || lower.contains("qwen:2.5")
            || lower.contains("qwen:3")
        {
            return true;
        }

        // Devstral
        if lower.contains("devstral") {
            return true;
        }

        // Default: no native tool calling
        false
    }

    /// Calculate cost for a request
    pub async fn calculate_cost(
        &self,
        provider: &str,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> f64 {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            model.calculate_cost(input_tokens, output_tokens)
        } else {
            // Fallback to hardcoded defaults
            Self::fallback_cost(provider, model_id, input_tokens, output_tokens)
        }
    }

    /// Fallback cost calculation when model not found
    fn fallback_cost(provider: &str, model_id: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        let (input_price, output_price) = match provider.to_lowercase().as_str() {
            "openai" | "gpt" => {
                if model_id.contains("gpt-4o-mini") {
                    (0.15, 0.60)
                } else if model_id.contains("gpt-4o") {
                    (2.5, 10.0)
                } else if model_id.contains("gpt-4") {
                    (30.0, 60.0)
                } else if model_id.contains("o1") {
                    (15.0, 60.0)
                } else {
                    (0.5, 1.5)
                }
            }
            "anthropic" | "claude" => {
                if model_id.contains("haiku") {
                    (0.25, 1.25)
                } else if model_id.contains("sonnet") {
                    (3.0, 15.0)
                } else if model_id.contains("opus") {
                    (15.0, 75.0)
                } else {
                    (3.0, 15.0)
                }
            }
            "ollama" | "local" | "tark_sim" => (0.0, 0.0),
            _ => (0.0, 0.0),
        };

        let input_cost = (input_tokens as f64) * input_price / 1_000_000.0;
        let output_cost = (output_tokens as f64) * output_price / 1_000_000.0;
        input_cost + output_cost
    }

    /// Get context window limit for a model
    pub async fn get_context_limit(&self, provider: &str, model_id: &str) -> u32 {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            if model.limit.context > 0 {
                return model.limit.context;
            }
        }

        // Fallback defaults
        Self::fallback_context_limit(provider, model_id)
    }

    /// Fallback context limit when model not found
    fn fallback_context_limit(provider: &str, model_id: &str) -> u32 {
        match provider.to_lowercase().as_str() {
            "openai" | "gpt" => {
                if model_id.contains("gpt-4o") || model_id.contains("gpt-4-turbo") {
                    128_000
                } else if model_id.contains("gpt-4") {
                    8_192
                } else {
                    16_385
                }
            }
            "anthropic" | "claude" => 200_000,
            "ollama" | "local" => 32_000,
            "tark_sim" => 8_192,
            _ => 128_000,
        }
    }

    /// Check if a model supports tool calling
    pub async fn supports_tools(&self, provider: &str, model_id: &str) -> bool {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            return model.tool_call;
        }
        // Assume true for major providers except ollama/local
        let provider = provider.to_lowercase();
        if provider == "tark_sim" {
            return true;
        }
        !matches!(provider.as_str(), "ollama" | "local")
    }

    /// Check if a model supports reasoning/thinking mode
    pub async fn supports_reasoning(&self, provider: &str, model_id: &str) -> bool {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            return model.reasoning;
        }
        // Check known reasoning models
        model_id.contains("o1")
            || model_id.contains("o3")
            || model_id.contains("deepseek-r1")
            || model_id.contains("thinking")
            || (provider == "tark_sim" && model_id == "tark_llm")
    }

    /// Check if a model supports vision
    pub async fn supports_vision(&self, provider: &str, model_id: &str) -> bool {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            return model.supports_vision();
        }
        // Fallback check
        model_id.contains("vision")
            || model_id.contains("4o")
            || model_id.contains("claude-3")
            || model_id.contains("gemini")
            || model_id.contains("llava")
    }

    /// Get model capabilities as a struct
    pub async fn get_capabilities(&self, provider: &str, model_id: &str) -> ModelCapabilities {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            ModelCapabilities {
                tool_call: model.tool_call,
                reasoning: model.reasoning,
                vision: model.supports_vision(),
                audio_input: model.supports_audio_input(),
                video_input: model.supports_video_input(),
                pdf: model.supports_pdf(),
                image_output: model.supports_image_output(),
                audio_output: model.supports_audio_output(),
                structured_output: model.structured_output.unwrap_or(false),
                temperature: model.temperature,
                context_limit: model.limit.context,
                output_limit: model.limit.output,
                input_cost: model.cost.input,
                output_cost: model.cost.output,
                supports_caching: model.cost.cache_read.is_some(),
                reasoning_cost: model.cost.reasoning_cost_per_million(),
            }
        } else {
            // Return defaults
            ModelCapabilities {
                tool_call: true,
                reasoning: false,
                vision: false,
                audio_input: false,
                video_input: false,
                pdf: false,
                image_output: false,
                audio_output: false,
                structured_output: false,
                temperature: true,
                context_limit: Self::fallback_context_limit(provider, model_id),
                output_limit: 4096,
                input_cost: 0.0,
                output_cost: 0.0,
                supports_caching: false,
                reasoning_cost: 0.0,
            }
        }
    }

    /// Get smart thinking defaults for a specific model from models.dev
    pub async fn get_thinking_defaults(
        &self,
        provider: &str,
        model_id: &str,
    ) -> ModelThinkingDefaults {
        if let Ok(Some(model)) = self.get_model(provider, model_id).await {
            if !model.reasoning {
                return ModelThinkingDefaults::default();
            }

            // Calculate suggested budget: 1/4 of output limit, min 8K, max 50K
            let suggested_budget = (model.limit.output / 4).clamp(8192, 50000);

            // Get thinking cost (falls back to output cost)
            let cost_per_million = model.cost.reasoning.unwrap_or(model.cost.output);
            let cost_per_1k = cost_per_million / 1000.0;

            // Determine param type based on provider
            let param_type = match Self::normalize_provider(provider).as_str() {
                "anthropic" => ThinkingParamType::BudgetTokens,
                "openai" => ThinkingParamType::ReasoningEffort,
                "google" => ThinkingParamType::ThinkingBudget,
                _ => ThinkingParamType::BudgetTokens,
            };

            ModelThinkingDefaults {
                supported: true,
                suggested_budget,
                cost_per_1k,
                param_type,
            }
        } else {
            Self::fallback_thinking_defaults(provider, model_id)
        }
    }

    /// Fallback thinking defaults for unknown models
    fn fallback_thinking_defaults(provider: &str, model_id: &str) -> ModelThinkingDefaults {
        // Check known reasoning models by name pattern
        let is_reasoning = model_id.contains("o1")
            || model_id.contains("o3")
            || model_id.contains("thinking")
            || model_id.contains("sonnet-4")
            || model_id.contains("3-7-sonnet")
            || model_id.contains("deepseek-r1")
            || (provider == "tark_sim" && model_id == "tark_llm");

        if !is_reasoning {
            return ModelThinkingDefaults::default();
        }

        match provider.to_lowercase().as_str() {
            "openai" | "gpt" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 0, // Uses effort level, not tokens
                cost_per_1k: 0.06,   // ~$60/M for o1
                param_type: ThinkingParamType::ReasoningEffort,
            },
            "anthropic" | "claude" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 10_000,
                cost_per_1k: 0.015, // ~$15/M
                param_type: ThinkingParamType::BudgetTokens,
            },
            "google" | "gemini" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 8_192,
                cost_per_1k: 0.0, // Included in output
                param_type: ThinkingParamType::ThinkingBudget,
            },
            "tark_sim" => ModelThinkingDefaults {
                supported: true,
                suggested_budget: 2048,
                cost_per_1k: 0.0,
                param_type: ThinkingParamType::BudgetTokens,
            },
            _ => ModelThinkingDefaults::default(),
        }
    }
}

/// Model capabilities summary
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub tool_call: bool,
    pub reasoning: bool,
    pub vision: bool,
    pub audio_input: bool,
    pub video_input: bool,
    pub pdf: bool,
    pub image_output: bool,
    pub audio_output: bool,
    pub structured_output: bool,
    pub temperature: bool,
    pub context_limit: u32,
    pub output_limit: u32,
    pub input_cost: f64,
    pub output_cost: f64,
    pub supports_caching: bool,
    /// Reasoning/thinking token cost per million
    pub reasoning_cost: f64,
}

impl ModelCapabilities {
    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        let mut caps = Vec::new();

        if self.tool_call {
            caps.push("tools");
        }
        if self.reasoning {
            caps.push("reasoning");
        }
        if self.vision {
            caps.push("vision");
        }
        if self.audio_input {
            caps.push("audio");
        }
        if self.pdf {
            caps.push("pdf");
        }
        if self.structured_output {
            caps.push("structured");
        }

        if caps.is_empty() {
            "text-only".to_string()
        } else {
            caps.join(", ")
        }
    }

    /// Format cost as string
    pub fn cost_string(&self) -> String {
        if self.input_cost == 0.0 && self.output_cost == 0.0 {
            "free".to_string()
        } else {
            format!(
                "${:.2}/${:.2} per 1M tokens",
                self.input_cost, self.output_cost
            )
        }
    }

    /// Format context limit as string
    pub fn context_string(&self) -> String {
        if self.context_limit >= 1_000_000 {
            format!("{}M", self.context_limit / 1_000_000)
        } else if self.context_limit >= 1_000 {
            format!("{}K", self.context_limit / 1_000)
        } else {
            format!("{}", self.context_limit)
        }
    }

    /// Estimate thinking cost for given token budget
    pub fn estimate_thinking_cost(&self, budget_tokens: u32) -> f64 {
        let cost_per_million = if self.reasoning_cost > 0.0 {
            self.reasoning_cost
        } else {
            self.output_cost
        };
        (budget_tokens as f64) * cost_per_million / 1_000_000.0
    }
}

/// Provider-specific thinking parameter type
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ThinkingParamType {
    #[default]
    None,
    /// Claude: thinking.budget_tokens
    BudgetTokens,
    /// OpenAI o1/o3: reasoning_effort (low/medium/high)
    ReasoningEffort,
    /// Gemini: thinkingConfig.thinkingBudget
    ThinkingBudget,
}

/// Thinking configuration defaults for a specific model
#[derive(Debug, Clone, Default)]
pub struct ModelThinkingDefaults {
    /// Whether this model supports thinking
    pub supported: bool,
    /// Suggested token budget (based on model's output limit)
    pub suggested_budget: u32,
    /// Cost per 1K thinking tokens
    pub cost_per_1k: f64,
    /// Provider-specific param type
    pub param_type: ThinkingParamType,
}

/// Global singleton for the models database manager
static MODELS_DB: std::sync::OnceLock<ModelsDbManager> = std::sync::OnceLock::new();

/// Get the global models database manager
pub fn models_db() -> &'static ModelsDbManager {
    MODELS_DB.get_or_init(ModelsDbManager::new)
}

/// Initialize the global models database with a cache directory
pub fn init_models_db(cache_dir: PathBuf) {
    let _ = MODELS_DB.set(ModelsDbManager::new().with_cache_dir(cache_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_cost_calculation() {
        let model = ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            family: Some("gpt-4o".to_string()),
            attachment: true,
            reasoning: false,
            tool_call: true,
            temperature: true,
            structured_output: Some(true),
            knowledge: Some("2024-04".to_string()),
            release_date: Some("2024-05-13".to_string()),
            last_updated: None,
            modalities: ModelModalities {
                input: vec!["text".to_string(), "image".to_string()],
                output: vec!["text".to_string()],
            },
            open_weights: false,
            cost: ModelCost {
                input: 2.5,
                output: 10.0,
                cache_read: Some(1.25),
                cache_write: Some(2.5),
                reasoning: None,
            },
            limit: ModelLimits {
                context: 128000,
                output: 16384,
            },
        };

        // 1000 input tokens, 500 output tokens
        let cost = model.calculate_cost(1000, 500);
        // Expected: (1000 * 2.5 + 500 * 10.0) / 1_000_000 = 0.0075
        assert!((cost - 0.0075).abs() < 0.0001);
    }

    #[test]
    fn test_model_capabilities() {
        let model = ModelInfo {
            id: "claude-3-sonnet".to_string(),
            name: "Claude 3 Sonnet".to_string(),
            family: Some("claude-sonnet".to_string()),
            attachment: true,
            reasoning: true,
            tool_call: true,
            temperature: true,
            structured_output: None,
            knowledge: None,
            release_date: None,
            last_updated: None,
            modalities: ModelModalities {
                input: vec!["text".to_string(), "image".to_string(), "pdf".to_string()],
                output: vec!["text".to_string()],
            },
            open_weights: false,
            cost: ModelCost::default(),
            limit: ModelLimits::default(),
        };

        assert!(model.supports_vision());
        assert!(model.supports_pdf());
        assert!(!model.supports_audio_input());
        assert!(!model.supports_image_output());
    }

    #[test]
    fn test_normalize_provider() {
        assert_eq!(ModelsDbManager::normalize_provider("openai"), "openai");
        assert_eq!(ModelsDbManager::normalize_provider("gpt"), "openai");
        assert_eq!(ModelsDbManager::normalize_provider("claude"), "anthropic");
        assert_eq!(
            ModelsDbManager::normalize_provider("ANTHROPIC"),
            "anthropic"
        );
        assert_eq!(ModelsDbManager::normalize_provider("ollama"), "ollama");
        assert_eq!(ModelsDbManager::normalize_provider("local"), "ollama");
        assert_eq!(
            ModelsDbManager::normalize_provider("copilot"),
            "github-copilot"
        );
        assert_eq!(
            ModelsDbManager::normalize_provider("github"),
            "github-copilot"
        );
        assert_eq!(ModelsDbManager::normalize_provider("gemini"), "google");
        assert_eq!(ModelsDbManager::normalize_provider("google"), "google");
        assert_eq!(
            ModelsDbManager::normalize_provider("openrouter"),
            "openrouter"
        );
    }

    #[test]
    fn test_fallback_cost() {
        // GPT-4o
        let cost = ModelsDbManager::fallback_cost("openai", "gpt-4o", 1000, 500);
        assert!(cost > 0.0);

        // Ollama (free)
        let cost = ModelsDbManager::fallback_cost("ollama", "llama3", 1000, 500);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_capability_summary() {
        let caps = ModelCapabilities {
            tool_call: true,
            reasoning: true,
            vision: true,
            audio_input: false,
            video_input: false,
            pdf: true,
            image_output: false,
            audio_output: false,
            structured_output: false,
            temperature: true,
            context_limit: 200000,
            output_limit: 8192,
            input_cost: 3.0,
            output_cost: 15.0,
            supports_caching: true,
            reasoning_cost: 15.0,
        };

        let summary = caps.summary();
        assert!(summary.contains("tools"));
        assert!(summary.contains("reasoning"));
        assert!(summary.contains("vision"));
        assert!(summary.contains("pdf"));
    }

    #[test]
    fn test_normalize_provider_plugin_aliases() {
        // Plugin IDs should still resolve to canonical models.dev provider keys
        // so the rest of the app doesn't need special-casing.
        assert_eq!(
            ModelsDbManager::normalize_provider("gemini-oauth"),
            "google"
        );
        assert_eq!(
            ModelsDbManager::normalize_provider("claude-oauth"),
            "anthropic"
        );
        assert_eq!(ModelsDbManager::normalize_provider("openai-sso"), "openai");
    }
}
