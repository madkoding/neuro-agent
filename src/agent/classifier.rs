//! Task type classifier for routing between fast and heavy models

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Type of task identified by the fast model for routing
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskType {
    /// Simple commands that the fast model handles directly
    SimpleCommand {
        action: SimpleAction,
    },
    /// Simple conversational queries
    SimpleChat {
        message: String,
    },
    /// Code review requests → delegate to heavy model
    CodeReview {
        file_paths: Vec<String>,
        review_type: ReviewType,
    },
    /// Code generation requests → delegate to heavy model
    CodeGeneration {
        description: String,
        language: String,
        context_files: Vec<String>,
    },
    /// Complex reasoning that requires the heavy model
    ComplexReasoning {
        query: String,
        requires_tools: bool,
    },
    /// Tool execution (can be handled by either model)
    ToolExecution {
        tool_name: String,
        reasoning: String,
    },
}

/// Simple actions that don't require LLM processing
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SimpleAction {
    ListFiles,
    ShowStatus,
    Help,
    Exit,
    ClearHistory,
    ShowHistory,
}

/// Type of code review
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewType {
    Security,
    Performance,
    BestPractices,
    Full,
    Bugs,
}

impl TaskType {
    /// Check if this task should be handled by the heavy model
    pub fn requires_heavy_model(&self) -> bool {
        matches!(
            self,
            TaskType::CodeReview { .. }
                | TaskType::CodeGeneration { .. }
                | TaskType::ComplexReasoning { .. }
        )
    }

    /// Get estimated processing time in seconds
    pub fn estimated_time(&self) -> u64 {
        match self {
            TaskType::SimpleCommand { .. } => 1,
            TaskType::SimpleChat { .. } => 5,
            TaskType::CodeReview { file_paths, .. } => 30 + (file_paths.len() as u64 * 10),
            TaskType::CodeGeneration { .. } => 60,
            TaskType::ComplexReasoning { .. } => 45,
            TaskType::ToolExecution { .. } => 10,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            TaskType::SimpleCommand { action } => format!("Executing: {:?}", action),
            TaskType::SimpleChat { .. } => "Processing chat message".to_string(),
            TaskType::CodeReview { review_type, .. } => {
                format!("Performing {:?} code review", review_type)
            }
            TaskType::CodeGeneration { language, .. } => {
                format!("Generating {} code", language)
            }
            TaskType::ComplexReasoning { .. } => "Analyzing complex query".to_string(),
            TaskType::ToolExecution { tool_name, .. } => format!("Executing tool: {}", tool_name),
        }
    }
}

/// Classifier that uses pattern matching and keywords to route tasks
pub struct TaskClassifier {
    code_review_keywords: Vec<&'static str>,
    code_gen_keywords: Vec<&'static str>,
    simple_command_patterns: Vec<(&'static str, SimpleAction)>,
}

impl Default for TaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskClassifier {
    pub fn new() -> Self {
        Self {
            code_review_keywords: vec![
                // English
                "review",
                "analyze",
                "check",
                "audit",
                "inspect",
                "examine",
                "security",
                "vulnerability",
                "bug",
                "issue",
                // Spanish
                "revisa",
                "analiza",
                "examina",
                "verifica",
                "audita",
                "inspecciona",
                "seguridad",
                "vulnerabilidad",
                "error",
                "problema",
            ],
            code_gen_keywords: vec![
                // English
                "generate",
                "create",
                "write",
                "implement",
                "build",
                "make",
                "code",
                "function",
                "class",
                "struct",
                // Spanish
                "genera",
                "crea",
                "escribe",
                "implementa",
                "construye",
                "haz",
                "hazme",
                "código",
                "función",
                "clase",
                "estructura",
                "agrega",
                "añade",
            ],
            simple_command_patterns: vec![
                // English
                ("exit", SimpleAction::Exit),
                ("quit", SimpleAction::Exit),
                ("bye", SimpleAction::Exit),
                ("help", SimpleAction::Help),
                ("?", SimpleAction::Help),
                ("clear", SimpleAction::ClearHistory),
                ("history", SimpleAction::ShowHistory),
                ("status", SimpleAction::ShowStatus),
                ("ls", SimpleAction::ListFiles),
                ("list", SimpleAction::ListFiles),
                // Spanish
                ("salir", SimpleAction::Exit),
                ("chao", SimpleAction::Exit),
                ("ayuda", SimpleAction::Help),
                ("limpiar", SimpleAction::ClearHistory),
                ("historial", SimpleAction::ShowHistory),
                ("estado", SimpleAction::ShowStatus),
                ("listar", SimpleAction::ListFiles),
                ("archivos", SimpleAction::ListFiles),
            ],
        }
    }

