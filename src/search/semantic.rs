//! Semantic Search Engine
//!
//! Hybrid search combining BM25 (keyword-based) and vector similarity (semantic).

use crate::db::{Database, DatabaseError};
use crate::embedding::{blob_to_embedding, embedding_to_blob, EmbeddingEngine};
use crate::search::chunker::CodeChunk;
use anyhow::{Context, Result};
use sqlx::Row;
use std::sync::Arc;
use thiserror::Error;

/// Maximum number of BM25 candidates to retrieve
const BM25_CANDIDATE_LIMIT: usize = 50;

/// Default similarity threshold
const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.3;

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Chunk ID
    pub chunk_id: String,

    /// File path
    pub file_path: String,

    /// Symbol name (if applicable)
    pub symbol_name: Option<String>,

    /// Chunk type
    pub chunk_type: String,

    /// Content of the chunk
    pub content: String,

    /// Summary (formatted for LLM)
    pub summary: String,

    /// Similarity score (0.0-1.0)
    pub score: f32,

    /// Line range in file
    pub line_range: (usize, usize),

    /// Language
    pub language: String,
}

impl SearchResult {
    /// Format for LLM consumption
    pub fn format_for_llm(&self) -> String {
        format!(
            "**{}** in `{}` (lines {}-{}) - Score: {:.2}\n{}\n\n```{}\n{}\n```",
            self.symbol_name.as_deref().unwrap_or(&self.chunk_type),
            self.file_path,
            self.line_range.0,
            self.line_range.1,
            self.score,
            self.summary,
            self.language,
            self.content
        )
    }
}

/// Semantic search engine
pub struct SemanticSearch {
    db: Arc<Database>,
    embedder: Arc<EmbeddingEngine>,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),

    #[error("Embedding error: {0}")]
    EmbeddingError(#[from] anyhow::Error),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Invalid embedding dimension")]
    InvalidEmbeddingDimension,
}

impl SemanticSearch {
    /// Create a new semantic search engine
    pub fn new(db: Arc<Database>, embedder: Arc<EmbeddingEngine>) -> Self {
        Self { db, embedder }
    }

    /// Hybrid search: BM25 + Vector similarity
    pub async fn search(
        &self,
        query: &str,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        // Step 1: Embed the query
        let query_embedding = self
            .embedder
            .embed_text(query)
            .await
            .context("Failed to embed query")?;

        // Step 2: BM25 full-text search for candidates
        let candidates = self.bm25_search(project_id, query, BM25_CANDIDATE_LIMIT).await?;

        // Step 3: If no candidates, try semantic-only search
        if candidates.is_empty() {
            return self
                .vector_search_all(project_id, &query_embedding, limit)
                .await;
        }

        // Step 4: Compute vector similarity for candidates
        let mut scored_results = Vec::new();

        for candidate in candidates {
            let embedding = match blob_to_embedding(&candidate.embedding) {
                Ok(emb) => emb,
                Err(_) => continue,
            };

            let similarity = EmbeddingEngine::cosine_similarity(&query_embedding, &embedding);

            // Filter by threshold
            if similarity >= DEFAULT_SIMILARITY_THRESHOLD {
                scored_results.push((candidate, similarity));
            }
        }

        // Step 5: Sort by similarity (descending)
        scored_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Step 6: Take top N results
        let results = scored_results
            .into_iter()
            .take(limit)
            .map(|(candidate, score)| SearchResult {
                chunk_id: candidate.chunk_id,
                file_path: candidate.file_path,
                symbol_name: candidate.symbol_name,
                chunk_type: candidate.chunk_type,
                content: candidate.chunk_text,
                summary: candidate.chunk_summary,
                score,
                line_range: (candidate.line_start, candidate.line_end),
                language: candidate.language,
            })
            .collect();

        Ok(results)
    }

