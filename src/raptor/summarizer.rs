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
        Self {
            orchestrator,
            max_chars,
        }
    }

    /// Ask the model to summarize a list of texts into a short abstract.
    /// Uses fast model for speed - summaries don't need heavy reasoning.
    pub async fn summarize_group(&self, texts: &[String]) -> Result<String> {
        if texts.is_empty() {
            return Ok(String::from("Empty group"));
        }

        // For small groups, just concatenate first lines
        if texts.len() <= 2 {
            let combined: String = texts.iter()
                .map(|t| t.lines().next().unwrap_or("").chars().take(100).collect::<String>())
                .collect::<Vec<_>>()
                .join(" | ");
            return Ok(combined);
        }

        // Construct concise prompt
        let mut prompt = String::from("/no_think Resume en 1-2 frases:\n");
        let mut included = 0;
        for t in texts {
            if prompt.len() + t.len() > self.max_chars {
                break;
            }
            // Only take first 150 chars of each text
            let short: String = t.chars().take(150).collect();
            prompt.push_str(&format!("- {}\n", short));
            included += 1;
        }

        if included == 0 {
            return Ok(texts[0].chars().take(200).collect());
        }

        // Use fast model for summaries (much faster)
        let orch = self.orchestrator.lock().await;
        let resp = orch.call_fast_model_direct(&prompt).await?;
        // Limit response length
        Ok(resp.chars().take(300).collect())
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
