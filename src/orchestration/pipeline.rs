//! Agent Pipeline - Sequential agent chaining

use crate::llm::{LlmProvider, Message};
use anyhow::Result;
use std::sync::Arc;

pub struct PipelineStage {
    pub name: String,
    pub agent: Arc<dyn LlmProvider>,
    pub prompt_template: String,
}

#[derive(Default)]
pub struct AgentPipeline {
    stages: Vec<PipelineStage>,
}

impl AgentPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_stage(&mut self, name: &str, agent: Arc<dyn LlmProvider>, prompt_template: &str) {
        self.stages.push(PipelineStage {
            name: name.to_string(),
            agent,
            prompt_template: prompt_template.to_string(),
        });
    }

    pub async fn execute(&self, initial_input: &str) -> Result<String> {
        let mut context = initial_input.to_string();
        
        for stage in &self.stages {
            let prompt = stage.prompt_template.replace("{input}", &context);
            let response = stage.agent.chat(&[Message::user(&prompt)], None).await?;
            context = response.text().unwrap_or_default().to_string();
            
            tracing::info!("Stage '{}' completed", stage.name);
        }
        
        Ok(context)
    }
}
