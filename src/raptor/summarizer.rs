use crate::agent::orchestrator::DualModelOrchestrator;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// Node in the recursive summary tree
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SummaryNode {
    pub id: String,
    pub summary: String,
    pub children: Vec<String>, // child node ids or chunk ids
    pub is_chunk: bool,
}

impl SummaryNode {
    pub fn new(summary: String, children: Vec<String>, is_chunk: bool) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            summary,
            children,
            is_chunk,
        }
    }
}

/// Summarizer wrapping a local model call. Uses `DualModelOrchestrator` for local LLM access.
/// Uses Arc<Mutex<...>> so it can be shared safely across async calls without borrowing issues.
pub struct RecursiveSummarizer {
    pub orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
    pub max_chars: usize,
}

impl RecursiveSummarizer {
    pub fn new(orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>, max_chars: usize) -> Self {
        Self { orchestrator, max_chars }
    }

    /// Ask the model to summarize a list of texts into a short abstract.
    /// We construct a concise prompt and rely on local model.
    pub async fn summarize_group(&self, texts: &[String]) -> Result<String> {
        if texts.is_empty() {
            return Ok(String::from("Empty group"));
        }

        // Construct prompt with size limits to avoid OOM in local models
        let mut prompt = String::from("Resume estos fragmentos en máximo 3 frases concisas y claras:\n\n");
        let mut included = 0;
        for t in texts {
            // ensure we don't exceed max_chars
            if prompt.len() + t.len() > self.max_chars {
                break;
            }
            prompt.push_str(&format!("- {}\n", t));
            included += 1;
        }
        
        if included == 0 {
            return Ok(texts[0].chars().take(200).collect());
        }
        
        prompt.push_str("\nResumen:");

        // Call the heavy model via orchestrator (lock before awaiting)
        let orch = self.orchestrator.lock().await;
        let resp = orch.call_heavy_model_direct(&prompt).await?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn summary_node_basic() {
        let node = SummaryNode::new("sum".to_string(), vec!["a".to_string()], false);
        assert!(!node.id.is_empty());
    }
}
