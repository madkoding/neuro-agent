//! Linter tool for running cargo clippy and cargo check

use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;

// ============================================================================
// Errors
// ============================================================================

#[derive(Error, Debug)]
pub enum LinterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a Rust project: {0}")]
    NotRustProject(String),
    #[error("Linter execution failed: {0}")]
    ExecutionFailed(String),
}

// ============================================================================
// LinterTool
// ============================================================================

/// Tool for running Rust linters (clippy, check)
#[derive(Debug, Clone, Default)]
pub struct LinterTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub enum LinterMode {
    /// Run cargo check (fast compilation check)
    Check,
    /// Run cargo clippy (detailed linting)
    #[default]
    Clippy,
    /// Run cargo test --no-run (check tests compile)
    TestCompile,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinterArgs {
    /// Path to the Rust project (directory containing Cargo.toml)
    pub project_path: String,
    /// Linting mode: check, clippy, or test_compile
    #[serde(default)]
    pub mode: LinterMode,
    /// Additional arguments to pass to the linter
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Whether to fix issues automatically (clippy only)
    #[serde(default)]
    pub auto_fix: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinterDiagnostic {
    /// Type of diagnostic: error, warning, note, help
    pub level: String,
    /// The diagnostic message
    pub message: String,
    /// File path where the issue was found
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Column number
    pub column: Option<u32>,
    /// Suggested fix (if available)
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinterOutput {
    /// Whether the linting passed (no errors)
    pub success: bool,
    /// Number of errors found
    pub error_count: usize,
    /// Number of warnings found
    pub warning_count: usize,
    /// Raw output from the linter
    pub raw_output: String,
    /// Parsed diagnostics
    pub diagnostics: Vec<LinterDiagnostic>,
    /// The command that was run
    pub command: String,
}

impl Tool for LinterTool {
    const NAME: &'static str = "run_linter";

    type Args = LinterArgs;
    type Output = LinterOutput;
    type Error = LinterError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run Rust linters (cargo check or cargo clippy) on a project. \
                         Returns compilation errors, warnings, and suggestions for fixes."
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(LinterArgs)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let project_path = Path::new(&args.project_path);

        // Verify it's a Rust project
        if !project_path.join("Cargo.toml").exists() {
            return Err(LinterError::NotRustProject(args.project_path));
        }

        // Build the command
        let (command_name, mut command_args) = match args.mode {
            LinterMode::Check => ("cargo", vec!["check", "--message-format=short"]),
            LinterMode::Clippy => {
                let mut clippy_args = vec!["clippy", "--message-format=short"];
                if args.auto_fix {
                    clippy_args.push("--fix");
                    clippy_args.push("--allow-dirty");
                }
                ("cargo", clippy_args)
            }
            LinterMode::TestCompile => {
                ("cargo", vec!["test", "--no-run", "--message-format=short"])
            }
        };

        // Add extra args
        for arg in &args.extra_args {
            command_args.push(arg.as_str());
        }

        let command_str = format!("{} {}", command_name, command_args.join(" "));

        // Run the command
        let mut cmd = Command::new(command_name);
        cmd.args(&command_args)
            .current_dir(project_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_output = format!("{}\n{}", stdout, stderr);

        // Parse diagnostics
        let (diagnostics, error_count, warning_count) = parse_cargo_output(&combined_output);

        Ok(LinterOutput {
            success: output.status.success(),
            error_count,
            warning_count,
            raw_output: combined_output,
            diagnostics,
            command: command_str,
        })
    }
}

/// Parse cargo output to extract diagnostics
fn parse_cargo_output(output: &str) -> (Vec<LinterDiagnostic>, usize, usize) {
    let mut diagnostics = Vec::new();
    let mut error_count = 0;
    let mut warning_count = 0;

    for line in output.lines() {
        let line_lower = line.to_lowercase();

        if line_lower.contains("error") && !line_lower.contains("aborting") {
            error_count += 1;
            if let Some(diag) = parse_diagnostic_line(line, "error") {
                diagnostics.push(diag);
            }
        } else if line_lower.contains("warning:") {
            warning_count += 1;
            if let Some(diag) = parse_diagnostic_line(line, "warning") {
                diagnostics.push(diag);
            }
        }
    }

    (diagnostics, error_count, warning_count)
}

/// Parse a single diagnostic line
fn parse_diagnostic_line(line: &str, level: &str) -> Option<LinterDiagnostic> {
    // Try to parse location format: file:line:column: level: message
    let parts: Vec<&str> = line.splitn(2, ": ").collect();

    if parts.len() >= 2 {
        let location = parts[0];
        let message = parts[1..].join(": ");

        let loc_parts: Vec<&str> = location.split(':').collect();
        let (file, line_num, column) = match loc_parts.len() {
            3.. => (
                Some(loc_parts[0].to_string()),
                loc_parts[1].parse().ok(),
                loc_parts[2].parse().ok(),
            ),
            2 => (
                Some(loc_parts[0].to_string()),
                loc_parts[1].parse().ok(),
                None,
            ),
            _ => (None, None, None),
        };

        return Some(LinterDiagnostic {
            level: level.to_string(),
            message: message.trim().to_string(),
            file,
            line: line_num,
            column,
            suggestion: None,
        });
    }

    Some(LinterDiagnostic {
        level: level.to_string(),
        message: line.to_string(),
        file: None,
        line: None,
        column: None,
        suggestion: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostic() {
        let line = "src/main.rs:10:5: error: expected `;`";
        let diag = parse_diagnostic_line(line, "error").unwrap();

        assert_eq!(diag.level, "error");
        assert_eq!(diag.file, Some("src/main.rs".to_string()));
        assert_eq!(diag.line, Some(10));
        assert_eq!(diag.column, Some(5));
    }

    #[test]
    fn test_parse_output() {
        let output = r#"
error[E0425]: cannot find value `x` in this scope
 --> src/main.rs:5:13
  |
5 |     println!("{}", x);
  |                    ^ not found in this scope

warning: unused variable: `y`
 --> src/main.rs:3:9
  |
3 |     let y = 5;
  |         ^ help: if this is intentional, prefix it with an underscore: `_y`
"#;

        let (diagnostics, errors, warnings) = parse_cargo_output(output);

        assert_eq!(errors, 1);
        assert_eq!(warnings, 1);
        assert!(!diagnostics.is_empty());
    }
}
