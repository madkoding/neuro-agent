//! Embedding Module
//!
//! Provides text embedding generation using FastEmbed (ONNX-based, local inference).
//! Uses sentence-transformers model for semantic code search.

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;
use tokio::sync::RwLock;
use lru::LruCache;
use std::num::NonZeroUsize;

/// Default embedding model
const DEFAULT_MODEL: EmbeddingModel = EmbeddingModel::AllMiniLML6V2;

/// Embedding dimension for AllMiniLML6V2
pub const EMBEDDING_DIMENSION: usize = 384;

/// Embedding engine for generating text embeddings
pub struct EmbeddingEngine {
    model: Arc<RwLock<TextEmbedding>>,
    cache: Arc<RwLock<LruCache<String, Vec<f32>>>>,
    model_name: String,
    dimension: usize,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with default model
    pub async fn new() -> Result<Self> {
        Self::with_model(DEFAULT_MODEL).await
    }

    /// Create a new embedding engine with specific model
    pub async fn with_model(embedding_model: EmbeddingModel) -> Result<Self> {
        let model_name = format!("{:?}", embedding_model);

        // Initialize FastEmbed model
        let init_options = InitOptions::new(embedding_model);

        let model = tokio::task::spawn_blocking(move || {
            TextEmbedding::try_new(init_options)
        })
        .await
        .context("Failed to spawn blocking task")?
        .context("Failed to initialize embedding model")?;

        // Create LRU cache for embeddings (max 1000 entries)
        let cache_size = NonZeroUsize::new(1000).unwrap();
        let cache = LruCache::new(cache_size);

        Ok(Self {
            model: Arc::new(RwLock::new(model)),
            cache: Arc::new(RwLock::new(cache)),
            model_name,
            dimension: EMBEDDING_DIMENSION,
        })
    }

    /// Embed a single text
    pub async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get(text) {
                return Ok(cached.clone());
            }
        }

        // Generate embedding
        let text_owned = text.to_string();
        let model = self.model.clone();

        let embeddings = tokio::task::spawn_blocking(move || {
            let model_guard = futures::executor::block_on(model.read());
            model_guard.embed(vec![text_owned], None)
        })
        .await
        .context("Failed to spawn blocking task")?
        .context("Failed to generate embedding")?;

        if embeddings.is_empty() {
            anyhow::bail!("No embedding generated");
        }

        let embedding = embeddings[0].clone();

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.put(text.to_string(), embedding.clone());
        }

        Ok(embedding)
    }

    /// Embed multiple texts in batch (more efficient)
    pub async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(texts.len());
        let mut to_embed = Vec::new();
        let mut to_embed_indices = Vec::new();

        // Check cache for each text
        {
            let mut cache = self.cache.write().await;
            for (i, text) in texts.iter().enumerate() {
                if let Some(cached) = cache.get(*text) {
                    results.push(cached.clone());
                } else {
                    to_embed.push(text.to_string());
                    to_embed_indices.push(i);
                    results.push(Vec::new()); // Placeholder
                }
            }
        }

        // Embed texts that weren't in cache
        if !to_embed.is_empty() {
            let model = self.model.clone();
            let to_embed_copy = to_embed.clone();

            let embeddings = tokio::task::spawn_blocking(move || {
                let model_guard = futures::executor::block_on(model.read());
                model_guard.embed(to_embed_copy, None)
            })
            .await
            .context("Failed to spawn blocking task")?
            .context("Failed to generate embeddings")?;

            // Update cache and results
            {
                let mut cache = self.cache.write().await;
                for (i, embedding) in embeddings.into_iter().enumerate() {
                    let text = &to_embed[i];
                    let idx = to_embed_indices[i];

                    cache.put(text.clone(), embedding.clone());
                    results[idx] = embedding;
                }
            }
        }

        Ok(results)
    }

    /// Get model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Calculate cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    /// Normalize an embedding vector
    pub fn normalize(embedding: &mut [f32]) {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in embedding.iter_mut() {
                *val /= norm;
            }
        }
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        (cache.len(), cache.cap().get())
    }
}

/// Helper to convert embedding to blob for SQLite storage
pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &value in embedding {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Helper to convert blob from SQLite to embedding
pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>> {
    if blob.len() % 4 != 0 {
        anyhow::bail!("Invalid blob size for f32 array");
    }

    let mut embedding = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        let bytes: [u8; 4] = chunk.try_into().context("Invalid chunk size")?;
        embedding.push(f32::from_le_bytes(bytes));
    }

    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_engine() {
        let engine = EmbeddingEngine::new().await.unwrap();

        let text = "This is a test sentence";
        let embedding = engine.embed_text(text).await.unwrap();

        assert_eq!(embedding.len(), EMBEDDING_DIMENSION);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let engine = EmbeddingEngine::new().await.unwrap();

        let texts = vec![
            "First sentence",
            "Second sentence",
            "Third sentence",
        ];

        let embeddings = engine.embed_batch(texts).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        for emb in embeddings {
            assert_eq!(emb.len(), EMBEDDING_DIMENSION);
        }
    }

    #[tokio::test]
    async fn test_cosine_similarity() {
        let engine = EmbeddingEngine::new().await.unwrap();

        let text1 = "The quick brown fox";
        let text2 = "The fast brown fox";
        let text3 = "Completely different text";

        let emb1 = engine.embed_text(text1).await.unwrap();
        let emb2 = engine.embed_text(text2).await.unwrap();
        let emb3 = engine.embed_text(text3).await.unwrap();

        let sim_12 = EmbeddingEngine::cosine_similarity(&emb1, &emb2);
        let sim_13 = EmbeddingEngine::cosine_similarity(&emb1, &emb3);

        // Similar texts should have higher similarity
        assert!(sim_12 > sim_13);
    }

    #[test]
    fn test_embedding_serialization() {
        let original = vec![1.0, 2.5, -3.7, 0.0, 4.2];
        let blob = embedding_to_blob(&original);
        let restored = blob_to_embedding(&blob).unwrap();

        assert_eq!(original, restored);
    }

    #[tokio::test]
    async fn test_cache() {
        let engine = EmbeddingEngine::new().await.unwrap();

        let text = "Cached text";

        // First call - should compute
        let emb1 = engine.embed_text(text).await.unwrap();

        // Second call - should use cache
        let emb2 = engine.embed_text(text).await.unwrap();

        assert_eq!(emb1, emb2);

        let (used, capacity) = engine.cache_stats().await;
        assert_eq!(used, 1);
        assert!(capacity > 0);
    }
}
