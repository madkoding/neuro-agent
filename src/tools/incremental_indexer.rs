//! Incremental Indexer - Re-index only changed files

use anyhow::Result;

#[derive(Default)]
pub struct IncrementalIndexer {
    // Placeholder - in real implementation would include:
    // db: Arc<Database>,
    // ast_parser: AstParser,
    // embedder: Arc<EmbeddingEngine>,
}

#[derive(Default)]
pub struct UpdateReport {
    pub files_updated: usize,
    pub files_added: usize,
    pub files_removed: usize,
    pub chunks_updated: usize,
}

impl IncrementalIndexer {
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Re-index ONLY changed files
    pub async fn update_project(&mut self, _project_id: &str) -> Result<UpdateReport> {
        let report = UpdateReport::default();

        // In full implementation:
        // 1. Get all indexed files from database
        // 2. Check file hash for each
        // 3. Re-index only changed files
        // 4. Remove deleted files from index

        Ok(report)
    }
}

#[allow(dead_code)]
/// Compute SHA256 hash of content
fn compute_hash(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash() {
        let content = b"hello world";
        let hash = compute_hash(content);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
    }
}
