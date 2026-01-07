//! Shell executor tool - Execute shell commands safely

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Shell command arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellArgs {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub working_dir: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub timeout_secs: Option<u64>,
    pub capture_stderr: Option<bool>,
    pub shell: Option<String>,
}

/// Shell execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub command: String,
}

/// Streaming output line
#[derive(Debug, Clone)]
pub enum OutputLine {
    Stdout(String),
    Stderr(String),
}

/// Shell executor tool
#[derive(Debug, Clone)]
pub struct ShellExecutorTool {
    allowed_commands: Option<Vec<String>>,
    blocked_commands: Vec<String>,
    default_timeout: u64,
}

impl Default for ShellExecutorTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecutorTool {
    pub const NAME: &'static str = "execute_shell";

    pub fn new() -> Self {
        Self {
            allowed_commands: None,
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "rm -rf /*".to_string(),
                "mkfs".to_string(),
                "dd if=/dev".to_string(),
                ":(){:|:&};:".to_string(), // Fork bomb
                "chmod -R 777 /".to_string(),
            ],
            default_timeout: 300, // 5 minutes
        }
    }

    /// Create with restricted command set
    pub fn restricted(allowed: Vec<String>) -> Self {
        Self {
            allowed_commands: Some(allowed),
            blocked_commands: vec![],
            default_timeout: 300,
        }
    }

    /// Execute a shell command
    pub async fn execute(&self, args: ShellArgs) -> Result<ShellResult, ShellError> {
        // Security checks
        self.validate_command(&args.command)?;

        let timeout = args.timeout_secs.unwrap_or(self.default_timeout);
        let shell = args.shell.as_deref().unwrap_or("sh");

        let mut cmd = Command::new(shell);
        cmd.arg("-c").arg(&args.command);

        // Set working directory
        if let Some(ref dir) = args.working_dir {
            let path = PathBuf::from(dir);
            if !path.exists() {
                return Err(ShellError::WorkingDirNotFound(dir.clone()));
            }
            cmd.current_dir(path);
        }

        // Set environment variables
        if let Some(ref env) = args.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            cmd.output()
        ).await
            .map_err(|_| ShellError::Timeout(timeout))?
            .map_err(|e| ShellError::ExecutionError(e.to_string()))?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(ShellResult {
            exit_code,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            elapsed_ms,
            command: args.command,
        })
    }

    /// Execute with streaming output
    pub async fn execute_streaming<F>(
        &self, 
        args: ShellArgs,
        mut callback: F
    ) -> Result<ShellResult, ShellError> 
    where
        F: FnMut(OutputLine) + Send,
    {
        self.validate_command(&args.command)?;

        let shell = args.shell.as_deref().unwrap_or("sh");
        let mut cmd = Command::new(shell);
        cmd.arg("-c").arg(&args.command);

        if let Some(ref dir) = args.working_dir {
            cmd.current_dir(dir);
        }

        if let Some(ref env) = args.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let mut child = cmd.spawn()
            .map_err(|e| ShellError::ExecutionError(e.to_string()))?;

        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut stdout_content = String::new();
        let mut stderr_content = String::new();

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            stdout_content.push_str(&line);
                            stdout_content.push('\n');
                            callback(OutputLine::Stdout(line));
                        }
                        Ok(None) => break,
                        Err(e) => return Err(ShellError::ExecutionError(e.to_string())),
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            stderr_content.push_str(&line);
                            stderr_content.push('\n');
                            callback(OutputLine::Stderr(line));
                        }
                        Ok(None) => {}
                        Err(e) => return Err(ShellError::ExecutionError(e.to_string())),
                    }
                }
            }
        }

        let status = child.wait().await
            .map_err(|e| ShellError::ExecutionError(e.to_string()))?;
        
        let elapsed_ms = start.elapsed().as_millis() as u64;

        Ok(ShellResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_content,
            stderr: stderr_content,
            success: status.success(),
            elapsed_ms,
            command: args.command,
        })
    }

    /// Run multiple commands in sequence
    pub async fn run_sequence(&self, commands: Vec<ShellArgs>) -> Result<Vec<ShellResult>, ShellError> {
        let mut results = Vec::new();
        
        for args in commands {
            let result = self.execute(args).await?;
            let failed = !result.success;
            results.push(result);
            
            if failed {
                break;
            }
        }
        
        Ok(results)
    }

    /// Run commands in parallel
    pub async fn run_parallel(&self, commands: Vec<ShellArgs>) -> Vec<Result<ShellResult, ShellError>> {
        let futures: Vec<_> = commands.into_iter()
            .map(|args| self.execute(args))
            .collect();
        
        futures::future::join_all(futures).await
    }

    fn validate_command(&self, command: &str) -> Result<(), ShellError> {
        // Check blocked commands
        for blocked in &self.blocked_commands {
            if command.contains(blocked) {
                return Err(ShellError::BlockedCommand(command.to_string()));
            }
        }

        // Check allowed commands if restricted
        if let Some(ref allowed) = self.allowed_commands {
            let cmd_base = command.split_whitespace().next().unwrap_or("");
            if !allowed.iter().any(|a| cmd_base == a || cmd_base.ends_with(&format!("/{}", a))) {
                return Err(ShellError::NotAllowed(command.to_string()));
            }
        }

        Ok(())
    }
}

