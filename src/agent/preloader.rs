//! Context Preloader - Sistema de pre-carga de contexto para reducir latencia
//!
//! Pre-carga el árbol RAPTOR y embeddings en memoria durante el startup
//! para reducir la latencia del first-query de 5s a <500ms.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex as AsyncMutex;

/// Estado del preloader
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloaderState {
    /// No iniciado
    Idle,
    /// Cargando en background
    Loading,
    /// Carga completa
    Ready,
    /// Error durante la carga
    Failed,
}

/// Cache de embeddings con política LRU
#[derive(Debug, Clone, Default)]
pub struct EmbeddingCache {
    /// Embeddings cacheados (chunk_id → embedding vector)
    embeddings: HashMap<String, Vec<f32>>,
    /// Orden de acceso para LRU (más reciente al final)
    access_order: Vec<String>,
    /// Tamaño máximo de la cache (número de embeddings)
    max_size: usize,
    /// Hits de la cache
    hits: usize,
    /// Misses de la cache
    misses: usize,
}

impl EmbeddingCache {
    /// Crea una nueva cache con tamaño máximo
    pub fn new(max_size: usize) -> Self {
        Self {
            embeddings: HashMap::new(),
            access_order: Vec::new(),
            max_size,
            hits: 0,
            misses: 0,
        }
    }

    /// Obtiene un embedding de la cache
    pub fn get(&mut self, chunk_id: &str) -> Option<Vec<f32>> {
        if let Some(embedding) = self.embeddings.get(chunk_id) {
            // Actualizar orden de acceso (LRU)
            if let Some(pos) = self.access_order.iter().position(|id| id == chunk_id) {
                self.access_order.remove(pos);
            }
            self.access_order.push(chunk_id.to_string());
            
            self.hits += 1;
            Some(embedding.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Inserta un embedding en la cache
    pub fn insert(&mut self, chunk_id: String, embedding: Vec<f32>) {
        // Si ya existe, actualizar orden
        if self.embeddings.contains_key(&chunk_id) {
            if let Some(pos) = self.access_order.iter().position(|id| id == &chunk_id) {
                self.access_order.remove(pos);
            }
            self.access_order.push(chunk_id.clone());
            self.embeddings.insert(chunk_id, embedding);
            return;
        }

        // Si la cache está llena, eliminar el menos reciente (LRU)
        if self.embeddings.len() >= self.max_size {
            if let Some(oldest) = self.access_order.first() {
                let oldest = oldest.clone();
                self.embeddings.remove(&oldest);
                self.access_order.remove(0);
            }
        }

        // Insertar nuevo embedding
        self.embeddings.insert(chunk_id.clone(), embedding);
        self.access_order.push(chunk_id);
    }

    /// Limpia la cache
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.access_order.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Tamaño actual de la cache
    pub fn size(&self) -> usize {
        self.embeddings.len()
    }

    /// Tasa de hits de la cache (0.0 - 1.0)
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }

    /// Uso de memoria estimado (MB)
    pub fn memory_usage_mb(&self) -> f32 {
        // Cada embedding: ~1536 floats * 4 bytes = 6KB promedio
        // Más overhead de HashMap y Vec
        let embedding_bytes = self.embeddings.len() * 6144; // 6KB por embedding
        let access_order_bytes = self.access_order.len() * 32; // ~32 bytes por String
        (embedding_bytes + access_order_bytes) as f32 / (1024.0 * 1024.0)
    }
}

/// Cache RAPTOR con estructura del árbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaptorCache {
    /// Embeddings de chunks y nodos
    #[serde(skip)]
    pub embeddings: EmbeddingCache,
    /// IDs de chunks disponibles
    pub chunk_ids: Vec<String>,
    /// Timestamp de última actualización
    pub last_updated: SystemTime,
    /// Número de chunks cargados
    pub chunks_loaded: usize,
}

impl RaptorCache {
    /// Crea una nueva cache RAPTOR vacía
    pub fn new(max_embeddings: usize) -> Self {
        Self {
            embeddings: EmbeddingCache::new(max_embeddings),
            chunk_ids: Vec::new(),
            last_updated: SystemTime::now(),
            chunks_loaded: 0,
        }
    }

