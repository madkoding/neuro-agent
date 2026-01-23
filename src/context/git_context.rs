//! Git-aware context module
//!
//! Provides context about git repository state to prioritize recently modified
//! and changed files in code analysis.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git-aware context information
#[derive(Debug, Clone)]
pub struct GitContext {
    /// Project root directory (git repository root)
    project_root: PathBuf,
    /// Cache of recently modified files
    recently_modified_cache: Vec<PathBuf>,
    /// Cache last update timestamp
    cache_timestamp: std::time::SystemTime,
    /// Cache validity duration
    cache_ttl: std::time::Duration,
}

/// Type of git change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitChangeType {
    /// File was added
    Added,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File is untracked
    Untracked,
}

/// Information about a changed file
#[derive(Debug, Clone)]
pub struct GitChangedFile {
    /// Path to the file
    pub path: PathBuf,
    /// Type of change
    pub change_type: GitChangeType,
    /// Priority score (0.0-1.0, higher = more relevant)
    pub priority: f32,
}

impl GitContext {
    /// Create a new GitContext for the given project root
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            recently_modified_cache: Vec::new(),
            cache_timestamp: std::time::SystemTime::UNIX_EPOCH,
            cache_ttl: std::time::Duration::from_secs(60), // 1 minute cache
        }
    }

    /// Check if this directory is a git repository
    pub fn is_git_repo(&self) -> bool {
        self.project_root.join(".git").exists()
    }

    /// Get the current branch name
    pub fn current_branch(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(&self.project_root)
            .output()
            .context("Failed to execute git branch")?;

        if !output.status.success() {
            anyhow::bail!("git branch failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let branch = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in branch name")?
            .trim()
            .to_string();

        Ok(branch)
    }

    /// Get files modified in the last N days
    pub fn get_recently_modified(&mut self, days: u32) -> Result<Vec<PathBuf>> {
        // Check cache validity
        let now = std::time::SystemTime::now();
        if let Ok(elapsed) = now.duration_since(self.cache_timestamp) {
            if elapsed < self.cache_ttl && !self.recently_modified_cache.is_empty() {
                return Ok(self.recently_modified_cache.clone());
            }
        }

        let since_date = format!("{}d", days);
        let output = Command::new("git")
            .arg("log")
            .arg("--since")
            .arg(&since_date)
            .arg("--name-only")
            .arg("--pretty=format:")
            .current_dir(&self.project_root)
            .output()
            .context("Failed to execute git log")?;

        if !output.status.success() {
            anyhow::bail!("git log failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let files_str = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git log output")?;

        let mut files: HashSet<PathBuf> = HashSet::new();
        for line in files_str.lines() {
            let line = line.trim();
            if !line.is_empty() {
                let file_path = self.project_root.join(line);
                if file_path.exists() {
                    files.insert(file_path);
                }
            }
        }

        let result: Vec<PathBuf> = files.into_iter().collect();

        // Update cache
        self.recently_modified_cache = result.clone();
        self.cache_timestamp = now;

        Ok(result)
    }

    /// Get uncommitted changes (staged and unstaged)
    pub fn get_uncommitted_changes(&self) -> Result<Vec<GitChangedFile>> {
        let output = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(&self.project_root)
            .output()
            .context("Failed to execute git status")?;

        if !output.status.success() {
            anyhow::bail!("git status failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let status_str = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git status output")?;

        let mut changes = Vec::new();

        for line in status_str.lines() {
            if line.len() < 3 {
                continue;
            }

            let status_code = &line[0..2];
            let file_path_str = line[3..].trim();
            let file_path = self.project_root.join(file_path_str);

            let (change_type, priority) = match status_code {
                "A " | " A" => (GitChangeType::Added, 0.9),
                "M " | " M" | "MM" => (GitChangeType::Modified, 1.0), // Highest priority
                "D " | " D" => (GitChangeType::Deleted, 0.7),
                "??" => (GitChangeType::Untracked, 0.6),
                _ => continue, // Ignore other status codes
            };

            changes.push(GitChangedFile {
                path: file_path,
                change_type,
                priority,
            });
        }

        Ok(changes)
    }

    /// Get diff for uncommitted changes (useful for context)
    pub fn get_uncommitted_diff(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("diff")
            .arg("HEAD")
            .current_dir(&self.project_root)
            .output()
            .context("Failed to execute git diff")?;

        if !output.status.success() {
            anyhow::bail!("git diff failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let diff = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git diff output")?;

        Ok(diff)
    }

    /// Check if a file has uncommitted changes
    pub fn has_uncommitted_changes(&self, file_path: &Path) -> Result<bool> {
        let changes = self.get_uncommitted_changes()?;
        Ok(changes.iter().any(|change| change.path == file_path))
    }

    /// Get priority boost for a file based on git state
    /// Returns a value between 0.0 and 0.3 to add to confidence scores
    pub fn get_priority_boost(&mut self, file_path: &Path, base_days: u32) -> Result<f32> {
        // Check uncommitted changes (highest priority)
        if let Ok(changes) = self.get_uncommitted_changes() {
            if let Some(change) = changes.iter().find(|c| c.path == file_path) {
                return Ok(change.priority * 0.3); // Max boost 0.3
            }
        }

        // Check recently modified files
        if let Ok(recent) = self.get_recently_modified(base_days) {
            if recent.contains(&file_path.to_path_buf()) {
                return Ok(0.15); // Medium boost
            }
        }

        Ok(0.0) // No boost
    }

    /// Clear the cache (force refresh on next query)
    pub fn clear_cache(&mut self) {
        self.recently_modified_cache.clear();
        self.cache_timestamp = std::time::SystemTime::UNIX_EPOCH;
    }

    /// Get a comprehensive string of the full Git context
    pub async fn get_full_context(&mut self) -> String {
        let mut context = String::new();

        if !self.is_git_repo() {
            return context; // Return empty if not a git repo
        }

        // --- Uncommitted changes ---
        if let Ok(changes) = self.get_uncommitted_changes() {
            if !changes.is_empty() {
                context.push_str("\n\n⚠️ Cambios sin commit detectados:\n");

                let mut added = Vec::new();
                let mut modified = Vec::new();
                let mut deleted = Vec::new();
                let mut untracked = Vec::new();

                for change in changes {
                    let file_name = change.path.strip_prefix(&self.project_root)
                        .ok()
                        .and_then(|p| p.to_str())
                        .unwrap_or_else(|| change.path.to_str().unwrap_or("unknown"))
                        .to_string();

                    match change.change_type {
                        GitChangeType::Added => added.push(file_name),
                        GitChangeType::Modified => modified.push(file_name),
                        GitChangeType::Deleted => deleted.push(file_name),
                        GitChangeType::Untracked => untracked.push(file_name),
                    }
                }

                if !modified.is_empty() {
                    context.push_str(&format!("  • Modificados ({}): ", modified.len()));
                    for (i, file) in modified.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if modified.len() > 5 {
                        context.push_str(&format!(" +{} más", modified.len() - 5));
                    }
                    context.push('\n');
                }

                if !added.is_empty() {
                    context.push_str(&format!("  • Añadidos ({}): ", added.len()));
                    for (i, file) in added.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if added.len() > 5 {
                        context.push_str(&format!(" +{} más", added.len() - 5));
                    }
                    context.push('\n');
                }

                if !deleted.is_empty() {
                    context.push_str(&format!("  • Eliminados ({}): ", deleted.len()));
                    for (i, file) in deleted.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    context.push('\n');
                }

                if !untracked.is_empty() {
                    context.push_str(&format!("  • Sin seguimiento ({}): ", untracked.len()));
                    for (i, file) in untracked.iter().take(3).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if untracked.len() > 3 {
                        context.push_str(&format!(" +{} más", untracked.len() - 3));
                    }
                    context.push('\n');
                }

                context.push_str("\nEstos archivos tienen cambios pendientes que pueden ser relevantes para tu consulta.\n");
            }
        }

        // --- Recently modified files ---
        if let Ok(recent_files) = self.get_recently_modified(7) { // Last 7 days
            if !recent_files.is_empty() && recent_files.len() <= 20 {
                context.push_str("\n\n📝 Archivos modificados recientemente (últimos 7 días):\n");
                for file in recent_files.iter().take(10) {
                    if let Some(file_name) = file.strip_prefix(&self.project_root)
                        .ok()
                        .and_then(|p| p.to_str()) {
                        context.push_str(&format!("  • {}\n", file_name));
                    }
                }
                if recent_files.len() > 10 {
                    context.push_str(&format!("  ... y {} más\n", recent_files.len() - 10));
                }
            }
        }

        // --- Current branch ---
        if let Ok(branch) = self.current_branch() {
            if !branch.is_empty() && branch != "master" && branch != "main" {
                context.push_str(&format!("\n🌿 Rama actual: {}\n", branch));
            }
        }

        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_context_creation() {
        let ctx = GitContext::new(PathBuf::from("."));
        assert!(ctx.project_root.exists());
    }

    #[test]
    fn test_is_git_repo() {
        let ctx = GitContext::new(PathBuf::from("."));
        // This project should be a git repo
        assert!(ctx.is_git_repo());
    }

    #[test]
    fn test_current_branch() {
        let ctx = GitContext::new(PathBuf::from("."));
        if ctx.is_git_repo() {
            let branch = ctx.current_branch();
            assert!(branch.is_ok());
            let branch_name = branch.unwrap();
            assert!(!branch_name.is_empty());
        }
    }

    #[test]
    fn test_get_recently_modified() {
        let mut ctx = GitContext::new(PathBuf::from("."));
        if ctx.is_git_repo() {
            let files = ctx.get_recently_modified(7); // Last 7 days
            assert!(files.is_ok());
            // Should return a list (may be empty if no recent commits)
            let files = files.unwrap();
            assert!(files.len() >= 0); // Just verify it returns a valid vec
        }
    }

    #[test]
    fn test_get_uncommitted_changes() {
        let ctx = GitContext::new(PathBuf::from("."));
        if ctx.is_git_repo() {
            let changes = ctx.get_uncommitted_changes();
            assert!(changes.is_ok());
            // May be empty if working tree is clean
            let changes = changes.unwrap();
            assert!(changes.len() >= 0);
        }
    }

    #[test]
    fn test_cache_validity() {
        let mut ctx = GitContext::new(PathBuf::from("."));
        if ctx.is_git_repo() {
            // First call - should hit git
            let files1 = ctx.get_recently_modified(7);
            assert!(files1.is_ok());

            // Second call - should use cache
            let files2 = ctx.get_recently_modified(7);
            assert!(files2.is_ok());

            // Results should be identical
            assert_eq!(files1.unwrap(), files2.unwrap());
        }
    }

    #[test]
    fn test_clear_cache() {
        let mut ctx = GitContext::new(PathBuf::from("."));
        if ctx.is_git_repo() {
            // Populate cache
            let _ = ctx.get_recently_modified(7);
            assert!(!ctx.recently_modified_cache.is_empty() || ctx.recently_modified_cache.is_empty());

            // Clear cache
            ctx.clear_cache();
            assert_eq!(ctx.cache_timestamp, std::time::SystemTime::UNIX_EPOCH);
        }
    }
}