/// Convenience functions for common commands
impl ShellExecutorTool {
    /// Run `ls` command
    pub async fn ls(&self, path: Option<&str>, flags: Option<&str>) -> Result<ShellResult, ShellError> {
        let cmd = match (path, flags) {
            (Some(p), Some(f)) => format!("ls {} {}", f, p),
            (Some(p), None) => format!("ls {}", p),
            (None, Some(f)) => format!("ls {}", f),
            (None, None) => "ls".to_string(),
        };
        
        self.execute(ShellArgs {
            command: cmd,
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(30),
            capture_stderr: None,
            shell: None,
        }).await
    }

    /// Run `cat` command
    pub async fn cat(&self, file: &str) -> Result<ShellResult, ShellError> {
        self.execute(ShellArgs {
            command: format!("cat {}", file),
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(30),
            capture_stderr: None,
            shell: None,
        }).await
    }

    /// Run `grep` command
    pub async fn grep(&self, pattern: &str, path: &str, recursive: bool) -> Result<ShellResult, ShellError> {
        let cmd = if recursive {
            format!("grep -r '{}' {}", pattern, path)
        } else {
            format!("grep '{}' {}", pattern, path)
        };
        
        self.execute(ShellArgs {
            command: cmd,
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(60),
            capture_stderr: None,
            shell: None,
        }).await
    }

    /// Run `find` command
    pub async fn find(&self, path: &str, name: Option<&str>, type_: Option<&str>) -> Result<ShellResult, ShellError> {
        let mut cmd = format!("find {}", path);
        
        if let Some(n) = name {
            cmd.push_str(&format!(" -name '{}'", n));
        }
        
        if let Some(t) = type_ {
            cmd.push_str(&format!(" -type {}", t));
        }
        
        self.execute(ShellArgs {
            command: cmd,
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(60),
            capture_stderr: None,
            shell: None,
        }).await
    }

    /// Run `which` command
    pub async fn which(&self, program: &str) -> Result<ShellResult, ShellError> {
        self.execute(ShellArgs {
            command: format!("which {}", program),
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(10),
            capture_stderr: None,
            shell: None,
        }).await
    }

    /// Get environment variable
    pub async fn get_env(&self, var: &str) -> Result<String, ShellError> {
        let result = self.execute(ShellArgs {
            command: format!("echo ${}", var),
            args: None,
            working_dir: None,
            env: None,
            timeout_secs: Some(10),
            capture_stderr: None,
            shell: None,
        }).await?;
        
        Ok(result.stdout.trim().to_string())
    }
}

/// Shell executor errors
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("Command blocked for security: {0}")]
    BlockedCommand(String),
    #[error("Command not allowed: {0}")]
    NotAllowed(String),
    #[error("Working directory not found: {0}")]
    WorkingDirNotFound(String),
    #[error("Execution error: {0}")]
    ExecutionError(String),
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_commands() {
        let executor = ShellExecutorTool::new();
        assert!(executor.validate_command("rm -rf /").is_err());
        assert!(executor.validate_command("echo hello").is_ok());
    }

    #[test]
    fn test_restricted_mode() {
        let executor = ShellExecutorTool::restricted(vec!["echo".to_string(), "ls".to_string()]);
        assert!(executor.validate_command("echo hello").is_ok());
        assert!(executor.validate_command("rm file").is_err());
    }
}
