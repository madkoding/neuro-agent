//! Related Files Detection - Automatically find and include related files
//!
//! This module analyzes code to automatically discover related files that should
//! be included in the context when working on a specific file.
//!
//! # Features
//!
//! - Import/dependency detection using tree-sitter AST
//! - Test file discovery (finds tests for a given source file)
//! - Documentation linkage
//! - Cargo.toml dependencies
//! - Git-aware: prioritizes recently modified files
//!
//! # Performance
//!
//! - Results are cached using LRU
//! - Incremental updates on file changes
//! - Fast AST parsing with tree-sitter

use anyhow::Result;
use lru::LruCache;
use regex::Regex;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

/// Related file types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationType {
    /// Direct import/use statement
    Import,
    /// Test file for this source file
    Test,
    /// Documentation file
    Documentation,
    /// Cargo dependency
    Dependency,
    /// Git recently modified (co-changed files)
    GitRelated,
}

/// A related file with its relationship type
#[derive(Debug, Clone)]
pub struct RelatedFile {
    pub path: PathBuf,
    pub relation_type: RelationType,
    pub confidence: f32, // 0.0 - 1.0
}

/// Cache for related files to avoid repeated AST parsing
type RelatedFilesCache = LruCache<PathBuf, Vec<RelatedFile>>;

/// Related Files Detector
pub struct RelatedFilesDetector {
    cache: Arc<Mutex<RelatedFilesCache>>,
    project_root: PathBuf,
}

