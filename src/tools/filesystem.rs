//! Filesystem tools for reading, writing, and listing files

use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tokio::fs;

// ============================================================================
// Errors
// ============================================================================

#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path does not exist: {0}")]
    PathNotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

// ============================================================================
// FileReadTool
// ============================================================================

/// Tool for reading file contents
#[derive(Debug, Clone, Default)]
pub struct FileReadTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileReadArgs {
    /// The absolute path to the file to read
    pub path: String,
    /// Optional: start line (1-indexed). If not provided, reads from beginning.
    #[serde(default)]
    pub start_line: Option<usize>,
    /// Optional: end line (1-indexed, inclusive). If not provided, reads to end.
    #[serde(default)]
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadOutput {
    /// The content of the file
    pub content: String,
    /// Total number of lines in the file
    pub total_lines: usize,
    /// Lines actually read (start..=end)
    pub lines_read: String,
}

impl Tool for FileReadTool {
    const NAME: &'static str = "read_file";

    type Args = FileReadArgs;
    type Output = FileReadOutput;
    type Error = FileSystemError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read the contents of a file. You can optionally specify line ranges."
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(FileReadArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = Path::new(&args.path);

        if !path.exists() {
            return Err(FileSystemError::PathNotFound(args.path));
        }

        let content = fs::read_to_string(path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = args.start_line.unwrap_or(1).saturating_sub(1);
        let end = args.end_line.unwrap_or(total_lines).min(total_lines);

        let selected_lines: Vec<&str> = lines
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .copied()
            .collect();

        Ok(FileReadOutput {
            content: selected_lines.join("\n"),
            total_lines,
            lines_read: format!("{}-{}", start + 1, start + selected_lines.len()),
        })
    }
}

// ============================================================================
// FileWriteTool
// ============================================================================

/// Tool for writing content to files
#[derive(Debug, Clone, Default)]
pub struct FileWriteTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileWriteArgs {
    /// The absolute path to the file to write
    pub path: String,
    /// The content to write to the file
    pub content: String,
    /// If true, append to the file instead of overwriting
    #[serde(default)]
    pub append: bool,
    /// If true, create parent directories if they don't exist
    #[serde(default = "default_true")]
    pub create_dirs: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteOutput {
    /// Whether the write was successful
    pub success: bool,
    /// The path that was written to
    pub path: String,
    /// Number of bytes written
    pub bytes_written: usize,
}

impl Tool for FileWriteTool {
    const NAME: &'static str = "write_file";

    type Args = FileWriteArgs;
    type Output = FileWriteOutput;
    type Error = FileSystemError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Write content to a file. Can create new files or overwrite/append to existing ones.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(FileWriteArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = Path::new(&args.path);

        // Create parent directories if needed
        if args.create_dirs {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
        }

        let bytes_written = args.content.len();

        if args.append {
            use tokio::io::AsyncWriteExt;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?;
            file.write_all(args.content.as_bytes()).await?;
        } else {
            fs::write(path, &args.content).await?;
        }

        Ok(FileWriteOutput {
            success: true,
            path: args.path,
            bytes_written,
        })
    }
}

// ============================================================================
// ListDirectoryTool
// ============================================================================

/// Tool for listing directory contents
#[derive(Debug, Clone, Default)]
pub struct ListDirectoryTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListDirectoryArgs {
    /// The absolute path to the directory to list
    pub path: String,
    /// If true, list recursively (max depth 3)
    #[serde(default)]
    pub recursive: bool,
    /// Maximum depth for recursive listing (default: 3)
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

fn default_max_depth() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// Name of the entry
    pub name: String,
    /// Full path
    pub path: String,
    /// Whether it's a directory
    pub is_dir: bool,
    /// Size in bytes (for files)
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDirectoryOutput {
    /// List of entries in the directory
    pub entries: Vec<DirEntry>,
    /// Total count of entries
    pub count: usize,
}

impl Tool for ListDirectoryTool {
    const NAME: &'static str = "list_directory";

    type Args = ListDirectoryArgs;
    type Output = ListDirectoryOutput;
    type Error = FileSystemError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List the contents of a directory. Can optionally list recursively."
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ListDirectoryArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = Path::new(&args.path);

        if !path.exists() {
            return Err(FileSystemError::PathNotFound(args.path));
        }

        if !path.is_dir() {
            return Err(FileSystemError::InvalidPath(format!(
                "{} is not a directory",
                args.path
            )));
        }

        let mut entries = Vec::new();
        list_dir_recursive(path, &mut entries, 0, args.max_depth, args.recursive).await?;

        let count = entries.len();
        Ok(ListDirectoryOutput { entries, count })
    }
}

async fn list_dir_recursive(
    path: &Path,
    entries: &mut Vec<DirEntry>,
    current_depth: usize,
    max_depth: usize,
    recursive: bool,
) -> Result<(), FileSystemError> {
    let mut read_dir = fs::read_dir(path).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let is_dir = metadata.is_dir();
        let entry_path = entry.path();

        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            size: if is_dir { None } else { Some(metadata.len()) },
        });

        if recursive && is_dir && current_depth < max_depth {
            Box::pin(list_dir_recursive(
                &entry_path,
                entries,
                current_depth + 1,
                max_depth,
                recursive,
            ))
            .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_read_write() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Write
        let write_tool = FileWriteTool;
        let result = write_tool
            .call(FileWriteArgs {
                path: file_path.to_string_lossy().to_string(),
                content: "Hello, World!\nLine 2\nLine 3".to_string(),
                append: false,
                create_dirs: true,
            })
            .await
            .unwrap();

        assert!(result.success);

        // Read
        let read_tool = FileReadTool;
        let result = read_tool
            .call(FileReadArgs {
                path: file_path.to_string_lossy().to_string(),
                start_line: None,
                end_line: None,
            })
            .await
            .unwrap();

        assert_eq!(result.total_lines, 3);
        assert!(result.content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_list_directory() {
        let dir = tempdir().unwrap();

        // Create some test files
        fs::write(dir.path().join("file1.txt"), "content1")
            .await
            .unwrap();
        fs::write(dir.path().join("file2.txt"), "content2")
            .await
            .unwrap();
        fs::create_dir(dir.path().join("subdir")).await.unwrap();

        let tool = ListDirectoryTool;
        let result = tool
            .call(ListDirectoryArgs {
                path: dir.path().to_string_lossy().to_string(),
                recursive: false,
                max_depth: 3,
            })
            .await
            .unwrap();

        assert_eq!(result.count, 3);
    }
}