    /// BM25 full-text search using FTS5
    async fn bm25_search(
        &self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CandidateChunk>, SearchError> {
        let pool = self.db.pool();

        // FTS5 search query
        let sql = r#"
            SELECT
                ce.chunk_id,
                ce.chunk_type,
                ce.chunk_text,
                ce.chunk_summary,
                ce.line_start,
                ce.line_end,
                ce.language,
                ce.embedding,
                f.relative_path as file_path,
                s.symbol_name
            FROM code_fts fts
            INNER JOIN code_embeddings ce ON fts.chunk_id = ce.chunk_id
            INNER JOIN indexed_files f ON ce.file_id = f.id
            LEFT JOIN code_symbols s ON ce.symbol_id = s.id
            WHERE ce.project_id = ?
              AND fts.chunk_text MATCH ?
            ORDER BY rank
            LIMIT ?
        "#;

        let rows = sqlx::query(sql)
            .bind(project_id)
            .bind(query)
            .bind(limit as i64)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryError(format!("FTS5 search failed: {}", e))
            })?;

        let mut candidates = Vec::new();
        for row in rows {
            let candidate = CandidateChunk {
                chunk_id: row.try_get("chunk_id").unwrap_or_default(),
                chunk_type: row.try_get("chunk_type").unwrap_or_default(),
                chunk_text: row.try_get("chunk_text").unwrap_or_default(),
                chunk_summary: row.try_get("chunk_summary").unwrap_or_default(),
                line_start: row.try_get::<i64, _>("line_start").unwrap_or(0) as usize,
                line_end: row.try_get::<i64, _>("line_end").unwrap_or(0) as usize,
                language: row.try_get("language").unwrap_or_default(),
                embedding: row.try_get("embedding").unwrap_or_default(),
                file_path: row.try_get("file_path").unwrap_or_default(),
                symbol_name: row.try_get("symbol_name").ok(),
            };
            candidates.push(candidate);
        }

