//! Shell command execution tool with security scanning

use crate::security::{CommandScanner, RiskLevel};
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

// ============================================================================
// Errors
// ============================================================================

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Command blocked for security: {0}")]
    SecurityBlocked(String),
    #[error("Command requires confirmation")]
    RequiresConfirmation(RiskLevel),
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
    #[error("Command failed with exit code {0}: {1}")]
    ExitError(i32, String),
}

// ============================================================================
// ShellExecuteTool
// ============================================================================

/// Tool for executing shell commands
#[derive(Debug, Clone)]
pub struct ShellExecuteTool {
    scanner: CommandScanner,
    /// Maximum execution time in seconds (default: 1200s = 20 minutes)
    timeout_secs: u64,
    /// Working directory for commands
    working_dir: Option<String>,
    /// Whether to skip security checks (dangerous!)
    skip_security: bool,
}

impl Default for ShellExecuteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellExecuteTool {
    pub fn new() -> Self {
        Self {
            scanner: CommandScanner::new(),
            timeout_secs: 1200,
            working_dir: None,
            skip_security: false,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_working_dir(mut self, dir: String) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Check if a command is safe to execute
    pub fn check_security(&self, command: &str) -> Result<RiskLevel, CommandError> {
        if self.skip_security {
            return Ok(RiskLevel::Safe);
        }

        let risk = self.scanner.scan(command);

        if risk.is_blocked() {
            return Err(CommandError::SecurityBlocked(
                self.scanner
                    .get_warning(command)
                    .unwrap_or_else(|| "Command blocked".to_string()),
            ));
        }

        Ok(risk)
    }

    /// Execute a command without security checks (use with caution)
    pub async fn execute_unchecked(&self, command: &str) -> Result<CommandOutput, CommandError> {
        self.run_command(command).await
    }

    async fn run_command(&self, command: &str) -> Result<CommandOutput, CommandError> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        let future = async {
            let output = cmd.output().await?;
            Ok::<_, std::io::Error>(output)
        };

        let output = timeout(Duration::from_secs(self.timeout_secs), future)
            .await
            .map_err(|_| CommandError::Timeout(self.timeout_secs))??;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
            success: output.status.success(),
            command: command.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShellExecuteArgs {
    /// The shell command to execute
    pub command: String,
    /// Optional working directory
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Timeout in seconds (max 1200)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code
    pub exit_code: i32,
    /// Whether the command succeeded
    pub success: bool,
    /// The command that was executed
    pub command: String,
}

impl Tool for ShellExecuteTool {
    const NAME: &'static str = "execute_shell";

    type Args = ShellExecuteArgs;
    type Output = CommandOutput;
    type Error = CommandError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a shell command. Commands are scanned for security risks. \
                         Dangerous commands may be blocked or require confirmation."
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ShellExecuteArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Security check
        let risk = self.check_security(&args.command)?;

        if risk.requires_confirmation() {
            return Err(CommandError::RequiresConfirmation(risk));
        }

        // Create a new instance with the provided timeout and working dir
        let executor = Self {
            scanner: self.scanner.clone(),
            timeout_secs: args.timeout_secs.min(1200),
            working_dir: args.working_dir.or_else(|| self.working_dir.clone()),
            skip_security: self.skip_security,
        };

        executor.run_command(&args.command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safe_command() {
        let tool = ShellExecuteTool::new();
        let result = tool
            .call(ShellExecuteArgs {
                command: "echo 'Hello, World!'".to_string(),
                working_dir: None,
                timeout_secs: 10,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_command_with_exit_code() {
        let tool = ShellExecuteTool::new();
        let result = tool
            .call(ShellExecuteArgs {
                command: "exit 1".to_string(),
                working_dir: None,
                timeout_secs: 10,
            })
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_security_check() {
        let tool = ShellExecuteTool::new();

        // Safe command
        assert!(matches!(
            tool.check_security("ls -la"),
            Ok(RiskLevel::Safe)
        ));

        // Dangerous command
        assert!(matches!(
            tool.check_security("rm -rf /"),
            Err(CommandError::SecurityBlocked(_))
        ));
    }
}
