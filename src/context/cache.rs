//! Project context cache - Persistent cache for project analysis

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// Cached project context to avoid repeated analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContextCache {
    /// Project root path
    pub root: PathBuf,
    /// Cached project structure
    pub structure: Option<CachedEntry<ProjectStructure>>,
    /// Cached dependencies analysis
    pub dependencies: Option<CachedEntry<DependenciesInfo>>,
    /// Cached file analyses (path -> analysis)
    pub file_analyses: HashMap<PathBuf, CachedEntry<FileAnalysis>>,
    /// Cached search indices
    pub search_index: Option<CachedEntry<SearchIndex>>,
}

/// A cached entry with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry<T> {
    pub data: T,
    pub cached_at: SystemTime,
    pub expires_at: Option<SystemTime>,
}

/// Project structure information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub language: String,
    pub framework: Option<String>,
    pub entry_points: Vec<PathBuf>,
    pub important_dirs: Vec<PathBuf>,
    pub total_files: usize,
    pub total_lines: usize,
}

/// Dependencies information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependenciesInfo {
    pub dependencies: Vec<Dependency>,
    pub dev_dependencies: Vec<Dependency>,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub source: String,
}

/// File analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub symbols: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub complexity: u32,
    pub lines: usize,
}

/// Search index for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    pub indexed_files: Vec<PathBuf>,
    pub index_version: u32,
}

impl<T> CachedEntry<T> {
    pub fn new(data: T, ttl: Option<Duration>) -> Self {
        let now = SystemTime::now();
        Self {
            data,
            cached_at: now,
            expires_at: ttl.map(|d| now + d),
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            SystemTime::now() > expires_at
        } else {
            false
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }
}

impl ProjectContextCache {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            structure: None,
            dependencies: None,
            file_analyses: HashMap::new(),
            search_index: None,
        }
    }

    /// Get project structure, if cached and valid
    pub fn get_structure(&self) -> Option<&ProjectStructure> {
        self.structure
            .as_ref()
            .filter(|e| e.is_valid())
            .map(|e| &e.data)
    }

    /// Cache project structure
    pub fn cache_structure(&mut self, structure: ProjectStructure, ttl: Duration) {
        self.structure = Some(CachedEntry::new(structure, Some(ttl)));
    }

    /// Get dependencies, if cached and valid
    pub fn get_dependencies(&self) -> Option<&DependenciesInfo> {
        self.dependencies
            .as_ref()
            .filter(|e| e.is_valid())
            .map(|e| &e.data)
    }

    /// Cache dependencies
    pub fn cache_dependencies(&mut self, deps: DependenciesInfo, ttl: Duration) {
        self.dependencies = Some(CachedEntry::new(deps, Some(ttl)));
    }

    /// Get file analysis, if cached and valid
    pub fn get_file_analysis(&self, path: &Path) -> Option<&FileAnalysis> {
        self.file_analyses
            .get(path)
            .filter(|e| e.is_valid())
            .map(|e| &e.data)
    }

    /// Cache file analysis
    pub fn cache_file_analysis(&mut self, path: PathBuf, analysis: FileAnalysis, ttl: Duration) {
        self.file_analyses
            .insert(path, CachedEntry::new(analysis, Some(ttl)));
    }

    /// Get search index, if cached and valid
    pub fn get_search_index(&self) -> Option<&SearchIndex> {
        self.search_index
            .as_ref()
            .filter(|e| e.is_valid())
            .map(|e| &e.data)
    }

    /// Cache search index
    pub fn cache_search_index(&mut self, index: SearchIndex, ttl: Duration) {
        self.search_index = Some(CachedEntry::new(index, Some(ttl)));
    }

    /// Invalidate all expired entries
    pub fn cleanup_expired(&mut self) {
        if self.structure.as_ref().map_or(false, |e| e.is_expired()) {
            self.structure = None;
        }
        if self.dependencies.as_ref().map_or(false, |e| e.is_expired()) {
            self.dependencies = None;
        }
        self.file_analyses.retain(|_, e| e.is_valid());
        if self.search_index.as_ref().map_or(false, |e| e.is_expired()) {
            self.search_index = None;
        }
    }

    /// Clear all cache
    pub fn clear(&mut self) {
        self.structure = None;
        self.dependencies = None;
        self.file_analyses.clear();
        self.search_index = None;
    }
}

/// Thread-safe project context cache manager
pub struct ProjectContextCacheManager {
    cache: Arc<RwLock<ProjectContextCache>>,
}

impl ProjectContextCacheManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            cache: Arc::new(RwLock::new(ProjectContextCache::new(root))),
        }
    }

    pub async fn get_structure(&self) -> Option<ProjectStructure> {
        let cache = self.cache.read().await;
        cache.get_structure().cloned()
    }

    pub async fn cache_structure(&self, structure: ProjectStructure, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.cache_structure(structure, ttl);
    }

    pub async fn get_dependencies(&self) -> Option<DependenciesInfo> {
        let cache = self.cache.read().await;
        cache.get_dependencies().cloned()
    }

    pub async fn cache_dependencies(&self, deps: DependenciesInfo, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.cache_dependencies(deps, ttl);
    }

    pub async fn get_file_analysis(&self, path: &Path) -> Option<FileAnalysis> {
        let cache = self.cache.read().await;
        cache.get_file_analysis(path).cloned()
    }

    pub async fn cache_file_analysis(&self, path: PathBuf, analysis: FileAnalysis, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.cache_file_analysis(path, analysis, ttl);
    }

    pub async fn get_search_index(&self) -> Option<SearchIndex> {
        let cache = self.cache.read().await;
        cache.get_search_index().cloned()
    }

    pub async fn cache_search_index(&self, index: SearchIndex, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.cache_search_index(index, ttl);
    }

    pub async fn cleanup_expired(&self) {
        let mut cache = self.cache.write().await;
        cache.cleanup_expired();
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl Clone for ProjectContextCacheManager {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}
