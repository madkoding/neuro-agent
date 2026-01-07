//! Git tool - Git operations and history analysis

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Git status output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub branch: String,
    pub is_clean: bool,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub untracked: Vec<String>,
    pub ahead: usize,
    pub behind: usize,
}

/// File change info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: ChangeType,
}

/// Type of change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Commit info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub message: String,
    pub files_changed: usize,
}

/// Diff output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffOutput {
    pub files: Vec<FileDiff>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

/// File diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

/// Diff hunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub content: String,
}

/// Branch info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub tracking: Option<String>,
    pub last_commit: Option<String>,
}

/// Git tool
#[derive(Debug, Clone, Default)]
pub struct GitTool;

impl GitTool {
    pub const NAME: &'static str = "git_tool";

    pub fn new() -> Self {
        Self
    }

    /// Get git status
    pub async fn status(&self, args: GitStatusArgs) -> Result<GitStatus, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        // Get branch name
        let branch = run_git_command(&path, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string();

        // Get status
        let status_output = run_git_command(&path, &["status", "--porcelain=v1"])?;
        
        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for line in status_output.lines() {
            if line.len() < 3 {
                continue;
            }
            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');
            let file_path = line[3..].to_string();

            // Staged changes
            if index_status != ' ' && index_status != '?' {
                staged.push(FileChange {
                    path: file_path.clone(),
                    change_type: char_to_change_type(index_status),
                });
            }

            // Unstaged changes
            if worktree_status != ' ' && worktree_status != '?' {
                unstaged.push(FileChange {
                    path: file_path.clone(),
                    change_type: char_to_change_type(worktree_status),
                });
            }

            // Untracked
            if index_status == '?' {
                untracked.push(file_path);
            }
        }

        // Get ahead/behind
        let (ahead, behind) = get_ahead_behind(&path)?;

        let is_clean = staged.is_empty() && unstaged.is_empty() && untracked.is_empty();

        Ok(GitStatus {
            branch,
            is_clean,
            staged,
            unstaged,
            untracked,
            ahead,
            behind,
        })
    }

    /// Get commit log
    pub async fn log(&self, args: GitLogArgs) -> Result<Vec<CommitInfo>, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let count = args.count.unwrap_or(10);
        let format = "--format=%H|%h|%an|%ae|%ai|%s";
        let count_arg = format!("-{}", count);
        
        let mut cmd_args = vec!["log", format, &count_arg];
        
        if let Some(ref author) = args.author {
            cmd_args.push("--author");
            cmd_args.push(author);
        }

        let output = run_git_command(&path, &cmd_args)?;
        