    /// Verifica si la cache está lista
    pub fn is_ready(&self) -> bool {
        self.chunks_loaded > 0
    }

    /// Actualiza el timestamp
    pub fn touch(&mut self) {
        self.last_updated = SystemTime::now();
    }
}

/// Preloader de contexto
pub struct ContextPreloader {
    /// Cache RAPTOR
    raptor_cache: Arc<AsyncMutex<RaptorCache>>,
    /// Estado actual del preloader
    state: Arc<AsyncMutex<PreloaderState>>,
    /// Progreso de carga (0-100)
    progress: Arc<AtomicUsize>,
    /// Flag de cancelación
    cancel_flag: Arc<AtomicBool>,
    /// Máximo de embeddings en memoria
    max_embeddings: usize,
    /// Pre-cargar en startup
    preload_on_startup: bool,
}

impl ContextPreloader {
    /// Crea un nuevo preloader
    pub fn new(max_embeddings: usize, preload_on_startup: bool) -> Self {
        Self {
            raptor_cache: Arc::new(AsyncMutex::new(RaptorCache::new(max_embeddings))),
            state: Arc::new(AsyncMutex::new(PreloaderState::Idle)),
            progress: Arc::new(AtomicUsize::new(0)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            max_embeddings,
            preload_on_startup,
        }
    }

    /// Crea un preloader con configuración por defecto
    pub fn default() -> Self {
        Self::new(10000, true) // 10k embeddings max (~60MB)
    }

    /// Obtiene el estado actual
    pub async fn state(&self) -> PreloaderState {
        *self.state.lock().await
    }

    /// Obtiene el progreso actual (0-100)
    pub fn progress(&self) -> usize {
        self.progress.load(Ordering::Relaxed)
    }

    /// Verifica si está listo
    pub async fn is_ready(&self) -> bool {
        matches!(self.state().await, PreloaderState::Ready)
    }

    /// Cancela la carga en progreso
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Inicia la pre-carga de RAPTOR en background
    pub async fn preload_async(&self) -> Result<()> {
        // Verificar si ya está cargando o listo
        {
            let state = self.state.lock().await;
            if *state == PreloaderState::Loading || *state == PreloaderState::Ready {
                return Ok(());
            }
        }

        // Cambiar estado a Loading
        {
            let mut state = self.state.lock().await;
            *state = PreloaderState::Loading;
        }

        self.progress.store(0, Ordering::Relaxed);
        self.cancel_flag.store(false, Ordering::SeqCst);

        // Clonar referencias para el background task
        let raptor_cache = Arc::clone(&self.raptor_cache);
        let state = Arc::clone(&self.state);
        let progress = Arc::clone(&self.progress);
        let cancel_flag = Arc::clone(&self.cancel_flag);

        // Spawn background task
        tokio::spawn(async move {
            // Simular carga de RAPTOR (en producción, cargar desde GLOBAL_STORE)
            let result = Self::load_raptor_data(
                &raptor_cache,
                &progress,
                &cancel_flag,
            ).await;

            // Actualizar estado
            let mut state_guard = state.lock().await;
            *state_guard = if result.is_ok() {
                PreloaderState::Ready
            } else {
                PreloaderState::Failed
            };
        });

        Ok(())
    }

    /// Carga datos de RAPTOR (simulado para tests)
    async fn load_raptor_data(
        raptor_cache: &Arc<AsyncMutex<RaptorCache>>,
        progress: &Arc<AtomicUsize>,
        cancel_flag: &Arc<AtomicBool>,
    ) -> Result<()> {
        use crate::raptor::persistence::GLOBAL_STORE;

        // Obtener chunks del GLOBAL_STORE
        let chunk_ids: Vec<String> = {
            let store = GLOBAL_STORE.lock().unwrap();
            store.chunk_map.keys().cloned().collect()
        };

        let total_chunks = chunk_ids.len();
        if total_chunks == 0 {
            return Ok(()); // Sin chunks para cargar
        }

        // Cargar embeddings en batches
        let batch_size = 100;
        for (batch_idx, chunk_batch) in chunk_ids.chunks(batch_size).enumerate() {
            // Verificar cancelación
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Preload cancelled"));
            }

            // Simular carga de embeddings (en producción, obtener del GLOBAL_STORE)
            for chunk_id in chunk_batch {
                let embedding = {
                    let store = GLOBAL_STORE.lock().unwrap();
                    store.chunk_embeddings.get(chunk_id).cloned()
                };

                if let Some(embedding) = embedding {
                    let mut cache = raptor_cache.lock().await;
                    cache.embeddings.insert(chunk_id.clone(), embedding);
                    cache.chunks_loaded += 1;
                }
            }

            // Actualizar progreso
            let current_progress = ((batch_idx + 1) * batch_size * 100) / total_chunks;
            progress.store(current_progress.min(100), Ordering::Relaxed);

            // Pequeña pausa para no bloquear (yield)
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // Actualizar cache
        {
            let mut cache = raptor_cache.lock().await;
            cache.chunk_ids = chunk_ids;
            cache.touch();
        }

        progress.store(100, Ordering::Relaxed);
        Ok(())
    }

    /// Obtiene un embedding de la cache (con fallback a GLOBAL_STORE)
    pub async fn get_embedding(&self, chunk_id: &str) -> Option<Vec<f32>> {
        let mut cache = self.raptor_cache.lock().await;
        
        // Intentar obtener de la cache
        if let Some(embedding) = cache.embeddings.get(chunk_id) {
            return Some(embedding);
        }

        // Fallback: cargar de GLOBAL_STORE
        let embedding = {
            let store = crate::raptor::persistence::GLOBAL_STORE.lock().unwrap();
            store.chunk_embeddings.get(chunk_id).cloned()
        };

        // Agregar a la cache si se encontró
        if let Some(ref emb) = embedding {
            cache.embeddings.insert(chunk_id.to_string(), emb.clone());
        }

        embedding
    }

    /// Obtiene estadísticas de la cache
    pub async fn cache_stats(&self) -> PreloaderCacheStats {
        let cache = self.raptor_cache.lock().await;
        PreloaderCacheStats {
            size: cache.embeddings.size(),
            max_size: self.max_embeddings,
            hit_rate: cache.embeddings.hit_rate(),
            memory_mb: cache.embeddings.memory_usage_mb(),
            chunks_loaded: cache.chunks_loaded,
        }
    }

    /// Limpia la cache
    pub async fn clear_cache(&self) {
        let mut cache = self.raptor_cache.lock().await;
        cache.embeddings.clear();
        cache.chunks_loaded = 0;
        
        let mut state = self.state.lock().await;
        *state = PreloaderState::Idle;
        
        self.progress.store(0, Ordering::Relaxed);
    }
}

/// Estadísticas de la cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloaderCacheStats {
    /// Número de embeddings en cache
    pub size: usize,
    /// Tamaño máximo de la cache
    pub max_size: usize,
    /// Tasa de hits (0.0 - 1.0)
    pub hit_rate: f32,
    /// Uso de memoria (MB)
    pub memory_mb: f32,
    /// Chunks cargados
    pub chunks_loaded: usize,
}

impl PreloaderCacheStats {
    /// Genera un reporte legible
    pub fn report(&self) -> String {
        format!(
            "Cache: {}/{} embeddings ({:.1}% full), {:.1}MB RAM, {:.1}% hit rate, {} chunks",
            self.size,
            self.max_size,
            (self.size as f32 / self.max_size as f32) * 100.0,
            self.memory_mb,
            self.hit_rate * 100.0,
            self.chunks_loaded
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_cache_insert_get() {
        let mut cache = EmbeddingCache::new(3);
        
        cache.insert("chunk1".to_string(), vec![1.0, 2.0, 3.0]);
        cache.insert("chunk2".to_string(), vec![4.0, 5.0, 6.0]);
        
        assert_eq!(cache.size(), 2);
        assert_eq!(cache.get("chunk1"), Some(vec![1.0, 2.0, 3.0]));
        assert_eq!(cache.get("chunk2"), Some(vec![4.0, 5.0, 6.0]));
        assert_eq!(cache.get("chunk3"), None);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = EmbeddingCache::new(2); // Solo 2 embeddings
        
        cache.insert("chunk1".to_string(), vec![1.0]);
        cache.insert("chunk2".to_string(), vec![2.0]);
        assert_eq!(cache.size(), 2);
        
        // Insertar tercero debería eliminar el más antiguo (chunk1)
        cache.insert("chunk3".to_string(), vec![3.0]);
        assert_eq!(cache.size(), 2);
        assert_eq!(cache.get("chunk1"), None); // Eliminado
        assert_eq!(cache.get("chunk2"), Some(vec![2.0]));
        assert_eq!(cache.get("chunk3"), Some(vec![3.0]));
    }

    #[test]
    fn test_lru_access_order() {
        let mut cache = EmbeddingCache::new(2);
        
        cache.insert("chunk1".to_string(), vec![1.0]);
        cache.insert("chunk2".to_string(), vec![2.0]);
        
        // Acceder a chunk1 (lo hace más reciente)
        let _ = cache.get("chunk1");
        
        // Insertar chunk3 debería eliminar chunk2 (menos reciente)
        cache.insert("chunk3".to_string(), vec![3.0]);
        assert_eq!(cache.get("chunk1"), Some(vec![1.0])); // Preservado
        assert_eq!(cache.get("chunk2"), None); // Eliminado
        assert_eq!(cache.get("chunk3"), Some(vec![3.0]));
    }

    #[test]
    fn test_cache_hit_rate() {
        let mut cache = EmbeddingCache::new(10);
        
        cache.insert("chunk1".to_string(), vec![1.0]);
        
        // 3 hits, 2 misses
        let _ = cache.get("chunk1"); // hit
        let _ = cache.get("chunk1"); // hit
        let _ = cache.get("chunk1"); // hit
        let _ = cache.get("chunk2"); // miss
        let _ = cache.get("chunk3"); // miss
        
        assert_eq!(cache.hit_rate(), 0.6); // 3/5 = 60%
    }

    #[tokio::test]
    async fn test_preloader_creation() {
        let preloader = ContextPreloader::new(100, true);
        assert_eq!(preloader.state().await, PreloaderState::Idle);
        assert_eq!(preloader.progress(), 0);
        assert!(!preloader.is_ready().await);
    }

    #[tokio::test]
    async fn test_async_preload() {
        let preloader = ContextPreloader::new(100, true);
        
        // Iniciar pre-carga
        preloader.preload_async().await.unwrap();
        
        // Esperar un momento para que inicie
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Debería estar cargando o listo
        let state = preloader.state().await;
        assert!(
            state == PreloaderState::Loading || state == PreloaderState::Ready
        );
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let preloader = ContextPreloader::new(100, true);
        
        // Agregar algo a la cache manualmente
        {
            let mut cache = preloader.raptor_cache.lock().await;
            cache.embeddings.insert("test".to_string(), vec![1.0, 2.0]);
            cache.chunks_loaded = 5;
        }
        
        // Limpiar cache
        preloader.clear_cache().await;
        
        let cache = preloader.raptor_cache.lock().await;
        assert_eq!(cache.embeddings.size(), 0);
        assert_eq!(cache.chunks_loaded, 0);
        assert_eq!(preloader.state().await, PreloaderState::Idle);
    }

    #[test]
    fn test_memory_usage_estimation() {
        let mut cache = EmbeddingCache::new(1000);
        
        // Agregar algunos embeddings
        for i in 0..10 {
            let embedding = vec![0.0_f32; 1536]; // Embedding típico
            cache.insert(format!("chunk{}", i), embedding);
        }
        
        let memory_mb = cache.memory_usage_mb();
        assert!(memory_mb > 0.0);
        assert!(memory_mb < 1.0); // 10 embeddings ~0.06MB
    }

    #[tokio::test]
    async fn test_cache_stats_report() {
        let preloader = ContextPreloader::new(100, false);
        
        {
            let mut cache = preloader.raptor_cache.lock().await;
            cache.embeddings.insert("test1".to_string(), vec![1.0]);
            cache.embeddings.insert("test2".to_string(), vec![2.0]);
            cache.chunks_loaded = 50;
        }
        
        let stats = preloader.cache_stats().await;
        let report = stats.report();
        
        assert!(report.contains("2/100"));
        assert!(report.contains("50 chunks"));
    }
}
