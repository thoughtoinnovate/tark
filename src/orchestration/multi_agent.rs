//! Multi-Agent Coordinator - Manages parallel execution and consensus

use crate::llm::{LlmProvider, Message, LlmResponse};
use anyhow::Result;
use std::sync::Arc;

pub struct MultiAgentCoordinator {
    agents: Vec<Arc<dyn LlmProvider>>,
}

impl MultiAgentCoordinator {
    pub fn new(agents: Vec<Arc<dyn LlmProvider>>) -> Self {
        Self { agents }
    }

    pub async fn consensus(&self, prompt: &str) -> Result<String> {
        if self.agents.is_empty() {
            anyhow::bail!("No agents available for consensus");
        }

        let messages = vec![Message::user(prompt)];
        
        // Get responses from all agents in parallel
        let mut futures = Vec::new();
        for agent in &self.agents {
            futures.push(agent.chat(&messages, None));
        }
        
        let responses = futures::future::join_all(futures).await;
        
        // Aggregate responses
        let mut combined = String::new();
        for (i, resp) in responses.into_iter().enumerate() {
            if let Ok(LlmResponse::Text { text, .. }) = resp {
                if !combined.is_empty() {
                    combined.push_str("\n\n---\n\n");
                }
                combined.push_str(&format!("Agent {}:\n{}", i + 1, text));
            }
        }
        
        // Use primary agent to synthesize
        let synthesis_prompt = format!(
            "Multiple AI assistants provided these responses:\n\n{}\n\n\
             Synthesize these into a single coherent answer:",
            combined
        );
        
        let final_response = self.agents[0]
            .chat(&[Message::user(&synthesis_prompt)], None)
            .await?;
        
        Ok(final_response.text().unwrap_or_default().to_string())
    }
}
