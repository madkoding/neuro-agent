//! Interactive Diff Preview System
//!
//! Shows changes before applying them, similar to `git diff`.
//! Provides options to accept, reject, edit, or selectively apply changes.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// A diff preview showing old vs new content
#[derive(Debug, Clone)]
pub struct DiffPreview {
    /// Path to the file being modified
    pub file_path: PathBuf,
    /// Original file content (before changes)
    pub old_content: String,
    /// New file content (after changes)
    pub new_content: String,
    /// Timestamp when diff was created
    pub created_at: std::time::SystemTime,
}

/// User action on a diff preview
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAction {
    /// Apply all changes
    Apply,
    /// Reject all changes
    Reject,
    /// Edit the new content before applying
    Edit,
    /// Apply only selected hunks (not implemented yet)
    Split,
}

/// A hunk represents a contiguous block of changes
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line in old file (1-indexed)
    pub old_start: usize,
    /// Number of lines in old file
    pub old_count: usize,
    /// Starting line in new file (1-indexed)
    pub new_start: usize,
    /// Number of lines in new file
    pub new_count: usize,
    /// Lines in this hunk (with +/- prefix)
    pub lines: Vec<String>,
}

impl DiffPreview {
    /// Create a new diff preview
    pub fn new(file_path: PathBuf, old_content: String, new_content: String) -> Self {
        Self {
            file_path,
            old_content,
            new_content,
            created_at: std::time::SystemTime::now(),
        }
    }

    /// Create diff preview from file path and new content
    pub fn from_file(file_path: &Path, new_content: String) -> Result<Self> {
        let old_content = if file_path.exists() {
            std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read file: {:?}", file_path))?
        } else {
            String::new()
        };

        Ok(Self::new(file_path.to_path_buf(), old_content, new_content))
    }

    /// Generate unified diff format (like git diff)
    pub fn generate_unified_diff(&self) -> String {
        let old_lines: Vec<&str> = if self.old_content.is_empty() {
            vec![]
        } else {
            self.old_content.lines().collect()
        };
        let new_lines: Vec<&str> = self.new_content.lines().collect();

        let mut diff = String::new();

        // Header
        diff.push_str(&format!(
            "--- a/{}\n",
            self.file_path.display()
        ));
        diff.push_str(&format!(
            "+++ b/{}\n",
            self.file_path.display()
        ));

        // Generate hunks using simple line-by-line diff
        let hunks = self.generate_hunks(&old_lines, &new_lines);

        for hunk in hunks {
            diff.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));