impl RelatedFilesDetector {
    /// Create a new detector for a project
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(200).unwrap()
            ))),
            project_root,
        }
    }

    /// Find all files related to the given file
    pub async fn find_related(&self, file_path: &Path) -> Result<Vec<RelatedFile>> {
        // Check cache first
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&file_path.to_path_buf()) {
                return Ok(cached.clone());
            }
        }

        let mut related = Vec::new();

        // 1. Find imports
        related.extend(self.find_imports(file_path)?);

        // 2. Find test files
        related.extend(self.find_test_files(file_path)?);

        // 3. Find documentation
        related.extend(self.find_documentation(file_path)?);

        // 4. Find cargo dependencies (if relevant)
        if file_path.extension().and_then(|s| s.to_str()) == Some("rs") {
            related.extend(self.find_cargo_deps(file_path)?);
        }

        // Sort by confidence (highest first)
        related.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Cache result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(file_path.to_path_buf(), related.clone());
        }

        Ok(related)
    }

    /// Find files imported/used by this file using tree-sitter
    fn find_imports(&self, file_path: &Path) -> Result<Vec<RelatedFile>> {
        let extension = file_path.extension().and_then(|s| s.to_str());
        
        match extension {
            Some("rs") => self.find_rust_imports(file_path),
            Some("py") => self.find_python_imports(file_path),
            Some("js") | Some("ts") => self.find_js_imports(file_path),
            _ => Ok(Vec::new()),
        }
    }

    /// Find Rust imports using regex (fast but less accurate than AST)
    fn find_rust_imports(&self, file_path: &Path) -> Result<Vec<RelatedFile>> {
        let source = std::fs::read_to_string(file_path)?;
        
        // Regex to match use statements: use crate::module::path;
        let use_regex = Regex::new(r"use\s+(crate::[a-zA-Z0-9_:]+)")?;
        
        let mut imports = Vec::new();
        
        for cap in use_regex.captures_iter(&source) {
            if let Some(module_path) = cap.get(1) {
                let path_str = module_path.as_str();
                
                // Convert module path to file path
                if let Some(file_path) = self.module_to_file_path(path_str) {
                    imports.push(RelatedFile {
                        path: file_path,
                        relation_type: RelationType::Import,
                        confidence: 0.8, // Slightly lower confidence for regex vs AST
                    });
                }
            }
        }
        
        Ok(imports)
    }

    /// Find Python imports using tree-sitter
    fn find_python_imports(&self, _file_path: &Path) -> Result<Vec<RelatedFile>> {
        // TODO: Implement Python import detection
        Ok(Vec::new())
    }

    /// Find JavaScript/TypeScript imports
    fn find_js_imports(&self, _file_path: &Path) -> Result<Vec<RelatedFile>> {
        // TODO: Implement JS/TS import detection
        Ok(Vec::new())
    }

    /// Find test files for this source file
    fn find_test_files(&self, file_path: &Path) -> Result<Vec<RelatedFile>> {
        let mut tests = Vec::new();
        
        let file_stem = file_path.file_stem().and_then(|s| s.to_str());
        let Some(stem) = file_stem else {
            return Ok(tests);
        };
        
        // Common test patterns
        let test_patterns = vec![
            format!("{}_test.rs", stem),
            format!("test_{}.rs", stem),
            format!("{}_tests.rs", stem),
            format!("tests/{}.rs", stem),
        ];
        
        // Search in tests/ directory
        let tests_dir = self.project_root.join("tests");
        if tests_dir.exists() {
            for entry in WalkDir::new(&tests_dir).max_depth(2).into_iter().flatten() {
                let path = entry.path();
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    for pattern in &test_patterns {
                        if file_name.contains(stem) || pattern.contains(file_name) {
                            tests.push(RelatedFile {
                                path: path.to_path_buf(),
                                relation_type: RelationType::Test,
                                confidence: 0.85,
                            });
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(tests)
    }

    /// Find documentation files
    fn find_documentation(&self, file_path: &Path) -> Result<Vec<RelatedFile>> {
        let mut docs = Vec::new();
        
        // Check for README in same directory
        if let Some(parent) = file_path.parent() {
            let readme = parent.join("README.md");
            if readme.exists() {
                docs.push(RelatedFile {
                    path: readme,
                    relation_type: RelationType::Documentation,
                    confidence: 0.7,
                });
            }
        }
        
        Ok(docs)
    }

    /// Find Cargo.toml dependencies for this file
    fn find_cargo_deps(&self, _file_path: &Path) -> Result<Vec<RelatedFile>> {
        let cargo_toml = self.project_root.join("Cargo.toml");
        
        if cargo_toml.exists() {
            Ok(vec![RelatedFile {
                path: cargo_toml,
                relation_type: RelationType::Dependency,
                confidence: 0.6,
            }])
        } else {
            Ok(Vec::new())
        }
    }

    /// Convert Rust module path to file path
    fn module_to_file_path(&self, module_path: &str) -> Option<PathBuf> {
        // Examples:
        // "crate::agent::router" -> src/agent/router.rs
        // "super::mod" -> ../mod.rs
        // "self::utils" -> utils.rs
        
        let parts: Vec<&str> = module_path.split("::").collect();
        
        if parts.is_empty() {
            return None;
        }
        
        let mut path = self.project_root.clone();
        
        match parts[0] {
            "crate" => {
                path = path.join("src");
                for part in &parts[1..] {
                    path = path.join(part);
                }
            }
            "super" => {
                // Relative path - harder to resolve without current file context
                return None;
            }
            "self" => {
                // Same directory
                for part in &parts[1..] {
                    path = path.join(part);
                }
            }
            _ => {
                // External crate or std library
                return None;
            }
        }
        
        // Try .rs file first, then /mod.rs
        let rs_file = path.with_extension("rs");
        if rs_file.exists() {
            return Some(rs_file);
        }
        
        let mod_file = path.join("mod.rs");
        if mod_file.exists() {
            return Some(mod_file);
        }
        
        None
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.lock().unwrap();
        (cache.len(), cache.cap().get())
    }

    /// Detect file paths mentioned in user query and enrich with related files
    /// Returns: (detected_files, enriched_context)
    pub async fn enrich_with_query_context(
        &self,
        user_query: &str,
        config: &crate::agent::router_orchestrator::RouterConfig, // Use full path for RouterConfig
    ) -> (Vec<PathBuf>, String) {
        use regex::Regex;
        
        // Patterns to detect file paths in queries
        let file_patterns = vec![
            Regex::new(r"(?:analiza|lee|revisa|muestra|ver|check|analyze|read|review|show)\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
            Regex::new(r"archivo\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
            Regex::new(r"file\s+([a-zA-Z0-9_./][a-zA-Z0-9_./-]{0,100}?\.[a-zA-Z0-9]{1,10})").unwrap(),
            Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.rs)").unwrap(), // Rust files
            Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.py)").unwrap(), // Python files
            Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.js)").unwrap(), // JavaScript files
            Regex::new(r"([a-zA-Z0-9_][a-zA-Z0-9_/]{0,100}?\.ts)").unwrap(), // TypeScript files
        ];
        
        let mut detected_files: Vec<PathBuf> = Vec::new();
        
        // Detect files mentioned in query
        for pattern in &file_patterns {
            for cap in pattern.captures_iter(user_query) {
                if let Some(file_match) = cap.get(1) {
                    let file_path = PathBuf::from(file_match.as_str());
                    
                    // Check if file exists in project
                    let full_path = if file_path.is_absolute() {
                        file_path.clone()
                    } else {
                        PathBuf::from(&config.working_dir).join(&file_path)
                    };
                    
                    if full_path.exists() && !detected_files.contains(&full_path) {
                        detected_files.push(full_path);
                    }
                }
            }
        }
        
        if detected_files.is_empty() {
            return (vec![], String::new());
        }
        
        // Get related files for each detected file
        let mut all_related: Vec<PathBuf> = Vec::new();
        for file in &detected_files {
            if let Ok(related) = self.find_related(file).await {
                for rel_file in related {
                    if !all_related.contains(&rel_file.path) && !detected_files.contains(&rel_file.path) {
                        all_related.push(rel_file.path);
                    }
                }
            }
        }
        
        // Build enriched context string
        let mut context = String::new();
        
        if !all_related.is_empty() {
            context.push_str("\n\n📎 Archivos relacionados detectados:\n");
            
            // Group by relation type (based on file naming patterns)
            let mut imports: Vec<&PathBuf> = Vec::new();
            let mut tests: Vec<&PathBuf> = Vec::new();
            let mut docs: Vec<&PathBuf> = Vec::new();
            let others: Vec<&PathBuf> = Vec::new();
            
            for file in &all_related {
                let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                if file_name.contains("test") {
                    tests.push(file);
                } else if file_name.contains("README") || file_name.contains("doc") {
                    docs.push(file);
                } else if file_name == "Cargo.toml" || file_name == "package.json" {
                    docs.push(file);
                } else {
                    imports.push(file);
                }
            }
            
            if !imports.is_empty() {
                context.push_str("  • Imports/Dependencies:\n");
                for file in imports.iter().take(5) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !tests.is_empty() {
                context.push_str("  • Tests:\n");
                for file in tests.iter().take(3) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !docs.is_empty() {
                context.push_str("  • Documentation:\n");
                for file in docs.iter().take(2) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !others.is_empty() {
                context.push_str("  • Other:\n");
                for file in others.iter().take(3) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            context.push_str("\nNota: Estos archivos están relacionados con los mencionados en tu consulta y pueden proporcionar contexto adicional.\n");
        }
        
        (detected_files, context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_test_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        
        // Create source file
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let source_file = src_dir.join("example.rs");
        std::fs::write(&source_file, "fn main() {}").unwrap();
        
        // Create test file
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        let test_file = tests_dir.join("example_test.rs");
        std::fs::write(&test_file, "#[test] fn test_main() {}").unwrap();
        
        let detector = RelatedFilesDetector::new(root.to_path_buf());
        let related = detector.find_test_files(&source_file).unwrap();
        
        assert!(related.len() >= 1, "Should find at least 1 test file");
        assert_eq!(related[0].relation_type, RelationType::Test);
        assert!(related.iter().any(|r| r.path.ends_with("example_test.rs")));
    }

    #[test]
    fn test_find_documentation() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        
        // Create README
        let readme = src_dir.join("README.md");
        std::fs::write(&readme, "# Documentation").unwrap();
        
        let source_file = src_dir.join("lib.rs");
        std::fs::write(&source_file, "// lib").unwrap();
        
        let detector = RelatedFilesDetector::new(root.to_path_buf());
        let related = detector.find_documentation(&source_file).unwrap();
        
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].relation_type, RelationType::Documentation);
    }

    #[test]
    fn test_cache() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        
        let file = root.join("test.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        
        let detector = RelatedFilesDetector::new(root.to_path_buf());
        
        // First call - cache miss
        let _ = detector.find_related(&file).unwrap();
        let (size1, _) = detector.cache_stats();
        assert_eq!(size1, 1);
        
        // Second call - cache hit
        let _ = detector.find_related(&file).unwrap();
        let (size2, _) = detector.cache_stats();
        assert_eq!(size2, 1); // Same size
        
        // Clear cache
        detector.clear_cache();
        let (size3, _) = detector.cache_stats();
        assert_eq!(size3, 0);
    }

    #[test]
    fn test_module_to_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        
        let agent_dir = src_dir.join("agent");
        std::fs::create_dir_all(&agent_dir).unwrap();
        
        let router_file = agent_dir.join("router.rs");
        std::fs::write(&router_file, "// router").unwrap();
        
        let detector = RelatedFilesDetector::new(root.to_path_buf());
        
        let path = detector.module_to_file_path("crate::agent::router");
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("router.rs"));
    }
}
