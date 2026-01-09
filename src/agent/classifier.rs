//! Task type classifier for routing between fast and heavy models
//!
//! ## Classification Philosophy (v2.0)
//!
//! Instead of using fixed keyword patterns that don't understand context,
//! we now use a **minimal classification + intelligent delegation** approach:
//!
//! 1. **Simple Commands**: Only classify truly unambiguous commands (exit, help)
//! 2. **Code Generation**: Only when EXPLICITLY asking to generate/create code
//! 3. **Everything Else → SimpleChat**: Delegate to the multi-layer system which:
//!    - Has proactive tool execution
//!    - Has native function calling (95% confidence)
//!    - Has pattern matching fallback
//!    - Has context-aware LLM with tools
//!

#![allow(dead_code)]
//! This approach lets the **context** determine the action, not rigid keywords.
//!
//! Examples:
//! - "analiza este repositorio" → SimpleChat (will use tools proactively)
//! - "analiza el archivo main.rs para bugs" → SimpleChat (will use linter tool)
//! - "qué hace este proyecto" → SimpleChat (will read README/Cargo.toml)
//! - "genera una función para parsear JSON" → CodeGeneration (explicit)

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Query complexity level for routing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryComplexity {
    /// General questions without code context (math, definitions, casual chat)
    /// Route to: Fast model, no indexing required
    General,
    /// ANY code-related query (from simple lookups to complex refactoring)
    /// Route to: Heavy model + RAPTOR for full project context
    /// 
    /// PHILOSOPHY: Even "simple" code queries like "qué hace main.rs" benefit from
    /// understanding the project context, dependencies, and architecture.
    /// There's no such thing as a code query that doesn't need context.
    CodeContext,
}

