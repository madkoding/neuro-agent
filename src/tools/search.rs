//! Search tool - Search within files using patterns

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use walkdir::WalkDir;
use regex::Regex;

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Search output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOutput {
    pub pattern: String,
    pub total_matches: usize,
    pub files_searched: usize,
    pub files_with_matches: usize,
    pub results: Vec<SearchResult>,
}

/// Search in files tool
#[derive(Debug, Clone, Default)]
pub struct SearchInFilesTool;

impl SearchInFilesTool {
    pub const NAME: &'static str = "search_in_files";

    pub fn new() -> Self {
        Self
    }

    /// Search for a pattern in files
    pub async fn search(&self, args: SearchArgs) -> Result<SearchOutput, SearchError> {
        let root = PathBuf::from(&args.path);
        
        if !root.exists() {
            return Err(SearchError::PathNotFound(args.path));
        }

        let pattern = if args.is_regex.unwrap_or(false) {
            Regex::new(&args.pattern).map_err(|e| SearchError::InvalidRegex(e.to_string()))?
        } else {
            // Escape special regex characters for literal search
            let escaped = regex::escape(&args.pattern);
            Regex::new(&escaped).unwrap()
        };

        let case_insensitive = args.case_insensitive.unwrap_or(true);
        let pattern = if case_insensitive {
            Regex::new(&format!("(?i){}", pattern.as_str())).unwrap_or(pattern)
        } else {
            pattern
        };

        let max_results = args.max_results.unwrap_or(100);
        let context_lines = args.context_lines.unwrap_or(2);
        let file_pattern = args.file_pattern.as_deref();
        
        let ignore_patterns = vec![
            ".git", "node_modules", "target", "__pycache__", 
            ".venv", "venv", "dist", "build", ".next"
        ];

        let mut results = Vec::new();
        let mut files_searched = 0;
        let mut files_with_matches = 0;

        for entry in WalkDir::new(&root)
            .max_depth(args.max_depth.unwrap_or(10))
            .into_iter()
            .filter_entry(|e| {
                let path = e.path();
                !ignore_patterns.iter().any(|p| path.to_string_lossy().contains(p))
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            if !path.is_file() {
                continue;
            }

            // Check file pattern
            if let Some(fp) = file_pattern {
                let file_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !matches_glob(&file_name, fp) {
                    continue;
                }
            }

            // Skip binary files
            if is_binary_file(path) {
                continue;
            }

            // Read and search file
            let content = match fs::read_to_string(path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            files_searched += 1;
            let lines: Vec<&str> = content.lines().collect();
            let mut file_has_match = false;

            for (line_num, line) in lines.iter().enumerate() {
                if let Some(m) = pattern.find(line) {
                    file_has_match = true;
                    
                    // Get context
                    let context_before: Vec<String> = lines
                        .iter()
                        .skip(line_num.saturating_sub(context_lines))
                        .take(context_lines.min(line_num))
                        .map(|s| s.to_string())
                        .collect();

                    let context_after: Vec<String> = lines
                        .iter()
                        .skip(line_num + 1)
                        .take(context_lines)
                        .map(|s| s.to_string())
                        .collect();

                    results.push(SearchResult {
                        file: path.to_path_buf(),
                        line_number: line_num + 1,
                        line_content: line.to_string(),
                        match_start: m.start(),
                        match_end: m.end(),
                        context_before,
                        context_after,
                    });

                    if results.len() >= max_results {
                        break;
                    }
                }
            }

            if file_has_match {
                files_with_matches += 1;
            }

            if results.len() >= max_results {
                break;
            }
        }

        Ok(SearchOutput {
            pattern: args.pattern,
            total_matches: results.len(),
            files_searched,
            files_with_matches,
            results,
        })
    }

    /// Search and replace in files
    pub async fn search_replace(&self, args: SearchReplaceArgs) -> Result<ReplaceOutput, SearchError> {
        let search_args = SearchArgs {
            path: args.path.clone(),
            pattern: args.pattern.clone(),
            is_regex: args.is_regex,
            case_insensitive: args.case_insensitive,
            file_pattern: args.file_pattern.clone(),
            max_results: Some(1000),
            context_lines: Some(0),
            max_depth: args.max_depth,
        };

        let search_results = self.search(search_args).await?;
        
        if args.dry_run.unwrap_or(true) {
            return Ok(ReplaceOutput {
                pattern: args.pattern,
                replacement: args.replacement,
                files_modified: 0,
                total_replacements: search_results.total_matches,
                dry_run: true,
                modified_files: vec![],
            });
        }

        let pattern = if args.is_regex.unwrap_or(false) {
            Regex::new(&args.pattern).map_err(|e| SearchError::InvalidRegex(e.to_string()))?
        } else {
            Regex::new(&regex::escape(&args.pattern)).unwrap()
        };

        let case_insensitive = args.case_insensitive.unwrap_or(true);
        let pattern = if case_insensitive {
            Regex::new(&format!("(?i){}", pattern.as_str())).unwrap_or(pattern)
        } else {
            pattern
        };

        let mut files_modified = 0;
        let mut total_replacements = 0;
        let mut modified_files = Vec::new();

        // Group results by file
        let mut files_to_modify: std::collections::HashMap<PathBuf, Vec<&SearchResult>> = 
            std::collections::HashMap::new();
        
        for result in &search_results.results {
            files_to_modify
                .entry(result.file.clone())
                .or_default()
                .push(result);
        }

        for (file_path, _) in files_to_modify {
            let content = match fs::read_to_string(&file_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (new_content, count) = replace_all(&pattern, &content, &args.replacement);
            
            if count > 0 {
                fs::write(&file_path, &new_content).await
                    .map_err(|e| SearchError::WriteError(e.to_string()))?;
                
                files_modified += 1;
                total_replacements += count;
                modified_files.push(file_path);
            }
        }

        Ok(ReplaceOutput {
            pattern: args.pattern,
            replacement: args.replacement,
            files_modified,
            total_replacements,
            dry_run: false,
            modified_files,
        })
    }
}

/// Arguments for searching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchArgs {
    pub path: String,
    pub pattern: String,
    pub is_regex: Option<bool>,
    pub case_insensitive: Option<bool>,
    pub file_pattern: Option<String>,
    pub max_results: Option<usize>,
    pub context_lines: Option<usize>,
    pub max_depth: Option<usize>,
}

/// Arguments for search and replace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReplaceArgs {
    pub path: String,
    pub pattern: String,
    pub replacement: String,
    pub is_regex: Option<bool>,
    pub case_insensitive: Option<bool>,
    pub file_pattern: Option<String>,
    pub dry_run: Option<bool>,
    pub max_depth: Option<usize>,
}

/// Replace output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceOutput {
    pub pattern: String,
    pub replacement: String,
    pub files_modified: usize,
    pub total_replacements: usize,
    pub dry_run: bool,
    pub modified_files: Vec<PathBuf>,
}

