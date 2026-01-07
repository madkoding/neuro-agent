//! File indexer tool - Indexes and maintains context of project files

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use walkdir::WalkDir;

/// File information for indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
    pub modified: Option<u64>,
    pub file_type: FileType,
    pub language: Option<String>,
    pub line_count: Option<usize>,
    pub file_hash: String,
}

/// Type of file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileType {
    Source,
    Config,
    Documentation,
    Data,
    Binary,
    Other,
}

/// Project index containing all file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub root: PathBuf,
    pub files: Vec<FileInfo>,
    pub summary: ProjectSummary,
    pub indexed_at: u64,
}

/// Summary of the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_size: u64,
    pub languages: HashMap<String, LanguageStats>,
    pub structure: Vec<String>,
}

/// Statistics per language
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageStats {
    pub files: usize,
    pub lines: usize,
    pub size: u64,
}

/// File indexer tool
#[derive(Debug, Clone, Default)]
pub struct FileIndexerTool;

impl FileIndexerTool {
    pub const NAME: &'static str = "index_project";

    pub fn new() -> Self {
        Self
    }

    /// Index a project directory
    pub async fn index(&self, args: IndexProjectArgs) -> Result<ProjectIndex, IndexerError> {
        let root = PathBuf::from(&args.path);
        
        if !root.exists() {
            return Err(IndexerError::PathNotFound(args.path));
        }

        let mut files = Vec::new();
        let mut languages: HashMap<String, LanguageStats> = HashMap::new();
        let mut total_lines = 0usize;
        let mut total_size = 0u64;
        let mut structure = Vec::new();

        let max_depth = args.max_depth.unwrap_or(10);
        let ignore_patterns = args.ignore_patterns.clone().unwrap_or_else(|| {
            vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                "__pycache__".to_string(),
                ".venv".to_string(),
                "venv".to_string(),
                "dist".to_string(),
                "build".to_string(),
                ".next".to_string(),
                "*.pyc".to_string(),
                "*.lock".to_string(),
            ]
        });

        for entry in WalkDir::new(&root)
            .max_depth(max_depth)
            .into_iter()
            .filter_entry(|e| !should_ignore(e.path(), &ignore_patterns))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            if path.is_dir() {
                if let Ok(rel) = path.strip_prefix(&root) {
                    let rel_str = rel.to_string_lossy().to_string();
                    if !rel_str.is_empty() && entry.depth() <= 2 {
                        structure.push(format!("{}/", rel_str));
                    }
                }
                continue;
            }

            let metadata = match fs::metadata(path).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = metadata.len();
            total_size += size;

            let modified = metadata.modified().ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            let relative_path = path.strip_prefix(&root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let language = detect_language(path);
            let file_type = detect_file_type(path, &language);

            // Calculate file hash for cache invalidation
            let file_hash = if size < 10_000_000 {
                // Only hash files smaller than 10MB
                match fs::read(path).await {
                    Ok(content) => compute_file_hash(&content),
                    Err(_) => String::new(),
                }
            } else {
                // For large files, use a simple hash of metadata
                format!("{:x}", size ^ modified.unwrap_or(0))
            };

            // Count lines for text files
            let line_count = if file_type != FileType::Binary && size < 1_000_000 {
                count_lines(path).await.ok()
            } else {
                None
            };

            if let Some(lines) = line_count {
                total_lines += lines;
            }

            // Update language stats
            if let Some(ref lang) = language {
                let stats = languages.entry(lang.clone()).or_default();
                stats.files += 1;
                stats.size += size;
                if let Some(lines) = line_count {
                    stats.lines += lines;
                }
            }

            files.push(FileInfo {
                path: path.to_path_buf(),
                relative_path,
                size,
                modified,
                file_type,
                language,
                line_count,
                file_hash,
            });
        }

        // Sort structure
        structure.sort();

        let summary = ProjectSummary {
            total_files: files.len(),
            total_lines,
            total_size,
            languages,
            structure,
        };

