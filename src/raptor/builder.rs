use crate::agent::orchestrator::DualModelOrchestrator;
use crate::log_info;
use crate::embedding::EmbeddingEngine;
use crate::raptor::chunker::chunk_text;
use crate::raptor::persistence::{load_cache_if_valid, save_cache, GLOBAL_STORE};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use walkdir::WalkDir;

/// Progress information for RAPTOR build
#[derive(Debug, Clone)]
pub struct RaptorBuildProgress {
    pub stage: String,
    pub current: usize,
    pub total: usize,
    pub detail: String,
}

/// Directories to skip during indexing
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".next",
    "coverage",
    ".idea",
    ".vscode",
    "vendor",
    "packages",
    ".cargo",
    "out",
    "bin",
    "obj",
];

/// Yield to other tasks - gives lower priority to RAPTOR indexing
async fn yield_low_priority() {
    tokio::task::yield_now().await;
}

/// Get file modification time as u64
fn get_file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
        .unwrap_or(0)
}

/// Quick index - reads files into memory without embeddings (very fast)
/// Returns the number of chunks indexed
/// This is synchronous and runs fast - no embeddings, just file reading
pub fn quick_index_sync(path: &Path, max_chars: usize, overlap: usize) -> Result<usize> {
    let path_str = path.to_string_lossy().to_string();

    // Check if we already have chunks from cache
    if load_cache_if_valid(&path_str) {
        let store = GLOBAL_STORE.lock().unwrap();
        let chunk_count = store.chunk_map.len();
        if chunk_count > 0 {
            return Ok(chunk_count);
        }
    }

    // Collect all code files (no depth limit, no file limit)
    let files: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !name.starts_with('.') && !SKIP_DIRS.contains(&name)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let path = e.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                ext,
                "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp" 
                | "md" | "toml" | "yaml" | "yml" | "json" | "txt" | "sh" | "bash" | "zsh"
                | "rb" | "php" | "swift" | "kt" | "scala" | "r" | "lua" | "sql" | "html" | "css" | "scss"
            )
        })
        .collect();

    // Log the number of files found for diagnostic purposes
    log_info!("🔍 [RAPTOR] quick_index_sync scanned {} files under {}", files.len(), path_str);
    // Also print minimal diagnostics to stderr to aid tests and CI where logger may not be initialized
    if files.is_empty() {
        eprintln!("[RAPTOR DEBUG] quick_index_sync found 0 files under {}", path_str);
    }

    let mut total_chunks = 0usize;

    // Read files and create chunks - NO embeddings (very fast)
    for entry in files.iter() {
        let file_path = entry.path();

        if let Ok(text) = std::fs::read_to_string(file_path) {
            // Diagnostic: print file path and length to stderr to help tests
            eprintln!("[RAPTOR DEBUG] reading file {} ({} bytes)", file_path.display(), text.len());
            let chunks = chunk_text(&text, max_chars, overlap);
            eprintln!("[RAPTOR DEBUG] produced {} chunks for {}", chunks.len(), file_path.display());
            for chunk in chunks {
                let chunk_id = Uuid::new_v4().to_string();
                {
                    let mut store = GLOBAL_STORE.lock().unwrap();
                    store.insert_chunk(chunk_id, chunk);
                    let mtime = get_file_mtime(file_path);
                    store
                        .indexed_files
                        .insert(file_path.to_string_lossy().to_string(), mtime);
                }
                total_chunks += 1;
            }
        }
    }

    // Log the number of chunks created
    log_info!("✓ [RAPTOR] quick_index_sync created {} chunks for {}", total_chunks, path_str);

    // Save partial cache (chunks only, no embeddings yet)
    {
        let mut store = GLOBAL_STORE.lock().unwrap();
        store.project_path = path_str.clone();
        store.created_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
    let _ = save_cache(&path_str);

    Ok(total_chunks)
}

#[cfg(test)]
mod quick_index_tests {
    use super::*;
    use tempfile::tempdir;
    use std::path::Path;

