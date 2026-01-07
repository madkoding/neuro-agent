//! Search module for semantic code search

pub mod chunker;
pub mod semantic;

pub use chunker::{ChunkType, CodeChunk, CodeChunker};
pub use semantic::{EmbeddingStats, SearchError, SearchResult, SemanticSearch};