/// Type of task identified by the fast model for routing
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskType {
    /// Simple commands that the fast model handles directly
    SimpleCommand { action: SimpleAction },
    /// Simple conversational queries
    SimpleChat { message: String },
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
    ComplexReasoning { query: String, requires_tools: bool },
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

/// Classifier that uses minimal pattern matching to route tasks
/// Most queries go to SimpleChat which has the full multi-layer system
pub struct TaskClassifier {
    simple_command_patterns: Vec<(&'static str, SimpleAction)>,
    // NOTE: Removed code_review_keywords and code_gen_keywords
    // We now rely on SimpleChat's multi-layer system with tools
    // instead of rigid keyword matching that doesn't understand context
}

impl Default for TaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskClassifier {
    pub fn new() -> Self {
        Self {
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

    /// Fast classification using simple pattern matching for obvious cases
    /// Returns None for ambiguous cases that should go through SimpleChat's multi-layer system
    /// 
    /// DESIGN: We removed rigid keyword matching (code_review_keywords, code_gen_keywords)
    /// because they don't understand context. For example:
    ///   "analiza este repositorio" → should explore with tools (SimpleChat)
    ///   "analiza la seguridad de auth.rs" → should review specific file (SimpleChat)
    /// 
    /// The multi-layer system in SimpleChat with proactive tool execution handles
    /// context-aware decisions better than fixed patterns ever could.
    pub fn classify_fast(&self, input: &str) -> Option<TaskType> {
        let input_lower = input.to_lowercase().trim().to_string();

        // Check for simple commands first (estas son muy claras, no ambiguas)
        for (pattern, action) in &self.simple_command_patterns {
            if input_lower == *pattern || input_lower.starts_with(&format!("{} ", pattern)) {
                return Some(TaskType::SimpleCommand {
                    action: action.clone(),
                });
            }
        }

        // ONLY classify truly unambiguous code generation requests
        // Must be EXPLICIT about generating new code with type specification
        if (input_lower.contains("genera") || input_lower.contains("generate") 
            || input_lower.contains("crea una función") || input_lower.contains("create a function")
            || input_lower.contains("escribe una clase") || input_lower.contains("write a class"))
            && (input_lower.contains("función") || input_lower.contains("function")
                || input_lower.contains("clase") || input_lower.contains("class")
                || input_lower.contains("struct") || input_lower.contains("método"))
        {
            let language = self.detect_language(&input_lower);
            return Some(TaskType::CodeGeneration {
                description: input.to_string(),
                language,
                context_files: vec![],
            });
        }

        // Everything else: None → SimpleChat with full multi-layer system
        // This includes:
        //   - "analiza este repositorio" → proactive_tool_execution() will fetch context
        //   - "revisa la seguridad" → native tools will handle analysis
        //   - "qué hace main.rs" → semantic_search + read_file tools
        //   - ambiguous queries → LLM decides what tools to use
        None
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

    /// Classify query complexity to determine routing strategy
    /// 
    /// SIMPLIFIED PHILOSOPHY: Only 2 categories
    /// - General: Pure math, casual chat, no code context
    /// - CodeContext: EVERYTHING related to code (always needs full project context)
    pub fn classify_complexity(&self, input: &str) -> QueryComplexity {
        let input_lower = input.to_lowercase().trim().to_string();

        // === GENERAL (no code context needed) ===
        
        // Math expressions (pure calculation, no code)
        if self.is_pure_math_expression(&input_lower) {
            return QueryComplexity::General;
        }

        // Casual conversation (greetings only at start)
        let casual_patterns = [
            "hola", "hello", "hi", "hey", "buenos días", "good morning",
            "qué tal", "how are you", "cómo estás",
            "gracias", "thank", "thanks",
        ];
        if casual_patterns.iter().any(|p| input_lower.starts_with(p)) && input_lower.len() < 30 {
            return QueryComplexity::General;
        }

        // Pure theoretical questions without code context
        let definition_words = ["qué es", "what is", "define", "definition"];
        let has_definition = definition_words.iter().any(|w| input_lower.contains(w));
        let has_code_keywords = self.has_code_keywords(&input_lower);
        let has_project_intent = self.has_project_intent(&input_lower);
        
        if has_definition && !has_code_keywords && !has_project_intent {
            return QueryComplexity::General;
        }

        // === CODE CONTEXT (everything else) ===
        // 
        // RULE: If the query mentions files, code, project, architecture, functions,
        // or ANY code-related concept → use heavy model + RAPTOR
        // 
        // Why? Even "simple" queries like "muestra main.rs" benefit from understanding:
        // - Project structure and dependencies
        // - How this file fits in the architecture
        // - Related components and patterns
        // 
        // There's no such thing as a code query that doesn't need context.
        
        if has_code_keywords || has_project_intent {
            return QueryComplexity::CodeContext;
        }

        // File mentions (explicit extensions)
        let file_indicators = [
            ".rs", ".py", ".js", ".ts", ".go", ".java", ".c", ".cpp",
            "archivo", "file", "src/", "main", "lib", "test",
        ];
        if file_indicators.iter().any(|p| input_lower.contains(p)) {
            return QueryComplexity::CodeContext;
        }

        // Default: if we're not sure, assume it's code-related for safety
        // Better to have extra context than to miss important information
        QueryComplexity::CodeContext
    }

    /// Check if input is a pure math expression (no code context)
    fn is_pure_math_expression(&self, input: &str) -> bool {
        // Simple heuristic: contains numbers and operators, no code words
        let has_numbers = input.chars().any(|c| c.is_ascii_digit());
        let has_operators = input.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '=' | '^' | '%'));
        let no_code_words = !self.has_code_keywords(input);
        
        has_numbers && has_operators && no_code_words && input.len() < 50
    }

    /// Check if input implies project/code intent (even without explicit keywords)
    fn has_project_intent(&self, input: &str) -> bool {
        let project_patterns = [
            // Direct references
            "este proyecto", "this project",
            "el proyecto", "the project",
            "este código", "this code",
            "el código", "the code",
            "esta aplicación", "this application",
            "la aplicación", "the application",
            "este programa", "this program",
            "el programa", "the program",
            "este repositorio", "this repository", "this repo",
            "el repositorio", "the repository", "the repo",
            // Analysis requests (always about code/project)
            "analiza este", "analyze this",
            "analiza el", "analyze the",
            "revisa este", "review this",
            "explica este", "explain this",
        ];
        
        project_patterns.iter().any(|p| input.contains(p))
    }

    /// Check if input contains code-related keywords
    fn has_code_keywords(&self, input: &str) -> bool {
        let code_keywords = [
            // Generic
            "código", "code", "función", "function", "clase", "class",
            "archivo", "file", "módulo", "module", "struct", "estructura",
            // Extensions
            ".rs", ".py", ".js", ".ts", ".go", ".java", ".cpp", ".c", ".h",
            // Locations
            "src/", "lib/", "main", "mod.rs", "lib.rs",
            // Concepts
            "implementación", "implementation", "método", "method",
            "variable", "trait", "interface", "tipo", "type",
        ];

        code_keywords.iter().any(|kw| input.contains(kw))
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
    fn test_ambiguous_queries_return_none() {
        let classifier = TaskClassifier::new();

        // These queries are ambiguous and should go to SimpleChat
        // where the multi-layer system decides what tools to use
        
        // "review" could mean many things - let SimpleChat figure it out
        let result = classifier.classify_fast("review the code in main.rs for security issues");
        assert!(matches!(result, None)); // → SimpleChat

        // "analiza" is context-dependent - needs intelligent routing
        let result = classifier.classify_fast("analiza este repositorio");
        assert!(matches!(result, None)); // → SimpleChat → proactive tools

        // "check" could be review, could be exploration
        let result = classifier.classify_fast("check what main.rs does");
        assert!(matches!(result, None)); // → SimpleChat
    }

    #[test]
    fn test_explicit_code_generation() {
        let classifier = TaskClassifier::new();

        // Only EXPLICIT code generation with type specification
        let result = classifier.classify_fast("genera una función en rust para parsear JSON");
        assert!(matches!(result, Some(TaskType::CodeGeneration { .. })));

        let result = classifier.classify_fast("create a function to handle authentication");
        assert!(matches!(result, Some(TaskType::CodeGeneration { .. })));
    }

    #[test]
    fn test_query_complexity_general() {
        let classifier = TaskClassifier::new();

        // Math
        assert_eq!(
            classifier.classify_complexity("1+1"),
            QueryComplexity::General
        );
        assert_eq!(
            classifier.classify_complexity("what is 2 * 3 + 5?"),
            QueryComplexity::General
        );

        // Casual
        assert_eq!(
            classifier.classify_complexity("hello"),
            QueryComplexity::General
        );

        // Definitions without code
        assert_eq!(
            classifier.classify_complexity("what is recursion?"),
            QueryComplexity::General
        );
    }

    #[test]
    fn test_query_complexity_code_context() {
        let classifier = TaskClassifier::new();

        // ALL code queries → CodeContext (always need full context)

        // Search
        assert_eq!(
            classifier.classify_complexity("find function parse_json in code"),
            QueryComplexity::CodeContext
        );

        // Single file
        assert_eq!(
            classifier.classify_complexity("show me main.rs"),
            QueryComplexity::CodeContext
        );

        // Symbol lookup
        assert_eq!(
            classifier.classify_complexity("where is the Database struct defined?"),
            QueryComplexity::CodeContext
        );

        // Repository analysis
        assert_eq!(
            classifier.classify_complexity("analiza este repositorio"),
            QueryComplexity::CodeContext
        );
    }

    #[test]
    fn test_query_complexity_code_context_complex() {
        let classifier = TaskClassifier::new();

        // Even complex operations still go to CodeContext (same as simple)

        // Refactoring
        assert_eq!(
            classifier.classify_complexity("refactor the authentication module"),
            QueryComplexity::CodeContext
        );

        // Architecture
        assert_eq!(
            classifier.classify_complexity("explain the project architecture"),
            QueryComplexity::CodeContext
        );

        // Multi-file analysis
        assert_eq!(
            classifier.classify_complexity("analyze all files in src/"),
            QueryComplexity::CodeContext
        );
    }
}