        Ok(candidates)
    }

    /// Vector-only search (fallback when BM25 has no results)
    async fn vector_search_all(
        &self,
        project_id: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let pool = self.db.pool();

        // Get all embeddings for the project
        let sql = r#"
            SELECT
                ce.chunk_id,
                ce.chunk_type,
                ce.chunk_text,
                ce.chunk_summary,
                ce.line_start,
                ce.line_end,
                ce.language,
                ce.embedding,
                f.relative_path as file_path,
                s.symbol_name
            FROM code_embeddings ce
            INNER JOIN indexed_files f ON ce.file_id = f.id
            LEFT JOIN code_symbols s ON ce.symbol_id = s.id
            WHERE ce.project_id = ?
        "#;

        let rows = sqlx::query(sql)
            .bind(project_id)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryError(format!("Vector search failed: {}", e))
            })?;

        // Compute similarity for all chunks
        let mut scored_results = Vec::new();

        for row in rows {
            let embedding_blob: Vec<u8> = row.try_get("embedding").unwrap_or_default();
            let embedding = match blob_to_embedding(&embedding_blob) {
                Ok(emb) => emb,
                Err(_) => continue,
            };

            let similarity = EmbeddingEngine::cosine_similarity(query_embedding, &embedding);

            if similarity >= DEFAULT_SIMILARITY_THRESHOLD {
                let result = SearchResult {
                    chunk_id: row.try_get("chunk_id").unwrap_or_default(),
                    file_path: row.try_get("file_path").unwrap_or_default(),
                    symbol_name: row.try_get("symbol_name").ok(),
                    chunk_type: row.try_get("chunk_type").unwrap_or_default(),
                    content: row.try_get("chunk_text").unwrap_or_default(),
                    summary: row.try_get("chunk_summary").unwrap_or_default(),
                    score: similarity,
                    line_range: (
                        row.try_get::<i64, _>("line_start").unwrap_or(0) as usize,
                        row.try_get::<i64, _>("line_end").unwrap_or(0) as usize,
                    ),
                    language: row.try_get("language").unwrap_or_default(),
                };
                scored_results.push(result);
            }
        }

        // Sort by similarity
        scored_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N
        scored_results.truncate(limit);

        Ok(scored_results)
    }

    /// Index chunks with embeddings
    pub async fn index_chunks(
        &self,
        project_id: &str,
        chunks: Vec<CodeChunk>,
    ) -> Result<usize, SearchError> {
        if chunks.is_empty() {
            return Ok(0);
        }

        // Extract texts for batch embedding
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();

        // Generate embeddings in batch
        let embeddings = self
            .embedder
            .embed_batch(texts)
            .await
            .context("Failed to generate embeddings")?;

        let pool = self.db.pool();
        let mut indexed_count = 0;

        // Insert each chunk with its embedding
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            let embedding_blob = embedding_to_blob(embedding);

            // Insert into code_embeddings
            let result = sqlx::query(
                r#"
                INSERT INTO code_embeddings (
                    project_id, chunk_id, chunk_type, file_id, symbol_id,
                    embedding, chunk_text, chunk_summary,
                    line_start, line_end, language
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(chunk_id) DO UPDATE SET
                    embedding = excluded.embedding,
                    chunk_text = excluded.chunk_text,
                    chunk_summary = excluded.chunk_summary,
                    indexed_at = datetime('now')
                "#,
            )
            .bind(project_id)
            .bind(&chunk.id)
            .bind(chunk.chunk_type.as_str())
            .bind(1) // TODO: Get actual file_id from database
            .bind(chunk.symbol_id)
            .bind(&embedding_blob)
            .bind(&chunk.text)
            .bind(&chunk.summary)
            .bind(chunk.line_start as i64)
            .bind(chunk.line_end as i64)
            .bind(&chunk.language)
            .execute(pool)
            .await;

            if result.is_ok() {
                // Insert into FTS5 index
                let _ = sqlx::query(
                    r#"
                    INSERT INTO code_fts (chunk_id, chunk_text, file_path, symbol_name, language)
                    VALUES (?, ?, ?, ?, ?)
                    ON CONFLICT(chunk_id) DO UPDATE SET
                        chunk_text = excluded.chunk_text
                    "#,
                )
                .bind(&chunk.id)
                .bind(&chunk.text)
                .bind(chunk.file_path.to_string_lossy().as_ref())
                .bind(&chunk.symbol_name)
                .bind(&chunk.language)
                .execute(pool)
                .await;

                indexed_count += 1;
            }
        }

        // Update metadata
        self.update_embedding_metadata(project_id, indexed_count)
            .await?;

        Ok(indexed_count)
    }

    /// Update embedding metadata for a project
    async fn update_embedding_metadata(
        &self,
        project_id: &str,
        chunk_count: usize,
    ) -> Result<(), SearchError> {
        let pool = self.db.pool();

        let model_name = self.embedder.model_name();
        let dimension = self.embedder.dimension() as i64;

        sqlx::query(
            r#"
            INSERT INTO embedding_metadata (
                project_id, model_name, model_version, dimension, last_updated, total_chunks
            ) VALUES (?, ?, ?, ?, datetime('now'), ?)
            ON CONFLICT(project_id) DO UPDATE SET
                last_updated = datetime('now'),
                total_chunks = total_chunks + excluded.total_chunks
            "#,
        )
        .bind(project_id)
        .bind(model_name)
        .bind("v1.0")
        .bind(dimension)
        .bind(chunk_count as i64)
        .execute(pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Failed to update metadata: {}", e)))?;

        Ok(())
    }

    /// Get embedding statistics for a project
    pub async fn get_stats(&self, project_id: &str) -> Result<EmbeddingStats, SearchError> {
        let pool = self.db.pool();

        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) as total_chunks,
                COUNT(DISTINCT chunk_type) as chunk_types,
                COUNT(DISTINCT file_id) as indexed_files
            FROM code_embeddings
            WHERE project_id = ?
            "#,
        )
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Failed to get stats: {}", e)))?;

        Ok(EmbeddingStats {
            total_chunks: row.try_get::<i64, _>("total_chunks").unwrap_or(0) as usize,
            chunk_types: row.try_get::<i64, _>("chunk_types").unwrap_or(0) as usize,
            indexed_files: row.try_get::<i64, _>("indexed_files").unwrap_or(0) as usize,
        })
    }
}

/// Candidate chunk from BM25 search
struct CandidateChunk {
    chunk_id: String,
    chunk_type: String,
    chunk_text: String,
    chunk_summary: String,
    line_start: usize,
    line_end: usize,
    language: String,
    embedding: Vec<u8>,
    file_path: String,
    symbol_name: Option<String>,
}

/// Embedding statistics
#[derive(Debug, Clone)]
pub struct EmbeddingStats {
    pub total_chunks: usize,
    pub chunk_types: usize,
    pub indexed_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a database setup and are integration tests
    // They should be run with `cargo test --features integration-tests`
}
