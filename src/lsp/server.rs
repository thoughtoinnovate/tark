//! Main LSP server implementation

use super::document::DocumentStore;
use super::{code_action, completion, diagnostics, hover};
use crate::completion::CompletionEngine;
use crate::config::Config;
use crate::diagnostics::DiagnosticsEngine;
use crate::llm::{self, LlmProvider};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// The LSP backend
pub struct EngLspBackend {
    client: Client,
    documents: Arc<DocumentStore>,
    completion_engine: Arc<CompletionEngine>,
    diagnostics_engine: Arc<DiagnosticsEngine>,
    llm: Arc<dyn LlmProvider>,
    config: Arc<Config>,
    // Debounce timer for diagnostics
    diagnostics_pending: Arc<Mutex<Option<Url>>>,
}

impl EngLspBackend {
    pub fn new(client: Client, llm: Arc<dyn LlmProvider>, config: Config) -> Self {
        let completion_engine = CompletionEngine::new(llm.clone())
            .with_cache_size(config.completion.cache_size)
            .with_context_lines(
                config.completion.context_lines_before,
                config.completion.context_lines_after,
            );

        let diagnostics_engine =
            DiagnosticsEngine::new(llm.clone()).with_debounce(config.completion.debounce_ms);

        Self {
            client,
            documents: Arc::new(DocumentStore::new()),
            completion_engine: Arc::new(completion_engine),
            diagnostics_engine: Arc::new(diagnostics_engine),
            llm,
            config: Arc::new(config),
            diagnostics_pending: Arc::new(Mutex::new(None)),
        }
    }

    async fn run_diagnostics_for_uri(&self, uri: Url) {
        let debounce_ms = self.config.completion.debounce_ms;
        let client = self.client.clone();
        let documents = self.documents.clone();
        let diagnostics_engine = self.diagnostics_engine.clone();
        let diagnostics_pending = self.diagnostics_pending.clone();

        // Store the pending URI
        {
            let mut pending = diagnostics_pending.lock().await;
            *pending = Some(uri.clone());
        }

        // Wait for debounce period
        tokio::time::sleep(Duration::from_millis(debounce_ms)).await;

        // Check if this URI is still pending
        let should_run = {
            let pending = diagnostics_pending.lock().await;
            pending.as_ref() == Some(&uri)
        };

        if !should_run {
            return;
        }

        // Get document
        let doc = match documents.get(&uri) {
            Some(d) => d,
            None => return,
        };

        // Run diagnostics
        match diagnostics::run_diagnostics(&diagnostics_engine, &doc).await {
            Ok(diags) => {
                client.publish_diagnostics(uri.clone(), diags, None).await;
            }
            Err(e) => {
                tracing::error!("Failed to run diagnostics: {}", e);
            }
        }

        // Clear pending
        {
            let mut pending = diagnostics_pending.lock().await;
            if pending.as_ref() == Some(&uri) {
                *pending = None;
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for EngLspBackend {
    async fn initialize(&self, _: InitializeParams) -> JsonRpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        "(".to_string(),
                        " ".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "tark".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("LSP server initialized");
        self.client
            .log_message(MessageType::INFO, "tark LSP server ready")
            .await;
    }

    async fn shutdown(&self) -> JsonRpcResult<()> {
        tracing::info!("LSP server shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.documents.open(params);

        // Run diagnostics in background
        let backend = self.clone();
        tokio::spawn(async move {
            backend.run_diagnostics_for_uri(uri).await;
        });
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.documents.change(params);

        // Run diagnostics (debounced) in background
        let backend = self.clone();
        tokio::spawn(async move {
            backend.run_diagnostics_for_uri(uri).await;
        });
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Clear diagnostics
        self.client
            .publish_diagnostics(params.text_document.uri.clone(), vec![], None)
            .await;
        self.documents.close(params);
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> JsonRpcResult<Option<CompletionResponse>> {
        match completion::handle_completion(&self.completion_engine, &self.documents, params).await
        {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::error!("Completion error: {}", e);
                Ok(None)
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> JsonRpcResult<Option<Hover>> {
        match hover::handle_hover(self.llm.clone(), &self.documents, params).await {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::error!("Hover error: {}", e);
                Ok(None)
            }
        }
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonRpcResult<Option<CodeActionResponse>> {
        match code_action::handle_code_action(self.llm.clone(), &self.documents, params).await {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::error!("Code action error: {}", e);
                Ok(None)
            }
        }
    }
}

impl Clone for EngLspBackend {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            documents: self.documents.clone(),
            completion_engine: self.completion_engine.clone(),
            diagnostics_engine: self.diagnostics_engine.clone(),
            llm: self.llm.clone(),
            config: self.config.clone(),
            diagnostics_pending: self.diagnostics_pending.clone(),
        }
    }
}

/// Run the LSP server on stdio
pub async fn run_lsp_server() -> Result<()> {
    let config = Config::load().unwrap_or_default();

    // Create LLM provider
    let provider: Arc<dyn LlmProvider> =
        Arc::from(llm::create_provider(&config.llm.default_provider)?);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| EngLspBackend::new(client, provider, config));

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