    /// Classify a user input into a task type using heuristics
    /// This is a fast, non-LLM classification for obvious cases
    pub fn classify_fast(&self, input: &str) -> Option<TaskType> {
        let input_lower = input.to_lowercase().trim().to_string();

        // Check for simple commands first
        for (pattern, action) in &self.simple_command_patterns {
            if input_lower == *pattern || input_lower.starts_with(&format!("{} ", pattern)) {
                return Some(TaskType::SimpleCommand {
                    action: action.clone(),
                });
            }
        }

        // Check for code review patterns
        if self.contains_keywords(&input_lower, &self.code_review_keywords)
            && (input_lower.contains("code")
                || input_lower.contains("código")
                || input_lower.contains("archivo")
                || input_lower.contains("file")
                || input_lower.contains(".rs")
                || input_lower.contains(".py")
                || input_lower.contains(".js")
                || input_lower.contains(".ts")
                || input_lower.contains(".toml")
                || input_lower.contains(".json"))
        {
            let review_type = if input_lower.contains("security") || input_lower.contains("seguridad") {
                ReviewType::Security
            } else if input_lower.contains("performance") || input_lower.contains("perf") || input_lower.contains("rendimiento") {
                ReviewType::Performance
            } else if input_lower.contains("bug") || input_lower.contains("error") {
                ReviewType::Bugs
            } else if input_lower.contains("best practice") || input_lower.contains("style") || input_lower.contains("estilo") {
                ReviewType::BestPractices
            } else {
                ReviewType::Full
            };

            // Extract file paths (simple heuristic)
            let file_paths = self.extract_file_paths(&input_lower);

            return Some(TaskType::CodeReview {
                file_paths,
                review_type,
            });
        }

        // Check for code generation patterns
        if self.contains_keywords(&input_lower, &self.code_gen_keywords) {
            let language = self.detect_language(&input_lower);

            return Some(TaskType::CodeGeneration {
                description: input.to_string(),
                language,
                context_files: vec![],
            });
        }

        // Default: let the LLM classify
        None
    }

    fn contains_keywords(&self, input: &str, keywords: &[&str]) -> bool {
        keywords.iter().any(|kw| input.contains(kw))
    }

    fn extract_file_paths(&self, input: &str) -> Vec<String> {
        let mut paths = Vec::new();

        for word in input.split_whitespace() {
            if word.contains('.')
                && (word.ends_with(".rs")
                    || word.ends_with(".py")
                    || word.ends_with(".js")
                    || word.ends_with(".ts")
                    || word.ends_with(".go")
                    || word.ends_with(".java")
                    || word.contains('/'))
            {
                paths.push(word.to_string());
            }
        }

        paths
    }

    fn detect_language(&self, input: &str) -> String {
        if input.contains("rust") || input.contains(".rs") {
            "rust".to_string()
        } else if input.contains("python") || input.contains(".py") {
            "python".to_string()
        } else if input.contains("javascript") || input.contains(".js") {
            "javascript".to_string()
        } else if input.contains("typescript") || input.contains(".ts") {
            "typescript".to_string()
        } else if input.contains("go ") || input.contains("golang") {
            "go".to_string()
        } else {
            "unknown".to_string()
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_commands() {
        let classifier = TaskClassifier::new();

        assert!(matches!(
            classifier.classify_fast("exit"),
            Some(TaskType::SimpleCommand {
                action: SimpleAction::Exit
            })
        ));

        assert!(matches!(
            classifier.classify_fast("help"),
            Some(TaskType::SimpleCommand {
                action: SimpleAction::Help
            })
        ));
    }

    #[test]
    fn test_code_review_detection() {
        let classifier = TaskClassifier::new();

        let result = classifier.classify_fast("review the code in main.rs for security issues");
        assert!(matches!(result, Some(TaskType::CodeReview { .. })));
    }

    #[test]
    fn test_code_generation_detection() {
        let classifier = TaskClassifier::new();

        let result = classifier.classify_fast("generate a rust function to parse JSON");
        assert!(matches!(result, Some(TaskType::CodeGeneration { .. })));
    }
}