            for line in &hunk.lines {
                diff.push_str(line);
                diff.push('\n');
            }
        }

        diff
    }

    /// Generate diff hunks (simplified algorithm)
    fn generate_hunks(&self, old_lines: &[&str], new_lines: &[&str]) -> Vec<DiffHunk> {
        let mut hunks = Vec::new();

        if old_lines.is_empty() && new_lines.is_empty() {
            return hunks;
        }

        // Simple case: file creation
        if old_lines.is_empty() {
            let lines: Vec<String> = new_lines
                .iter()
                .map(|line| format!("+{}", line))
                .collect();

            hunks.push(DiffHunk {
                old_start: 0,
                old_count: 0,
                new_start: 1,
                new_count: new_lines.len(),
                lines,
            });
            return hunks;
        }

        // Simple case: file deletion
        if new_lines.is_empty() {
            let lines: Vec<String> = old_lines
                .iter()
                .map(|line| format!("-{}", line))
                .collect();

            hunks.push(DiffHunk {
                old_start: 1,
                old_count: old_lines.len(),
                new_start: 0,
                new_count: 0,
                lines,
            });
            return hunks;
        }

        // General case: show first changed section (simplified)
        let mut hunk_lines = Vec::new();
        let context_lines = 3;

        let min_len = old_lines.len().min(new_lines.len());
        let max_len = old_lines.len().max(new_lines.len());

        // Find first difference
        let mut first_diff = 0;
        for i in 0..min_len {
            if old_lines[i] != new_lines[i] {
                first_diff = i;
                break;
            }
        }

        // Add context before
        let start = first_diff.saturating_sub(context_lines);
        for i in start..first_diff {
            if i < old_lines.len() {
                hunk_lines.push(format!(" {}", old_lines[i]));
            }
        }

        // Add changed lines
        let mut old_idx = first_diff;
        let mut new_idx = first_diff;

        while old_idx < old_lines.len() || new_idx < new_lines.len() {
            if old_idx < old_lines.len() && new_idx < new_lines.len() {
                if old_lines[old_idx] == new_lines[new_idx] {
                    hunk_lines.push(format!(" {}", old_lines[old_idx]));
                    old_idx += 1;
                    new_idx += 1;
                } else {
                    // Different lines
                    hunk_lines.push(format!("-{}", old_lines[old_idx]));
                    hunk_lines.push(format!("+{}", new_lines[new_idx]));
                    old_idx += 1;
                    new_idx += 1;
                }
            } else if old_idx < old_lines.len() {
                hunk_lines.push(format!("-{}", old_lines[old_idx]));
                old_idx += 1;
            } else if new_idx < new_lines.len() {
                hunk_lines.push(format!("+{}", new_lines[new_idx]));
                new_idx += 1;
            }

            // Stop after some lines to avoid huge diffs
            if hunk_lines.len() > 50 {
                break;
            }
        }

        hunks.push(DiffHunk {
            old_start: start + 1,
            old_count: old_idx - start,
            new_start: start + 1,
            new_count: new_idx - start,
            lines: hunk_lines,
        });

        hunks
    }

    /// Get a colored version of the diff (for terminal display)
    pub fn generate_colored_diff(&self) -> String {
        let diff = self.generate_unified_diff();
        let mut colored = String::new();

        for line in diff.lines() {
            if line.starts_with("---") || line.starts_with("+++") {
                // File headers in bold
                colored.push_str(&format!("\x1b[1m{}\x1b[0m\n", line));
            } else if line.starts_with("@@") {
                // Hunk headers in cyan
                colored.push_str(&format!("\x1b[36m{}\x1b[0m\n", line));
            } else if line.starts_with('+') {
                // Additions in green
                colored.push_str(&format!("\x1b[32m{}\x1b[0m\n", line));
            } else if line.starts_with('-') {
                // Deletions in red
                colored.push_str(&format!("\x1b[31m{}\x1b[0m\n", line));
            } else {
                // Context lines normal
                colored.push_str(line);
                colored.push('\n');
            }
        }

        colored
    }

    /// Apply the diff (write new content to file)
    pub fn apply(&self) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        std::fs::write(&self.file_path, &self.new_content)
            .with_context(|| format!("Failed to write file: {:?}", self.file_path))?;

        Ok(())
    }

    /// Get statistics about the diff
    pub fn stats(&self) -> DiffStats {
        let old_lines: Vec<&str> = self.old_content.lines().collect();
        let new_lines: Vec<&str> = self.new_content.lines().collect();

        let mut additions = 0;
        let mut deletions = 0;

        // Simple line count comparison
        if new_lines.len() > old_lines.len() {
            additions = new_lines.len() - old_lines.len();
        } else if old_lines.len() > new_lines.len() {
            deletions = old_lines.len() - new_lines.len();
        }

        // Count actual changed lines
        let min_len = old_lines.len().min(new_lines.len());
        for i in 0..min_len {
            if old_lines[i] != new_lines[i] {
                deletions += 1;
                additions += 1;
            }
        }

        DiffStats {
            additions,
            deletions,
            file_path: self.file_path.clone(),
        }
    }

    /// Check if the diff represents a file creation
    pub fn is_new_file(&self) -> bool {
        self.old_content.is_empty() && !self.new_content.is_empty()
    }

    /// Check if the diff represents a file deletion
    pub fn is_deleted_file(&self) -> bool {
        !self.old_content.is_empty() && self.new_content.is_empty()
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        self.old_content != self.new_content
    }
}

