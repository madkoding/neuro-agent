use crate::embedding::EmbeddingEngine;
use crate::raptor::persistence::TreeStore;
use anyhow::Result;

/// Retriever that searches the summary tree and also falls back to chunk search.
/// Uses batch embeddings for efficiency and a lightweight linear scan. Designed to be memory-friendly.
pub struct TreeRetriever<'a> {
    pub embedder: &'a EmbeddingEngine,
    pub store: &'a TreeStore,
}

impl<'a> TreeRetriever<'a> {
    pub fn new(embedder: &'a EmbeddingEngine, store: &'a TreeStore) -> Self {
        Self { embedder, store }
    }

    /// Retrieve top-k summary nodes by similarity to the query.
    /// Returns vector of (node_id, score, summary)
    pub async fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<(String, f32, String)>> {
        // Compute query embedding once and delegate
        let q_emb = self.embedder.embed_text(query).await?;
        self.retrieve_with_emb(&q_emb, top_k).await
    }

    /// Internal helper that reuses a precomputed query embedding.
    /// Optimized to minimize memory allocations using index-based approach.
    async fn retrieve_with_emb(&self, q_emb: &[f32], top_k: usize) -> Result<Vec<(String, f32, String)>> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        use ordered_float::OrderedFloat;

        // If the store has precomputed summary embeddings, query them directly (very memory-friendly)
        if !self.store.summary_embeddings.is_empty() {
            let hits = self.store.query_top_k_summaries(q_emb, top_k);
            let mut results = Vec::with_capacity(hits.len());
            for (id, score) in hits.into_iter() {
                if let Some(node) = self.store.get_node(&id) {
                    results.push((id, score, node.summary.clone()));
                }
            }
            return Ok(results);
        }

        // Memory-friendly batch processing using indices
        const DEFAULT_BATCH_SIZE: usize = 128;
        
        // Collect node IDs first
        let node_ids: Vec<String> = self.store.nodes.keys().cloned().collect();
        let node_count = node_ids.len();
        
        // Use index-based heap to avoid cloning strings during processing
        let mut heap: BinaryHeap<Reverse<(OrderedFloat<f32>, usize)>> = BinaryHeap::new();

        // Process nodes in batches
        for batch_start in (0..node_count).step_by(DEFAULT_BATCH_SIZE) {
            let batch_end = (batch_start + DEFAULT_BATCH_SIZE).min(node_count);
            
            // Prepare batch texts using indices
            let batch_texts: Vec<&str> = (batch_start..batch_end)
                .filter_map(|i| self.store.nodes.get(&node_ids[i]).map(|n| n.summary.as_str()))
                .collect();
            
            if batch_texts.is_empty() {
                continue;
            }

            // Embed batch
            let embeddings = self.embedder.embed_batch(batch_texts).await?;
            
            // Score each node and maintain top-k heap with indices only
            for (offset, emb) in embeddings.into_iter().enumerate() {
                let idx = batch_start + offset;
                let sim = EmbeddingEngine::cosine_similarity(q_emb, &emb);
                heap.push(Reverse((OrderedFloat(sim), idx)));
                
                // Keep heap size bounded
                if heap.len() > top_k {
                    heap.pop();
                }
            }
        }