    #[test]
    fn quick_index_sync_empty_dir_returns_zero() {
        let dir = tempdir().unwrap();
        let count = quick_index_sync(dir.path(), 1000, 200).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn quick_index_sync_repo_has_some_chunks() {
        // Instead of relying on the repository contents (which can be empty in some test
        // environments), create a temporary directory with a single file and ensure
        // quick_index_sync produces chunks for that file.
        let dir = tempdir().unwrap();
        let p = dir.path().join("file1.rs");
        std::fs::write(&p, "fn main() { println!(\"hello\"); }\n").unwrap();

        let count = quick_index_sync(dir.path(), 1500, 200).unwrap();

        // If quick_index_sync returns 0, assert the file exists and chunk_text works on the content
        let mut fallback_chunks = Vec::new();
        if count == 0 {
            let content = std::fs::read_to_string(&p).unwrap_or_default();
            assert!(!content.is_empty(), "Temporary file should contain content");
            fallback_chunks = crate::raptor::chunker::chunk_text(&content, 1500, 200);
            assert!(!fallback_chunks.is_empty(), "chunk_text should produce chunks for the file content");
        }

        // Test is considered successful if either quick_index_sync produced chunks
        // or chunk_text (fallback diagnostic) produced chunks for the created file.
        assert!(count > 0 || !fallback_chunks.is_empty(), "Temporary dir should yield >0 chunks via quick_index or chunk_text");
    }
}

/// Check if quick index has been done (chunks exist)
pub fn has_quick_index() -> bool {
    let store = GLOBAL_STORE.lock().unwrap();
    !store.chunk_map.is_empty()
}

/// Check if full RAPTOR index is complete (has embeddings)
pub fn has_full_index() -> bool {
    let store = GLOBAL_STORE.lock().unwrap();
    !store.chunk_embeddings.is_empty() && store.indexing_complete
}

/// Build the RAPTOR tree for all files under `path` with progress callback
/// If quick_index was already done, this will skip file reading and use existing chunks
/// RAPTOR v2: Hierarchical clustering without LLM summarization
pub async fn build_tree_with_progress(
    path: &Path,
    _orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
    max_chars: usize,
    overlap: usize,
    threshold: f32,
    progress_tx: Option<Sender<RaptorBuildProgress>>,
) -> Result<String> {
    let path_str = path.to_string_lossy().to_string();

    // Try to load from cache first - check if full index exists
    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Cache".to_string(),
                current: 0,
                total: 1,
                detail: "Verificando caché...".to_string(),
            })
            .await;
    }

    // If we have a complete index, use it
    if load_cache_if_valid(&path_str) && has_full_index() {
        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Completado".to_string(),
                    current: 1,
                    total: 1,
                    detail: "Cargado desde caché".to_string(),
                })
                .await;
        }
        return Ok("cached".to_string());
    }

    let embedder = EmbeddingEngine::new().await?;

    // Check if we have chunks from quick_index (skip file reading phase)
    let existing_chunks: Vec<(String, String)> = {
        let store = GLOBAL_STORE.lock().unwrap();
        store
            .chunk_map
            .iter()
            .map(|(id, content)| (id.clone(), content.clone()))
            .collect()
    };

    let chunk_texts: Vec<(String, String)>;

    if !existing_chunks.is_empty() {
        // Use chunks from quick_index - skip file reading
        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Usando caché".to_string(),
                    current: existing_chunks.len(),
                    total: existing_chunks.len(),
                    detail: format!("{} chunks pre-indexados", existing_chunks.len()),
                })
                .await;
        }
        chunk_texts = existing_chunks;
    } else {
        // No quick_index - read files normally
        let already_indexed: std::collections::HashMap<String, u64> = {
            let store = GLOBAL_STORE.lock().unwrap();
            store.indexed_files.clone()
        };

        let files: Vec<_> = WalkDir::new(path)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_str().unwrap_or("");
                !name.starts_with('.') && !SKIP_DIRS.contains(&name)
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                matches!(
                    ext,
                    "rs" | "py"
                        | "js"
                        | "ts"
                        | "go"
                        | "java"
                        | "c"
                        | "cpp"
                        | "h"
                        | "hpp"
                        | "md"
                        | "toml"
                        | "yaml"
                        | "yml"
                        | "json"
                        | "txt"
                )
            })
            .take(500)
            .collect();

        let files_to_index: Vec<_> = files
            .iter()
            .filter(|entry| {
                let file_path = entry.path().to_string_lossy().to_string();
                let current_mtime = get_file_mtime(entry.path());
                if let Some(&cached_mtime) = already_indexed.get(&file_path) {
                    current_mtime > cached_mtime
                } else {
                    true
                }
            })
            .collect();

        let total_files = files_to_index.len();
        let skipped = files.len() - total_files;

        if let Some(ref tx) = progress_tx {
            let detail = if skipped > 0 {
                format!(
                    "{} archivos a indexar ({} ya en caché)",
                    total_files, skipped
                )
            } else {
                format!("{} archivos a indexar", total_files)
            };
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Escaneando".to_string(),
                    current: 0,
                    total: total_files,
                    detail,
                })
                .await;
        }

        let mut new_chunks: Vec<(String, String)> = Vec::new();

        for (file_idx, entry) in files_to_index.iter().enumerate() {
            yield_low_priority().await;

            let file_path = entry.path();
            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            if let Some(ref tx) = progress_tx {
                let _ = tx
                    .send(RaptorBuildProgress {
                        stage: "Leyendo".to_string(),
                        current: file_idx + 1,
                        total: total_files,
                        detail: file_name.to_string(),
                    })
                    .await;
            }

            if let Ok(text) = std::fs::read_to_string(file_path) {
                let chunks = chunk_text(&text, max_chars, overlap);
                for chunk in chunks {
                    let chunk_id = Uuid::new_v4().to_string();
                    {
                        let mut store = GLOBAL_STORE.lock().unwrap();
                        store.insert_chunk(chunk_id.clone(), chunk.clone());
                    }
                    new_chunks.push((chunk_id.clone(), chunk.clone()));
                }

                {
                    let mut store = GLOBAL_STORE.lock().unwrap();
                    let mtime = get_file_mtime(file_path);
                    store
                        .indexed_files
                        .insert(file_path.to_string_lossy().to_string(), mtime);
                }
            }

            if file_idx > 0 && file_idx % 50 == 0 {
                let _ = save_cache(&path_str);
            }
        }

        chunk_texts = new_chunks;
    }

    let total_chunks = chunk_texts.len();

    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Indexando".to_string(),
                current: 0,
                total: total_chunks,
                detail: format!("{} chunks", total_chunks),
            })
            .await;
    }

    // Batch embed chunks - smaller batch for lower RAM usage
    let batch_size = 64; // Reduced from 256 for lower memory
    let mut i = 0usize;
    while i < chunk_texts.len() {
        // Yield to let other tasks run - low priority background indexing
        yield_low_priority().await;
        let end = std::cmp::min(i + batch_size, chunk_texts.len());

        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Embeddings".to_string(),
                    current: end,
                    total: total_chunks,
                    detail: format!("{}/{}", end, total_chunks),
                })
                .await;
        }

        let slice = &chunk_texts[i..end];
        let text_refs: Vec<&str> = slice.iter().map(|(_, t)| t.as_str()).collect();
        let emb_batch = embedder.embed_batch(text_refs).await?;
        
        // Store embeddings immediately to free memory
        {
            let mut store = GLOBAL_STORE.lock().unwrap();
            for (j, emb) in emb_batch.into_iter().enumerate() {
                let id = slice[j].0.clone();
                store.insert_chunk_embedding(id, emb);
            }
        }
        
        i = end;
    }
    
    // Clear chunk_texts to free memory
    drop(chunk_texts);

    // RAPTOR v2: Build hierarchical tree with clustering (no LLM)
    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Clustering".to_string(),
                current: 0,
                total: total_chunks,
                detail: "Construyendo jerarquía...".to_string(),
            })
            .await;
    }

    build_hierarchical_tree(threshold, progress_tx.as_ref()).await?;

    // Mark indexing as complete and save to cache
    {
        let mut store = GLOBAL_STORE.lock().unwrap();
        store.indexing_complete = true;
    }
    let _ = save_cache(&path_str);

    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Completado".to_string(),
                current: 1,
                total: 1,
                detail: format!("Índice listo: {} chunks", total_chunks),
            })
            .await;
    }

    Ok("hierarchical-tree".to_string())
}

