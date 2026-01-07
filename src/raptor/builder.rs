use crate::agent::orchestrator::DualModelOrchestrator;
use crate::embedding::EmbeddingEngine;
use crate::raptor::chunker::chunk_text;
use crate::raptor::clustering::cluster_by_threshold;
use crate::raptor::persistence::{load_cache_if_valid, save_cache, GLOBAL_STORE};
use crate::raptor::summarizer::{RecursiveSummarizer, SummaryNode};
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

    // Collect files quickly
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

    let mut total_chunks = 0usize;

    // Read files and create chunks - NO embeddings (very fast)
    for entry in files.iter() {
        let file_path = entry.path();

        if let Ok(text) = std::fs::read_to_string(file_path) {
            let chunks = chunk_text(&text, max_chars, overlap);
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
pub async fn build_tree_with_progress(
    path: &Path,
    orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
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

    // If we have a complete RAPTOR index, use it
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
        let store = GLOBAL_STORE.lock().unwrap();
        let root = store.nodes.keys().next().cloned().unwrap_or_default();
        return Ok(root);
    }

    let embedder = EmbeddingEngine::new().await?;
    let summarizer = RecursiveSummarizer::new(orchestrator.clone(), max_chars);

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
    let mut chunk_embeddings: Vec<(String, Vec<f32>)> = Vec::new();

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

    // Batch embed chunks - larger batch for speed
    let batch_size = 128;
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
                    detail: format!("Procesando chunks {}-{}", i + 1, end),
                })
                .await;
        }

        let slice = &chunk_texts[i..end];
        let text_refs: Vec<&str> = slice.iter().map(|(_, t)| t.as_str()).collect();
        let emb_batch = embedder.embed_batch(text_refs).await?;
        for (j, emb) in emb_batch.into_iter().enumerate() {
            let id = slice[j].0.clone();
            chunk_embeddings.push((id.clone(), emb.clone()));
            {
                let mut store = GLOBAL_STORE.lock().unwrap();
                store.insert_chunk_embedding(id.clone(), emb);
            }
        }
        i = end;
    }

    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Clustering".to_string(),
                current: 0,
                total: 1,
                detail: "Agrupando chunks similares...".to_string(),
            })
            .await;
    }

    // Initial clustering
    let clusters = cluster_by_threshold(&chunk_embeddings, threshold);
    let total_clusters = clusters.len();

    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Resumiendo".to_string(),
                current: 0,
                total: total_clusters,
                detail: format!("{} grupos a resumir", total_clusters),
            })
            .await;
    }

    // For each cluster create a summary node
    let mut parent_ids = Vec::new();
    for (cluster_idx, cluster) in clusters.iter().enumerate() {
        // Yield to let other tasks run - low priority background indexing
        yield_low_priority().await;

        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Resumiendo".to_string(),
                    current: cluster_idx + 1,
                    total: total_clusters,
                    detail: format!("Grupo {} de {}", cluster_idx + 1, total_clusters),
                })
                .await;
        }

        let mut texts = Vec::new();
        for chunk_id in cluster.iter() {
            let store = GLOBAL_STORE.lock().unwrap();
            if let Some(c) = store.get_chunk(chunk_id) {
                texts.push(c.clone());
            }
        }

        let summary = summarizer.summarize_group(&texts).await?;
        let node = SummaryNode::new(summary.clone(), cluster.clone(), false);
        let pid = node.id.clone();
        {
            let mut store = GLOBAL_STORE.lock().unwrap();
            store.insert_node(node);
        }
        let s_emb = embedder.embed_text(&summary).await?;
        {
            let mut store = GLOBAL_STORE.lock().unwrap();
            store.insert_summary_embedding(pid.clone(), s_emb);
        }
        parent_ids.push(pid);
    }

    // Recursively summarize until single root
    let mut current_parents = parent_ids;
    let mut level = 1;
    while current_parents.len() > 1 {
        if let Some(ref tx) = progress_tx {
            let _ = tx
                .send(RaptorBuildProgress {
                    stage: "Jerarquía".to_string(),
                    current: level,
                    total: level + 1,
                    detail: format!("Nivel {} ({} nodos)", level, current_parents.len()),
                })
                .await;
        }
        level += 1;

        let mut parent_embeddings = Vec::new();
        for pid in current_parents.iter() {
            let summary = {
                let store = GLOBAL_STORE.lock().unwrap();
                store.get_node(pid).map(|n| n.summary.clone())
            };

            if let Some(summary_text) = summary {
                let emb = embedder.embed_text(&summary_text).await?;
                parent_embeddings.push((pid.clone(), emb));
            }
        }

        let clusters = cluster_by_threshold(&parent_embeddings, threshold);
        let mut new_parents = Vec::new();

        for cluster in clusters {
            let mut texts = Vec::new();
            let mut children = Vec::new();
            for pid in cluster.iter() {
                let node_summary = {
                    let store = GLOBAL_STORE.lock().unwrap();
                    store.get_node(pid).map(|n| n.summary.clone())
                };

                if let Some(summary) = node_summary {
                    texts.push(summary);
                    children.push(pid.clone());
                }
            }

            let summary = summarizer.summarize_group(&texts).await?;
            let node = SummaryNode::new(summary.clone(), children.clone(), false);
            let nid = node.id.clone();
            {
                let mut store = GLOBAL_STORE.lock().unwrap();
                store.insert_node(node);
            }
            let s_emb = embedder.embed_text(&summary).await?;
            {
                let mut store = GLOBAL_STORE.lock().unwrap();
                store.insert_summary_embedding(nid.clone(), s_emb);
            }
            new_parents.push(nid);
        }

        current_parents = new_parents;
    }

    if let Some(ref tx) = progress_tx {
        let _ = tx
            .send(RaptorBuildProgress {
                stage: "Guardando".to_string(),
                current: 1,
                total: 1,
                detail: "Guardando caché...".to_string(),
            })
            .await;
    }

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
                detail: "Índice RAPTOR listo".to_string(),
            })
            .await;
    }

    let root = current_parents
        .into_iter()
        .next()
        .unwrap_or_else(String::new);
    Ok(root)
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
