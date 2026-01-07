use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Lightweight in-memory persistence with optional on-disk snapshot.
/// For production, swap this for a Chroma/FAISS backend or Qdrant connector.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStore {
    pub nodes: HashMap<String, super::summarizer::SummaryNode>,
    pub chunk_map: HashMap<String, String>, // chunk_id -> content

    // Precomputed embeddings - now serialized for persistence
    #[serde(default)]
    pub summary_embeddings: HashMap<String, Vec<f32>>,
    #[serde(default)]
    pub chunk_embeddings: HashMap<String, Vec<f32>>,
    
    // Metadata for cache validation
    #[serde(default)]
    pub project_path: String,
    #[serde(default)]
    pub created_at: u64,
    
    // Incremental indexing - track processed files
    #[serde(default)]
    pub indexed_files: HashMap<String, u64>, // file_path -> modified_time
    #[serde(default)]
    pub indexing_complete: bool,
}

/// Maximum chunks to store (to prevent unbounded memory growth)
const MAX_CHUNKS: usize = 5000;
/// Maximum summary nodes to store
const MAX_NODES: usize = 500;

impl TreeStore {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            chunk_map: HashMap::new(),
            summary_embeddings: HashMap::new(),
            chunk_embeddings: HashMap::new(),
            project_path: String::new(),
            created_at: 0,
            indexed_files: HashMap::new(),
            indexing_complete: false,
        }
    }
    
    /// Clear all data from the store to free memory
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.chunk_map.clear();
        self.summary_embeddings.clear();
        self.chunk_embeddings.clear();
        self.project_path.clear();
        self.created_at = 0;
        self.indexed_files.clear();
        self.indexing_complete = false;
        // Shrink to free memory
        self.nodes.shrink_to_fit();
        self.chunk_map.shrink_to_fit();
        self.summary_embeddings.shrink_to_fit();
        self.chunk_embeddings.shrink_to_fit();
        self.indexed_files.shrink_to_fit();
    }
    
    /// Check if store is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.chunk_map.len() >= MAX_CHUNKS || self.nodes.len() >= MAX_NODES
    }

    pub fn insert_node(&mut self, node: super::summarizer::SummaryNode) {
        // Evitar crecimiento sin límites
        if self.nodes.len() >= MAX_NODES {
            return; // Silently skip if at capacity
        }
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn insert_chunk(&mut self, chunk_id: String, content: String) {
        // Evitar crecimiento sin límites
        if self.chunk_map.len() >= MAX_CHUNKS {
            return; // Silently skip if at capacity
        }
        self.chunk_map.insert(chunk_id, content);
    }

    pub fn get_node(&self, id: &str) -> Option<&super::summarizer::SummaryNode> {
        self.nodes.get(id)
    }

    pub fn get_chunk(&self, id: &str) -> Option<&String> {
        self.chunk_map.get(id)
    }

    /// Insert a precomputed embedding for a summary node
    pub fn insert_summary_embedding(&mut self, node_id: String, emb: Vec<f32>) {
        if self.summary_embeddings.len() >= MAX_NODES {
            return;
        }
        self.summary_embeddings.insert(node_id, emb);
    }

    /// Insert a precomputed embedding for a chunk
    pub fn insert_chunk_embedding(&mut self, chunk_id: String, emb: Vec<f32>) {
        if self.chunk_embeddings.len() >= MAX_CHUNKS {
            return;
        }
        self.chunk_embeddings.insert(chunk_id, emb);
    }

    /// Query top-k summary nodes using a precomputed query embedding.
    pub fn query_top_k_summaries(&self, q_emb: &[f32], top_k: usize) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = self
            .summary_embeddings
            .iter()
            .map(|(id, emb)| {
                let sim = crate::embedding::EmbeddingEngine::cosine_similarity(q_emb, emb);
                (id.clone(), sim)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Query top-k chunks using a precomputed query embedding.
    pub fn query_top_k_chunks(&self, q_emb: &[f32], top_k: usize) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = self
            .chunk_embeddings
            .iter()
            .map(|(id, emb)| {
                let sim = crate::embedding::EmbeddingEngine::cosine_similarity(q_emb, emb);
                (id.clone(), sim)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Simple on-disk save for persistence (now includes embeddings)
    pub fn save_to(&self, path: PathBuf) -> Result<()> {
        // Use bincode for faster serialization and smaller file size
        let data = bincode::serialize(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load_from(path: PathBuf) -> Result<Self> {
        let data = std::fs::read(path)?;
        let s: Self = bincode::deserialize(&data)?;
        Ok(s)
    }
    
    /// Get the cache file path for a project
    pub fn cache_path_for(project_path: &str) -> PathBuf {
        // Create a hash of the project path for the filename
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        project_path.hash(&mut hasher);
        let hash = hasher.finish();
        
        // Use system cache directory or fallback to .neuro-cache
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("neuro-agent");
        
        std::fs::create_dir_all(&cache_dir).ok();
        cache_dir.join(format!("raptor_{:x}.bin", hash))
    }
    
    /// Check if cache is valid for the given project
    pub fn is_cache_valid(&self, project_path: &str) -> bool {
        if self.project_path != project_path {
            return false;
        }
        // Cache is valid if created within last 24 hours
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        now.saturating_sub(self.created_at) < 86400 // 24 hours
    }
    
    /// Set metadata for cache validation
    pub fn set_metadata(&mut self, project_path: &str) {
        self.project_path = project_path.to_string();
        self.created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
}

// Global store wrapper (safe for simple CLI; for async you can wrap in Tokio Mutex)
lazy_static::lazy_static! {
    pub static ref GLOBAL_STORE: Mutex<TreeStore> = Mutex::new(TreeStore::new());
}

/// Try to load RAPTOR cache from disk
pub fn load_cache_if_valid(project_path: &str) -> bool {
    let cache_path = TreeStore::cache_path_for(project_path);
    
    if !cache_path.exists() {
        return false;
    }
    
    match TreeStore::load_from(cache_path) {
        Ok(store) if store.is_cache_valid(project_path) && !store.chunk_map.is_empty() => {
            let mut global = GLOBAL_STORE.lock().unwrap();
            *global = store;
            true
        }
        _ => false
    }
}

/// Save current RAPTOR store to disk
pub fn save_cache(project_path: &str) -> Result<()> {
    let cache_path = TreeStore::cache_path_for(project_path);
    let mut store = GLOBAL_STORE.lock().unwrap();
    store.set_metadata(project_path);
    store.save_to(cache_path)
}