        let mut commits = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(6, '|').collect();
            if parts.len() == 6 {
                commits.push(CommitInfo {
                    hash: parts[0].to_string(),
                    short_hash: parts[1].to_string(),
                    author: parts[2].to_string(),
                    email: parts[3].to_string(),
                    date: parts[4].to_string(),
                    message: parts[5].to_string(),
                    files_changed: 0,
                });
            }
        }

        Ok(commits)
    }

    /// Get diff
    pub async fn diff(&self, args: GitDiffArgs) -> Result<DiffOutput, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let mut cmd_args = vec!["diff"];
        
        if args.staged.unwrap_or(false) {
            cmd_args.push("--staged");
        }

        if let Some(ref commit) = args.commit {
            cmd_args.push(commit);
        }

        let output = run_git_command(&path, &cmd_args)?;
        
        parse_diff_output(&output)
    }

    /// Get branches
    pub async fn branches(&self, args: GitBranchesArgs) -> Result<Vec<BranchInfo>, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let mut cmd_args = vec!["branch", "-v"];
        if args.all.unwrap_or(false) {
            cmd_args.push("-a");
        }

        let output = run_git_command(&path, &cmd_args)?;
        
        let mut branches = Vec::new();
        for line in output.lines() {
            let is_current = line.starts_with('*');
            let line = line.trim_start_matches(['*', ' '].as_ref());
            let parts: Vec<&str> = line.split_whitespace().collect();
            
            if parts.is_empty() {
                continue;
            }

            let name = parts[0].to_string();
            let is_remote = name.starts_with("remotes/");
            let last_commit = parts.get(1).map(|s| s.to_string());

            branches.push(BranchInfo {
                name,
                is_current,
                is_remote,
                tracking: None,
                last_commit,
            });
        }

        Ok(branches)
    }

    /// Stage files
    pub async fn add(&self, args: GitAddArgs) -> Result<String, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let files: Vec<&str> = args.files.iter().map(|s| s.as_str()).collect();
        let mut cmd_args = vec!["add"];
        cmd_args.extend(files);

        run_git_command(&path, &cmd_args)?;
        Ok("Files staged successfully".to_string())
    }

    /// Commit changes
    pub async fn commit(&self, args: GitCommitArgs) -> Result<CommitInfo, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let cmd_args = vec!["commit", "-m", &args.message];
        run_git_command(&path, &cmd_args)?;

        // Get the commit we just made
        let output = run_git_command(&path, &["log", "-1", "--format=%H|%h|%an|%ae|%ai|%s"])?;
        let parts: Vec<&str> = output.trim().splitn(6, '|').collect();
        
        if parts.len() == 6 {
            Ok(CommitInfo {
                hash: parts[0].to_string(),
                short_hash: parts[1].to_string(),
                author: parts[2].to_string(),
                email: parts[3].to_string(),
                date: parts[4].to_string(),
                message: parts[5].to_string(),
                files_changed: 0,
            })
        } else {
            Err(GitError::CommandFailed("Failed to parse commit info".to_string()))
        }
    }

    /// Get file blame
    pub async fn blame(&self, args: GitBlameArgs) -> Result<Vec<BlameLine>, GitError> {
        let path = PathBuf::from(&args.path);
        
        if !is_git_repo(&path) {
            return Err(GitError::NotAGitRepo);
        }

        let cmd_args = vec!["blame", "--line-porcelain", &args.file];
        let output = run_git_command(&path, &cmd_args)?;
        
        parse_blame_output(&output)
    }
}

/// Blame line
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    pub commit: String,
    pub author: String,
    pub date: String,
    pub line_number: usize,
    pub content: String,
}

/// Arguments for git status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusArgs {
    pub path: String,
}

/// Arguments for git log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLogArgs {
    pub path: String,
    pub count: Option<usize>,
    pub author: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// Arguments for git diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffArgs {
    pub path: String,
    pub commit: Option<String>,
    pub staged: Option<bool>,
    pub file: Option<String>,
}

/// Arguments for git branches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBranchesArgs {
    pub path: String,
    pub all: Option<bool>,
}

/// Arguments for git add
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAddArgs {
    pub path: String,
    pub files: Vec<String>,
}

/// Arguments for git commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitArgs {
    pub path: String,
    pub message: String,
}

/// Arguments for git blame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBlameArgs {
    pub path: String,
    pub file: String,
}

/// Git errors
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Not a git repository")]
    NotAGitRepo,
    #[error("Git command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

fn is_git_repo(path: &PathBuf) -> bool {
    let git_dir = path.join(".git");
    git_dir.exists() || run_git_command(path, &["rev-parse", "--git-dir"]).is_ok()
}

fn run_git_command(path: &PathBuf, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .map_err(|e| GitError::IoError(e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(GitError::CommandFailed(stderr))
    }
}

fn char_to_change_type(c: char) -> ChangeType {
    match c {
        'A' => ChangeType::Added,
        'M' => ChangeType::Modified,
        'D' => ChangeType::Deleted,
        'R' => ChangeType::Renamed,
        'C' => ChangeType::Copied,
        _ => ChangeType::Modified,
    }
}

fn get_ahead_behind(path: &PathBuf) -> Result<(usize, usize), GitError> {
    let output = run_git_command(path, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]);
    
    match output {
        Ok(s) => {
            let parts: Vec<&str> = s.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().unwrap_or(0);
                let behind = parts[1].parse().unwrap_or(0);
                Ok((ahead, behind))
            } else {
                Ok((0, 0))
            }
        }
        Err(_) => Ok((0, 0)),
    }
}