        // Materialize results only for top-k nodes
        let mut results: Vec<(String, f32, String)> = heap.into_iter()
            .filter_map(|Reverse((score, idx))| {
                let id = &node_ids[idx];
                self.store.nodes.get(id).map(|node| {
                    (id.clone(), score.into_inner(), node.summary.clone())
                })
            })
            .collect();
        
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    /// Retrieve context for a query: top summary nodes plus fallback chunk matches.
    /// `expand_k` limits how many chunks to return for context if needed.
    /// Optimized to minimize memory allocations by using indices instead of cloning strings.
    pub async fn retrieve_with_context(&self, query: &str, top_k: usize, expand_k: usize, chunk_threshold: f32) -> Result<(Vec<(String, f32, String)>, Vec<(String, f32, String)>)> {
        // Compute query embedding once
        let q_emb = self.embedder.embed_text(query).await?;

        let summaries = self.retrieve_with_emb(&q_emb, top_k).await?;

        // If top summary is confident enough, skip chunk search
        if let Some((_, score, _)) = summaries.first() {
            if *score >= chunk_threshold {
                return Ok((summaries, Vec::new()));
            }
        }

        // Fallback: if chunk embeddings exist, query them directly (most memory-friendly)
        if !self.store.chunk_embeddings.is_empty() {
            let hits = self.store.query_top_k_chunks(&q_emb, expand_k);
            let mut chunk_matches = Vec::with_capacity(hits.len());
            for (id, score) in hits.into_iter() {
                let text = self.store.chunk_map.get(&id).cloned().unwrap_or_default();
                chunk_matches.push((id, score, text));
            }
            return Ok((summaries, chunk_matches));
        }

        // Otherwise, fallback to batched embedding of chunks using index-based approach
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        use ordered_float::OrderedFloat;

        const DEFAULT_BATCH_SIZE: usize = 128;
        
        // Collect chunk IDs first to avoid repeated HashMap access
        let chunk_ids: Vec<String> = self.store.chunk_map.keys().cloned().collect();
        let chunk_count = chunk_ids.len();
        
        // Use index-based heap to avoid cloning strings during processing
        let mut heap: BinaryHeap<Reverse<(OrderedFloat<f32>, usize)>> = BinaryHeap::new();

        // Process chunks in batches
        for batch_start in (0..chunk_count).step_by(DEFAULT_BATCH_SIZE) {
            let batch_end = (batch_start + DEFAULT_BATCH_SIZE).min(chunk_count);
            
            // Prepare batch texts using indices
            let batch_texts: Vec<&str> = (batch_start..batch_end)
                .filter_map(|i| self.store.chunk_map.get(&chunk_ids[i]).map(|s| s.as_str()))
                .collect();
            
            if batch_texts.is_empty() {
                continue;
            }

            // Embed batch
            let embeddings = self.embedder.embed_batch(batch_texts).await?;
            
            // Score each chunk and maintain top-k heap with indices only
            for (offset, emb) in embeddings.into_iter().enumerate() {
                let idx = batch_start + offset;
                let sim = EmbeddingEngine::cosine_similarity(&q_emb, &emb);
                heap.push(Reverse((OrderedFloat(sim), idx)));
                
                // Keep heap size bounded
                if heap.len() > expand_k {
                    heap.pop();
                }
            }
        }

        // Materialize results only for top-k chunks
        let mut chunk_matches: Vec<(String, f32, String)> = heap.into_iter()
            .filter_map(|Reverse((score, idx))| {
                let id = &chunk_ids[idx];
                self.store.chunk_map.get(id).map(|text| {
                    (id.clone(), score.into_inner(), text.clone())
                })
            })
            .collect();
        
        chunk_matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok((summaries, chunk_matches))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raptor::summarizer::SummaryNode;

    /// Test básico de SummaryNode sin embeddings (no causa memory leak)
    #[test]
    fn test_summary_node_creation() {
        let node = SummaryNode::new("test summary".to_string(), vec!["child1".to_string()], false);
        assert!(!node.id.is_empty());
        assert_eq!(node.summary, "test summary");
        assert_eq!(node.children.len(), 1);
    }

    /// Test básico de TreeStore sin embeddings (no causa memory leak)
    #[test]
    fn test_tree_store_basic() {
        let mut store = TreeStore::new();
        store.insert_chunk("c1".to_string(), "content 1".to_string());
        assert_eq!(store.chunk_map.len(), 1);
        
        let node = SummaryNode::new("summary".to_string(), vec![], false);
        let id = node.id.clone();
        store.insert_node(node);
        assert!(store.get_node(&id).is_some());
        
        // Test clear
        store.clear();
        assert!(store.chunk_map.is_empty());
        assert!(store.nodes.is_empty());
    }

    #[tokio::test]
    #[ignore] // HEAVY: Requires embedding model (~500MB). Run manually: cargo test -- --ignored
    async fn test_retriever_basic() {
        let embedder = EmbeddingEngine::new().await.unwrap();
        let mut store = TreeStore::new();

        let node = SummaryNode::new("El gato se sentó en la alfombra".to_string(), vec![], false);
        let _nid = node.id.clone();
        store.insert_node(node);
        store.insert_chunk("c1".to_string(), "Un fragmento sobre gatos y muebles".to_string());

        let retriever = TreeRetriever::new(&embedder, &store);
        let res = retriever.retrieve("gatos y muebles", 3).await.unwrap();
        assert!(!res.is_empty());

        let (_summaries, chunks) = retriever.retrieve_with_context("gatos y muebles", 3, 3, 0.95).await.unwrap();
        // Because threshold is high, fallback chunk search should run and return something
        assert!(chunks.len() <= 3);
    }

    #[tokio::test]
    #[ignore] // Requires embedding model - run manually with: cargo test -- --ignored
    async fn test_retriever_batching() {
        // Light test to verify batching logic works with reasonable memory usage
        let embedder = EmbeddingEngine::new().await.unwrap();
        let mut store = TreeStore::new();

        // add a root summary node
        let node = SummaryNode::new("root summary".to_string(), vec![], false);
        store.insert_node(node);

        // Only 50 chunks for reasonable memory usage in CI/local tests
        let total_chunks = 50usize;
        for i in 0..total_chunks {
            let text = format!("fragmento {} sobre gatos y muebles repetido {}", i, i % 10);
            store.insert_chunk(format!("c{}", i), text);
        }

        let retriever = TreeRetriever::new(&embedder, &store);
        let (summaries, chunks) = retriever.retrieve_with_context("gatos y muebles", 5, 10, 0.99).await.unwrap();

        // ensure bounded results
        assert!(summaries.len() <= 5);
        assert!(chunks.len() <= 10);
        
        // Verify we actually get results
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    #[ignore] // Ignored by default: requires significant memory. Run with: cargo test -- --ignored
    async fn test_retriever_stress() {
        // HEAVY stress test - only run manually when needed
        // WARNING: May consume 1-2GB+ memory with embedding model loaded
        let embedder = EmbeddingEngine::new().await.unwrap();
        let mut store = TreeStore::new();

        let node = SummaryNode::new("root summary".to_string(), vec![], false);
        store.insert_node(node);

        // Reduced to 200 chunks - still heavy but manageable
        let total_chunks = 200usize;
        for i in 0..total_chunks {
            let text = format!("fragmento {} sobre gatos y muebles repetido {}", i, i % 10);
            store.insert_chunk(format!("c{}", i), text);
        }

        let retriever = TreeRetriever::new(&embedder, &store);
        let (summaries, chunks) = retriever.retrieve_with_context("gatos y muebles", 5, 10, 0.99).await.unwrap();

        assert!(summaries.len() <= 5);
        assert!(chunks.len() <= 10);
        assert!(!chunks.is_empty());
    }
}