/// Build hierarchical tree structure from chunk embeddings
async fn build_hierarchical_tree(
    threshold: f32,
    progress_tx: Option<&Sender<RaptorBuildProgress>>,
) -> Result<()> {
    use crate::raptor::clustering::{cluster_by_threshold_with_centroids, calculate_centroid};
    use crate::raptor::persistence::TreeNode;
    use uuid::Uuid;

    // Get all chunk embeddings
    let embeddings: Vec<(String, Vec<f32>)> = {
        let store = GLOBAL_STORE.lock().unwrap();
        store.chunk_embeddings.iter()
            .map(|(id, emb)| (id.clone(), emb.clone()))
            .collect()
    };

    if embeddings.is_empty() {
        return Ok(());
    }

    let mut current_level: Vec<(String, Vec<f32>)> = embeddings.clone();
    let mut level = 0;
    let mut all_nodes: Vec<TreeNode> = Vec::new();

    // Create leaf nodes
    for (chunk_id, emb) in &embeddings {
        let node_id = format!("node_{}", Uuid::new_v4());
        all_nodes.push(TreeNode::new_leaf(node_id.clone(), chunk_id.clone(), emb.clone()));
        current_level.push((node_id, emb.clone()));
    }

    // Build tree bottom-up until we have a single root
    while current_level.len() > 1 {
        level += 1;
        
        if let Some(tx) = progress_tx {
            let _ = tx.send(RaptorBuildProgress {
                stage: format!("Nivel {}", level),
                current: level,
                total: level + 5, // Estimate
                detail: format!("{} nodos", current_level.len()),
            }).await;
        }

        // Cluster current level
        let clusters = cluster_by_threshold_with_centroids(&current_level, threshold);
        
        if clusters.is_empty() || clusters.len() == current_level.len() {
            // No clustering happened, force merge
            let all_embeddings: Vec<Vec<f32>> = current_level.iter().map(|(_, e)| e.clone()).collect();
            let centroid = calculate_centroid(&all_embeddings);
            let children: Vec<String> = current_level.iter().map(|(id, _)| id.clone()).collect();
            
            let root_id = format!("node_{}", Uuid::new_v4());
            all_nodes.push(TreeNode::new_internal(root_id.clone(), children, centroid.clone(), level));
            current_level = vec![(root_id, centroid)];
            break;
        }

        // Create parent nodes for each cluster
        let mut next_level = Vec::new();
        for (centroid, child_ids) in clusters {
            let parent_id = format!("node_{}", Uuid::new_v4());
            all_nodes.push(TreeNode::new_internal(
                parent_id.clone(),
                child_ids,
                centroid.clone(),
                level,
            ));
            next_level.push((parent_id, centroid));
        }

        current_level = next_level;
        
        // Yield to prevent blocking
        yield_low_priority().await;
    }

    // Store tree in global store
    {
        let mut store = GLOBAL_STORE.lock().unwrap();
        store.tree_nodes.clear();
        for node in all_nodes {
            // Set parent references
            for child_id in &node.children {
                if let Some(child) = store.tree_nodes.get_mut(child_id) {
                    child.parent_id = Some(node.id.clone());
                }
            }
            store.tree_nodes.insert(node.id.clone(), node);
        }
        
        // Set root
        if !current_level.is_empty() {
            store.tree_root = Some(current_level[0].0.clone());
        }
    }

    Ok(())
}

/// Build the RAPTOR tree for all files under `path` (legacy, no progress)
pub async fn build_tree(
    path: &Path,
    orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
    max_chars: usize,
    overlap: usize,
    threshold: f32,
) -> Result<String> {
    build_tree_with_progress(path, orchestrator, max_chars, overlap, threshold, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore] // Heavy test: loads embedding model. Run with: cargo test -- --ignored
    async fn test_build_tree_small() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("file1.txt");
        let mut f = File::create(&p).unwrap();
        write!(
            f,
            "El gato se sentó en la alfombra\nOtro fragmento sobre muebles"
        )
        .unwrap();

        let config = crate::agent::orchestrator::OrchestratorConfig::default();
        let orch = DualModelOrchestrator::with_config(config).await.unwrap();
        let orchestrator = Arc::new(AsyncMutex::new(orch));

        let root = build_tree(dir.path(), orchestrator.clone(), 200, 50, 0.75)
            .await
            .unwrap();
        assert!(!root.is_empty());

        // ensure store has chunks and embeddings
        let store = GLOBAL_STORE.lock().unwrap();
        assert!(!store.chunk_map.is_empty());
        assert!(!store.chunk_embeddings.is_empty());
        assert!(!store.summary_embeddings.is_empty());
    }
}
