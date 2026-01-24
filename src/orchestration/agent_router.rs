//! Agent Router - Classifies and routes requests to the best agent

use crate::llm::LlmProvider;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

pub struct RoutingRule {
    pub pattern: Regex,
    pub provider: String,
    #[allow(dead_code)]
    pub confidence: f64,
}

pub struct AgentRouter {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    routing_rules: Vec<RoutingRule>,
}

impl AgentRouter {
    pub fn new(providers: HashMap<String, Arc<dyn LlmProvider>>) -> Self {
        Self {
            providers,
            routing_rules: Vec::new(),
        }
    }

    pub fn add_rule(&mut self, pattern: &str, provider: &str) -> Result<()> {
        let regex = Regex::new(pattern)?;
        self.routing_rules.push(RoutingRule {
            pattern: regex,
            provider: provider.to_string(),
            confidence: 1.0,
        });
        Ok(())
    }

    pub fn route(&self, message: &str) -> Option<Arc<dyn LlmProvider>> {
        for rule in &self.routing_rules {
            if rule.pattern.is_match(message) {
                if let Some(provider) = self.providers.get(&rule.provider) {
                    return Some(provider.clone());
                }
            }
        }
        
        // Default to first provider if available
        self.providers.get("default").or_else(|| self.providers.values().next()).cloned()
    }
}
