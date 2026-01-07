//! RAPTOR Tool - Herramienta para construir y consultar árboles RAPTOR
//! Integrada con el sistema de tools del agente

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex as AsyncMutex;

use crate::agent::orchestrator::DualModelOrchestrator;
use crate::embedding::EmbeddingEngine;
use crate::raptor::builder::{build_tree_with_progress, RaptorBuildProgress};
use crate::raptor::persistence::GLOBAL_STORE;
use crate::raptor::retriever::TreeRetriever;

/// Arguments for building a RAPTOR tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTreeArgs {
    /// Path to directory or file to index
    pub path: String,
    /// Maximum characters per chunk
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
    /// Overlap between chunks
    #[serde(default = "default_overlap")]
    pub overlap: usize,
    /// Similarity threshold for clustering (0.0-1.0)
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_max_chars() -> usize {
    500
}
fn default_overlap() -> usize {
    50
}
fn default_threshold() -> f32 {
    0.7
}

/// Arguments for querying RAPTOR tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTreeArgs {
    /// Query text
    pub query: String,
    /// Number of top summaries to retrieve
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Number of chunks to expand to
    #[serde(default = "default_expand_k")]
    pub expand_k: usize,
    /// Threshold for chunk fallback
    #[serde(default = "default_chunk_threshold")]
    pub chunk_threshold: f32,
}

fn default_top_k() -> usize {
    5
}
fn default_expand_k() -> usize {
    10
}
fn default_chunk_threshold() -> f32 {
    0.85
}

/// RAPTOR Tool for building and querying hierarchical document trees
pub struct RaptorTool {
    orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
}

impl RaptorTool {
    pub fn new(orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>) -> Self {
        Self { orchestrator }
    }

    /// Build RAPTOR tree from directory
    pub async fn build_tree(&self, args: BuildTreeArgs) -> Result<String> {
        self.build_tree_with_progress(args, None).await
    }

    /// Build RAPTOR tree from directory with progress updates
    pub async fn build_tree_with_progress(
        &self,
        args: BuildTreeArgs,
        progress_tx: Option<Sender<RaptorBuildProgress>>,
    ) -> Result<String> {
        let path = PathBuf::from(&args.path);

        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", args.path);
        }

        let root_id = build_tree_with_progress(
            &path,
            self.orchestrator.clone(),
            args.max_chars,
            args.overlap,
            args.threshold,
            progress_tx,
        )
        .await?;

        let store = GLOBAL_STORE.lock().unwrap();
        let chunk_count = store.chunk_map.len();
        let node_count = store.nodes.len();

        Ok(format!(
            "✅ Árbol RAPTOR construido exitosamente\n\
             📂 Path: {}\n\
             🌳 Root ID: {}\n\
             📝 Chunks: {}\n\
             🔷 Nodos de resumen: {}\n\
             💾 Contexto almacenado en memoria para consultas",
            args.path, root_id, chunk_count, node_count
        ))
    }

    /// Query RAPTOR tree
    pub async fn query_tree(&self, args: QueryTreeArgs) -> Result<String> {
        let embedder = EmbeddingEngine::new().await?;

        // Check if tree exists (release lock immediately)
        {
            let store_guard = GLOBAL_STORE.lock().unwrap();
            if store_guard.chunk_map.is_empty() {
                return Ok(
                    "⚠️ No hay árbol RAPTOR construido. Usa 'build_raptor_tree' primero"
                        .to_string(),
                );
            }
        }

        // Perform retrieval - the retriever internally handles locks properly
        // by using index-based approach that doesn't hold locks during awaits
        let (summaries, chunks) = {
            // Clone the store to avoid holding lock during async operations
            // Alternative: modify TreeRetriever to not require store reference
            let store_clone = {
                let guard = GLOBAL_STORE.lock().unwrap();
                guard.clone()
            };

            let retriever = TreeRetriever::new(&embedder, &store_clone);
            retriever
                .retrieve_with_context(&args.query, args.top_k, args.expand_k, args.chunk_threshold)
                .await?
        };

        let mut result = format!("🔍 Resultados RAPTOR para: \"{}\"\n\n", args.query);

        // Format summaries
        if !summaries.is_empty() {
            result.push_str("📊 Resúmenes de alto nivel:\n");
            for (i, (id, score, summary)) in summaries.iter().enumerate() {
                result.push_str(&format!(
                    "{}. [Score: {:.3}] {}\n   ID: {}\n\n",
                    i + 1,
                    score,
                    summary.chars().take(200).collect::<String>(),
                    id
                ));
            }
        }

        // Format chunks
        if !chunks.is_empty() {
            result.push_str("📄 Fragmentos detallados:\n");
            for (i, (id, score, text)) in chunks.iter().enumerate() {
                result.push_str(&format!(
                    "{}. [Score: {:.3}] {}\n   ID: {}\n\n",
                    i + 1,
                    score,
                    text.chars().take(300).collect::<String>(),
                    id
                ));
            }
        }

        if summaries.is_empty() && chunks.is_empty() {
            result.push_str("❌ No se encontraron resultados relevantes.");
        }

        Ok(result)
    }

    /// Get statistics about current RAPTOR tree
    pub async fn get_tree_stats(&self) -> Result<String> {
        let store = GLOBAL_STORE.lock().unwrap();

        let chunk_count = store.chunk_map.len();
        let node_count = store.nodes.len();
        let has_embeddings =
            !store.summary_embeddings.is_empty() || !store.chunk_embeddings.is_empty();

        let mut result = String::from("📊 Estadísticas del Árbol RAPTOR\n\n");
        result.push_str(&format!("📝 Chunks almacenados: {}\n", chunk_count));
        result.push_str(&format!("🔷 Nodos de resumen: {}\n", node_count));
        result.push_str(&format!(
            "🧮 Embeddings: {}\n",
            if has_embeddings {
                "✅ Generados"
            } else {
                "❌ No disponibles"
            }
        ));

        if chunk_count == 0 {
            result.push_str("\n⚠️ No hay árbol construido. Usa 'build_raptor_tree' primero.");
        }

        Ok(result)
    }

    /// Clear RAPTOR tree from memory
    pub async fn clear_tree(&self) -> Result<String> {
        let mut store = GLOBAL_STORE.lock().unwrap();

        let chunk_count = store.chunk_map.len();
        let node_count = store.nodes.len();

        // Use the new clear method to properly free memory
        store.clear();

        Ok(format!(
            "🗑️ Árbol RAPTOR limpiado\n\
             Removidos: {} chunks, {} nodos",
            chunk_count, node_count
        ))
    }
}