/// Search errors
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Invalid regex: {0}")]
    InvalidRegex(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Write error: {0}")]
    WriteError(String),
}

fn matches_glob(filename: &str, pattern: &str) -> bool {
    if let Some(ext) = pattern.strip_prefix("*.") {
        filename.ends_with(&format!(".{}", ext))
    } else if pattern.contains('*') {
        // Simple glob matching
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            filename.starts_with(parts[0]) && filename.ends_with(parts[1])
        } else {
            filename.contains(pattern)
        }
    } else {
        filename.contains(pattern)
    }
}

fn is_binary_file(path: &Path) -> bool {
    let ext = path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    matches!(
        ext.as_str(),
        "exe" | "dll" | "so" | "dylib" | "bin" | "o" | "a" | "lib" |
        "png" | "jpg" | "jpeg" | "gif" | "ico" | "svg" | "woff" | "woff2" |
        "ttf" | "eot" | "pdf" | "zip" | "tar" | "gz" | "rar" | "7z" |
        "mp3" | "mp4" | "wav" | "avi" | "mov" | "mkv"
    )
}

fn replace_all(pattern: &Regex, content: &str, replacement: &str) -> (String, usize) {
    let mut count = 0;
    let result = pattern.replace_all(content, |_caps: &regex::Captures| {
        count += 1;
        replacement
    });
    (result.into_owned(), count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matching() {
        assert!(matches_glob("test.rs", "*.rs"));
        assert!(matches_glob("test.rs", "test*"));
        assert!(!matches_glob("test.py", "*.rs"));
    }

    #[test]
    fn test_binary_detection() {
        assert!(is_binary_file(Path::new("image.png")));
        assert!(is_binary_file(Path::new("archive.zip")));
        assert!(!is_binary_file(Path::new("code.rs")));
    }
}