        let indexed_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(ProjectIndex {
            root,
            files,
            summary,
            indexed_at,
        })
    }

    /// Search for files matching a pattern
    pub async fn search(&self, index: &ProjectIndex, pattern: &str) -> Vec<FileInfo> {
        let pattern_lower = pattern.to_lowercase();
        index.files.iter()
            .filter(|f| {
                f.relative_path.to_lowercase().contains(&pattern_lower) ||
                f.language.as_ref().map(|l| l.to_lowercase().contains(&pattern_lower)).unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Get files by language
    pub fn files_by_language<'a>(&self, index: &'a ProjectIndex, language: &str) -> Vec<&'a FileInfo> {
        let lang_lower = language.to_lowercase();
        index.files.iter()
            .filter(|f| f.language.as_ref().map(|l| l.to_lowercase() == lang_lower).unwrap_or(false))
            .collect()
    }

    /// Generate a context summary for the LLM
    pub fn generate_context_summary(&self, index: &ProjectIndex) -> String {
        let mut summary = String::new();
        
        summary.push_str(&format!("# Project: {}\n\n", index.root.display()));
        summary.push_str("## Statistics\n");
        summary.push_str(&format!("- Total files: {}\n", index.summary.total_files));
        summary.push_str(&format!("- Total lines: {}\n", index.summary.total_lines));
        summary.push_str(&format!("- Total size: {}\n\n", format_size(index.summary.total_size)));

        summary.push_str("## Languages\n");
        let mut langs: Vec<_> = index.summary.languages.iter().collect();
        langs.sort_by(|a, b| b.1.lines.cmp(&a.1.lines));
        for (lang, stats) in langs.iter().take(10) {
            summary.push_str(&format!("- {}: {} files, {} lines\n", lang, stats.files, stats.lines));
        }

        summary.push_str("\n## Project Structure\n");
        for dir in index.summary.structure.iter().take(20) {
            summary.push_str(&format!("- {}\n", dir));
        }

        summary.push_str("\n## Key Files\n");
        let key_files: Vec<_> = index.files.iter()
            .filter(|f| is_key_file(&f.relative_path))
            .take(15)
            .collect();
        for file in key_files {
            summary.push_str(&format!("- {} ({} lines)\n", 
                file.relative_path, 
                file.line_count.unwrap_or(0)
            ));
        }

        summary
    }

    /// Check if a file has changed by comparing hashes
    pub fn has_changed(&self, cached_hash: &str, current_hash: &str) -> bool {
        cached_hash != current_hash
    }
}

/// Arguments for indexing a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexProjectArgs {
    pub path: String,
    pub max_depth: Option<usize>,
    pub ignore_patterns: Option<Vec<String>>,
    pub include_hidden: Option<bool>,
}

/// Indexer errors
#[derive(Debug, thiserror::Error)]
pub enum IndexerError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to index: {0}")]
    IndexError(String),
}

fn should_ignore(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    for pattern in patterns {
        if let Some(ext) = pattern.strip_prefix("*.") {
            if path.extension().map(|e| e == ext).unwrap_or(false) {
                return true;
            }
        } else if file_name == *pattern || path_str.contains(pattern) {
            return true;
        }
    }

    // Always ignore hidden files/dirs
    if file_name.starts_with('.') && file_name != "." && file_name != ".." {
        return true;
    }

    false
}

fn detect_language(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    let name = path.file_name()?.to_str()?;

    // Check by filename first
    match name {
        "Cargo.toml" | "Cargo.lock" => return Some("Rust".to_string()),
        "package.json" | "package-lock.json" => return Some("JavaScript".to_string()),
        "tsconfig.json" => return Some("TypeScript".to_string()),
        "pyproject.toml" | "setup.py" | "requirements.txt" => return Some("Python".to_string()),
        "Makefile" | "makefile" => return Some("Make".to_string()),
        "Dockerfile" => return Some("Docker".to_string()),
        "docker-compose.yml" | "docker-compose.yaml" => return Some("Docker".to_string()),
        ".gitignore" | ".gitattributes" => return Some("Git".to_string()),
        _ => {}
    }

    // Check by extension
    match ext.to_lowercase().as_str() {
        "rs" => Some("Rust".to_string()),
        "py" | "pyi" => Some("Python".to_string()),
        "js" | "mjs" | "cjs" => Some("JavaScript".to_string()),
        "ts" | "mts" | "cts" => Some("TypeScript".to_string()),
        "tsx" | "jsx" => Some("React".to_string()),
        "go" => Some("Go".to_string()),
        "java" => Some("Java".to_string()),
        "kt" | "kts" => Some("Kotlin".to_string()),
        "c" | "h" => Some("C".to_string()),
        "cpp" | "cc" | "cxx" | "hpp" => Some("C++".to_string()),
        "cs" => Some("C#".to_string()),
        "rb" => Some("Ruby".to_string()),
        "php" => Some("PHP".to_string()),
        "swift" => Some("Swift".to_string()),
        "scala" => Some("Scala".to_string()),
        "lua" => Some("Lua".to_string()),
        "r" => Some("R".to_string()),
        "sql" => Some("SQL".to_string()),
        "sh" | "bash" | "zsh" => Some("Shell".to_string()),
        "ps1" => Some("PowerShell".to_string()),
        "html" | "htm" => Some("HTML".to_string()),
        "css" | "scss" | "sass" | "less" => Some("CSS".to_string()),
        "json" => Some("JSON".to_string()),
        "yaml" | "yml" => Some("YAML".to_string()),
        "toml" => Some("TOML".to_string()),
        "xml" => Some("XML".to_string()),
        "md" | "markdown" => Some("Markdown".to_string()),
        "txt" => Some("Text".to_string()),
        "vue" => Some("Vue".to_string()),
        "svelte" => Some("Svelte".to_string()),
        "elm" => Some("Elm".to_string()),
        "ex" | "exs" => Some("Elixir".to_string()),
        "erl" | "hrl" => Some("Erlang".to_string()),
        "hs" => Some("Haskell".to_string()),
        "ml" | "mli" => Some("OCaml".to_string()),
        "clj" | "cljs" | "cljc" => Some("Clojure".to_string()),
        "nim" => Some("Nim".to_string()),
        "zig" => Some("Zig".to_string()),
        "v" => Some("V".to_string()),
        "dart" => Some("Dart".to_string()),
        "proto" => Some("Protobuf".to_string()),
        "graphql" | "gql" => Some("GraphQL".to_string()),
        _ => None,
    }
}