/// Tool trait implementation for registry integration
#[async_trait]
pub trait RaptorToolCalls {
    async fn build_raptor_tree(
        &self,
        path: &str,
        max_chars: Option<usize>,
        threshold: Option<f32>,
    ) -> Result<String>;
    async fn query_raptor_tree(&self, query: &str, top_k: Option<usize>) -> Result<String>;
    async fn raptor_stats(&self) -> Result<String>;
    async fn clear_raptor(&self) -> Result<String>;
}

#[async_trait]
impl RaptorToolCalls for RaptorTool {
    async fn build_raptor_tree(
        &self,
        path: &str,
        max_chars: Option<usize>,
        threshold: Option<f32>,
    ) -> Result<String> {
        let args = BuildTreeArgs {
            path: path.to_string(),
            max_chars: max_chars.unwrap_or(500),
            overlap: 50,
            threshold: threshold.unwrap_or(0.7),
        };
        self.build_tree(args).await
    }

    async fn query_raptor_tree(&self, query: &str, top_k: Option<usize>) -> Result<String> {
        let args = QueryTreeArgs {
            query: query.to_string(),
            top_k: top_k.unwrap_or(5),
            expand_k: 10,
            chunk_threshold: 0.85,
        };
        self.query_tree(args).await
    }

    async fn raptor_stats(&self) -> Result<String> {
        self.get_tree_stats().await
    }

    async fn clear_raptor(&self) -> Result<String> {
        self.clear_tree().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore] // Heavy test: loads embedding model and builds full RAPTOR tree. Run with: cargo test -- --ignored
    async fn test_raptor_tool_workflow() {
        // Create test files
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("test1.txt");
        let mut f1 = File::create(&file1).unwrap();
        write!(
            f1,
            "El gato negro se sentó en la alfombra roja del salón principal.\n\
                     Los muebles antiguos decoraban toda la habitación con elegancia."
        )
        .unwrap();

        // Initialize tool
        let config = crate::agent::orchestrator::OrchestratorConfig::default();
        let orch = DualModelOrchestrator::with_config(config).await.unwrap();
        let tool = RaptorTool::new(Arc::new(AsyncMutex::new(orch)));

        // Clear any previous state
        let _ = tool.clear_tree().await;

        // Build tree
        let result = tool
            .build_raptor_tree(dir.path().to_str().unwrap(), Some(200), Some(0.7))
            .await
            .unwrap();

        assert!(result.contains("construido exitosamente"));

        // Get stats
        let stats = tool.raptor_stats().await.unwrap();
        assert!(stats.contains("Chunks almacenados"));

        // Query tree
        let query_result = tool
            .query_raptor_tree("gatos y muebles", Some(3))
            .await
            .unwrap();
        assert!(query_result.contains("Resultados RAPTOR"));

        // Clear
        let clear_result = tool.clear_tree().await.unwrap();
        assert!(clear_result.contains("limpiado"));
    }
}