/// Statistics about a diff
#[derive(Debug, Clone)]
pub struct DiffStats {
    /// Number of lines added
    pub additions: usize,
    /// Number of lines deleted
    pub deletions: usize,
    /// File being modified
    pub file_path: PathBuf,
}

impl DiffStats {
    /// Format stats for display (like git diff --stat)
    pub fn format(&self) -> String {
        let total = self.additions + self.deletions;
        let file_name = self.file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        format!(
            "{} | {} {}",
            file_name,
            total,
            if self.additions > self.deletions {
                format!("+{}", self.additions - self.deletions)
            } else if self.deletions > self.additions {
                format!("-{}", self.deletions - self.additions)
            } else {
                "~".to_string()
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_diff_preview_creation() {
        let old = "line 1\nline 2\nline 3".to_string();
        let new = "line 1\nline 2 modified\nline 3".to_string();
        
        let diff = DiffPreview::new(
            PathBuf::from("test.txt"),
            old,
            new,
        );

        assert_eq!(diff.file_path, PathBuf::from("test.txt"));
        assert!(diff.has_changes());
    }

    #[test]
    fn test_unified_diff_generation() {
        let old = "line 1\nline 2\nline 3".to_string();
        let new = "line 1\nline 2 modified\nline 3".to_string();
        
        let diff = DiffPreview::new(PathBuf::from("test.txt"), old, new);
        let unified = diff.generate_unified_diff();

        assert!(unified.contains("--- a/test.txt"));
        assert!(unified.contains("+++ b/test.txt"));
        assert!(unified.contains("@@"));
    }

    #[test]
    fn test_new_file_diff() {
        let old = String::new();
        let new = "new file content\nline 2".to_string();
        
        let diff = DiffPreview::new(PathBuf::from("new.txt"), old, new);

        assert!(diff.is_new_file());
        assert!(!diff.is_deleted_file());
        assert!(diff.has_changes());

        let unified = diff.generate_unified_diff();
        assert!(unified.contains("+new file content"));
    }

    #[test]
    fn test_deleted_file_diff() {
        let old = "old content\nline 2".to_string();
        let new = String::new();
        
        let diff = DiffPreview::new(PathBuf::from("deleted.txt"), old, new);

        assert!(!diff.is_new_file());
        assert!(diff.is_deleted_file());
        assert!(diff.has_changes());

        let unified = diff.generate_unified_diff();
        assert!(unified.contains("-old content"));
    }

    #[test]
    fn test_diff_stats() {
        let old = "line 1\nline 2\nline 3".to_string();
        let new = "line 1\nline 2 modified\nline 3\nline 4".to_string();
        
        let diff = DiffPreview::new(PathBuf::from("test.txt"), old, new);
        let stats = diff.stats();

        assert!(stats.additions > 0);
        assert_eq!(stats.file_path, PathBuf::from("test.txt"));
    }

    #[test]
    fn test_apply_diff() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "old content").unwrap();
        let path = temp_file.path().to_path_buf();

        let new_content = "new content\nline 2".to_string();
        let diff = DiffPreview::from_file(&path, new_content.clone()).unwrap();

        diff.apply().unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, new_content);
    }

    #[test]
    fn test_no_changes_diff() {
        let content = "same content\nline 2".to_string();
        let diff = DiffPreview::new(
            PathBuf::from("test.txt"),
            content.clone(),
            content,
        );

        assert!(!diff.has_changes());
    }

    #[test]
    fn test_colored_diff_generation() {
        let old = "line 1\nline 2".to_string();
        let new = "line 1\nline 2 modified".to_string();
        
        let diff = DiffPreview::new(PathBuf::from("test.txt"), old, new);
        let colored = diff.generate_colored_diff();

        // Should contain ANSI color codes
        assert!(colored.contains("\x1b["));
    }
}