fn detect_file_type(path: &Path, language: &Option<String>) -> FileType {
    let name = path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Config files
    if name.ends_with(".config") || name.ends_with(".conf") || 
       name.contains("config") || name.contains("settings") ||
       ext == "toml" || ext == "yaml" || ext == "yml" || ext == "ini" ||
       name == ".env" || name.starts_with(".env.") ||
       name == "dockerfile" || name.contains("docker-compose") {
        return FileType::Config;
    }

    // Documentation
    if ext == "md" || ext == "txt" || ext == "rst" || ext == "adoc" ||
       name == "readme" || name == "changelog" || name == "license" ||
       name == "contributing" || name == "authors" {
        return FileType::Documentation;
    }

    // Data files
    if ext == "json" || ext == "csv" || ext == "xml" || ext == "sql" {
        return FileType::Data;
    }

    // Binary files
    if ext == "exe" || ext == "dll" || ext == "so" || ext == "dylib" ||
       ext == "bin" || ext == "o" || ext == "a" || ext == "lib" ||
       ext == "png" || ext == "jpg" || ext == "jpeg" || ext == "gif" ||
       ext == "ico" || ext == "svg" || ext == "woff" || ext == "woff2" ||
       ext == "ttf" || ext == "eot" || ext == "pdf" || ext == "zip" ||
       ext == "tar" || ext == "gz" || ext == "rar" || ext == "7z" {
        return FileType::Binary;
    }

    // Source code
    if language.is_some() {
        return FileType::Source;
    }

    FileType::Other
}

async fn count_lines(path: &Path) -> Result<usize, std::io::Error> {
    let content = fs::read_to_string(path).await?;
    Ok(content.lines().count())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn is_key_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower == "readme.md" || lower == "cargo.toml" || lower == "package.json" ||
    lower == "main.rs" || lower == "lib.rs" || lower == "mod.rs" ||
    lower == "index.js" || lower == "index.ts" || lower == "app.py" ||
    lower == "main.py" || lower == "main.go" || lower == "pom.xml" ||
    lower == "build.gradle" || lower == "makefile" || lower == "dockerfile" ||
    lower.ends_with("mod.rs") || lower.ends_with("/main.rs") ||
    lower.contains("src/lib") || lower.contains("src/main")
}

fn compute_file_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(detect_language(Path::new("test.rs")), Some("Rust".to_string()));
        assert_eq!(detect_language(Path::new("test.py")), Some("Python".to_string()));
        assert_eq!(detect_language(Path::new("test.ts")), Some("TypeScript".to_string()));
        assert_eq!(detect_language(Path::new("Cargo.toml")), Some("Rust".to_string()));
    }

    #[test]
    fn test_file_type_detection() {
        assert_eq!(detect_file_type(Path::new("config.toml"), &None), FileType::Config);
        assert_eq!(detect_file_type(Path::new("README.md"), &None), FileType::Documentation);
        assert_eq!(detect_file_type(Path::new("image.png"), &None), FileType::Binary);
    }
}
