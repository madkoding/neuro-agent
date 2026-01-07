//! Self-correction loop for automatic error recovery

use crate::tools::{CommandOutput, LinterOutput};
use std::collections::HashMap;

/// Maximum number of retry attempts
pub const MAX_RETRIES: usize = 3;

/// Context for self-correction
#[derive(Debug, Clone)]
pub struct CorrectionContext {
    /// The original command or action
    pub original_action: String,
    /// Error output from the failed attempt
    pub error_output: String,
    /// Exit code (for commands)
    pub exit_code: Option<i32>,
    /// Previous correction attempts
    pub previous_attempts: Vec<CorrectionAttempt>,
    /// Relevant file contents
    pub file_context: HashMap<String, String>,
}

/// A single correction attempt
#[derive(Debug, Clone)]
pub struct CorrectionAttempt {
    /// What was tried
    pub action: String,
    /// The result
    pub result: String,
    /// Whether it succeeded
    pub success: bool,
}

/// Self-correction loop handler
pub struct SelfCorrectionLoop {
    max_retries: usize,
}

impl Default for SelfCorrectionLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfCorrectionLoop {
    pub fn new() -> Self {
        Self {
            max_retries: MAX_RETRIES,
        }
    }

    pub fn with_max_retries(mut self, max: usize) -> Self {
        self.max_retries = max;
        self
    }

    /// Check if a command output indicates failure
    pub fn is_failure(&self, output: &CommandOutput) -> bool {
        !output.success || output.exit_code != 0
    }

    /// Check if linter output indicates errors
    pub fn has_errors(&self, output: &LinterOutput) -> bool {
        !output.success || output.error_count > 0
    }

    /// Extract error information from command output
    pub fn extract_error_info(&self, output: &CommandOutput) -> String {
        let mut info = String::new();

        if !output.stderr.is_empty() {
            info.push_str("STDERR:\n");
            info.push_str(&output.stderr);
            info.push('\n');
        }

        if !output.stdout.is_empty() && !output.success {
            info.push_str("STDOUT:\n");
            info.push_str(&output.stdout);
            info.push('\n');
        }

        info.push_str(&format!("Exit code: {}\n", output.exit_code));

        info
    }

    /// Extract error information from linter output
    pub fn extract_linter_errors(&self, output: &LinterOutput) -> String {
        let mut info = String::new();

        info.push_str(&format!(
            "Errors: {}, Warnings: {}\n\n",
            output.error_count, output.warning_count
        ));

        for diag in &output.diagnostics {
            if diag.level == "error" {
                if let (Some(file), Some(line)) = (&diag.file, diag.line) {
                    info.push_str(&format!("{}:{}: ", file, line));
                }
                info.push_str(&format!("{}\n", diag.message));
            }
        }

        info
    }

    /// Generate a correction prompt for the LLM
    pub fn generate_correction_prompt(&self, context: &CorrectionContext) -> String {
        let mut prompt = String::new();

        prompt.push_str("The following action failed and needs correction:\n\n");
        prompt.push_str(&format!("Original action: {}\n\n", context.original_action));
        prompt.push_str(&format!("Error output:\n{}\n\n", context.error_output));

        if let Some(code) = context.exit_code {
            prompt.push_str(&format!("Exit code: {}\n\n", code));
        }

        if !context.previous_attempts.is_empty() {
            prompt.push_str("Previous correction attempts:\n");
            for (i, attempt) in context.previous_attempts.iter().enumerate() {
                prompt.push_str(&format!(
                    "Attempt {}: {} -> {}\n",
                    i + 1,
                    attempt.action,
                    if attempt.success {
                        "success"
                    } else {
                        "failed"
                    }
                ));
            }
            prompt.push('\n');
        }

        if !context.file_context.is_empty() {
            prompt.push_str("Relevant file contents:\n");
            for (path, content) in &context.file_context {
                prompt.push_str(&format!("=== {} ===\n{}\n\n", path, content));
            }
        }

        prompt.push_str(
            "Please analyze the error and provide a corrected version. \
             Focus on fixing the specific issue mentioned in the error output.",
        );

        prompt
    }

    /// Check if we should retry
    pub fn should_retry(&self, attempts: usize) -> bool {
        attempts < self.max_retries
    }

    /// Get remaining retry attempts
    pub fn remaining_attempts(&self, current: usize) -> usize {
        self.max_retries.saturating_sub(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_failure() {
        let loop_handler = SelfCorrectionLoop::new();

        let success = CommandOutput {
            stdout: "ok".to_string(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
            command: "echo ok".to_string(),
        };
        assert!(!loop_handler.is_failure(&success));

        let failure = CommandOutput {
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: 1,
            success: false,
            command: "exit 1".to_string(),
        };
        assert!(loop_handler.is_failure(&failure));
    }

    #[test]
    fn test_should_retry() {
        let loop_handler = SelfCorrectionLoop::new();

        assert!(loop_handler.should_retry(0));
        assert!(loop_handler.should_retry(1));
        assert!(loop_handler.should_retry(2));
        assert!(!loop_handler.should_retry(3));
    }
}