fn parse_diff_output(output: &str) -> Result<DiffOutput, GitError> {
    let mut files = Vec::new();
    let mut current_file: Option<FileDiff> = None;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut total_additions = 0;
    let mut total_deletions = 0;

    for line in output.lines() {
        if line.starts_with("diff --git") {
            // Save previous file
            if let Some(mut file) = current_file.take() {
                if let Some(hunk) = current_hunk.take() {
                    file.hunks.push(hunk);
                }
                files.push(file);
            }
            
            // Extract filename
            let parts: Vec<&str> = line.split_whitespace().collect();
            let path = parts.last()
                .map(|p| p.trim_start_matches("b/"))
                .unwrap_or("")
                .to_string();
            
            current_file = Some(FileDiff {
                path,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
        } else if line.starts_with("@@") {
            // New hunk
            if let Some(ref mut file) = current_file {
                if let Some(hunk) = current_hunk.take() {
                    file.hunks.push(hunk);
                }
            }
            
            // Parse hunk header
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            current_hunk = Some(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                content: String::new(),
            });
        } else if let Some(ref mut hunk) = current_hunk {
            hunk.content.push_str(line);
            hunk.content.push('\n');
            
            if let Some(ref mut file) = current_file {
                if line.starts_with('+') && !line.starts_with("+++") {
                    file.additions += 1;
                    total_additions += 1;
                } else if line.starts_with('-') && !line.starts_with("---") {
                    file.deletions += 1;
                    total_deletions += 1;
                }
            }
        }
    }

    // Save last file
    if let Some(mut file) = current_file {
        if let Some(hunk) = current_hunk {
            file.hunks.push(hunk);
        }
        files.push(file);
    }

    Ok(DiffOutput {
        files,
        total_additions,
        total_deletions,
    })
}

fn parse_hunk_header(line: &str) -> (usize, usize, usize, usize) {
    // @@ -1,3 +1,4 @@
    let parts: Vec<&str> = line.split_whitespace().collect();
    
    let (old_start, old_lines) = if parts.len() > 1 {
        parse_range(parts[1].trim_start_matches('-'))
    } else {
        (0, 0)
    };
    
    let (new_start, new_lines) = if parts.len() > 2 {
        parse_range(parts[2].trim_start_matches('+'))
    } else {
        (0, 0)
    };

    (old_start, old_lines, new_start, new_lines)
}

fn parse_range(s: &str) -> (usize, usize) {
    let parts: Vec<&str> = s.split(',').collect();
    let start = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
    let lines = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(1);
    (start, lines)
}

fn parse_blame_output(output: &str) -> Result<Vec<BlameLine>, GitError> {
    let mut lines = Vec::new();
    let mut current_commit = String::new();
    let mut current_author = String::new();
    let mut current_date = String::new();
    let mut line_number = 0;

    for line in output.lines() {
        if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
            current_commit = line[..8].to_string();
        } else if let Some(author) = line.strip_prefix("author ") {
            current_author = author.to_string();
        } else if let Some(date) = line.strip_prefix("author-time ") {
            current_date = date.to_string();
        } else if let Some(content) = line.strip_prefix('\t') {
            line_number += 1;
            lines.push(BlameLine {
                commit: current_commit.clone(),
                author: current_author.clone(),
                date: current_date.clone(),
                line_number,
                content: content.to_string(),
            });
        }
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type() {
        assert!(matches!(char_to_change_type('A'), ChangeType::Added));
        assert!(matches!(char_to_change_type('M'), ChangeType::Modified));
        assert!(matches!(char_to_change_type('D'), ChangeType::Deleted));
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("1,3"), (1, 3));
        assert_eq!(parse_range("5"), (5, 1));
    }
}
