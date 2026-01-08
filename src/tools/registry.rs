//! Tool registry for managing and sharing tools between agents

use super::{
    CalculatorTool,
    CodeAnalyzerTool,
    DependencyAnalyzerTool,
    DocumentationTool,
    EnvironmentTool,
    // New tools
    FileIndexerTool,
    FileReadTool,
    FileWriteTool,
    FormatterTool,
    GitTool,
    HttpClientTool,
    LinterTool,
    ListDirectoryTool,
    ProjectContextTool,
    RefactorTool,
    SearchInFilesTool,
    ShellExecuteTool,
    ShellExecutorTool,
    SnippetTool,
    TaskPlannerTool,
    TestRunnerTool,
};
use std::sync::Arc;

/// Registry that holds all available tools
/// This can be shared between multiple agents
#[derive(Clone)]
pub struct ToolRegistry {
    // Original tools
    pub file_read: Arc<FileReadTool>,
    pub file_write: Arc<FileWriteTool>,
    pub list_directory: Arc<ListDirectoryTool>,
    pub shell_execute: Arc<ShellExecuteTool>,
    pub linter: Arc<LinterTool>,
    pub calculator: Arc<CalculatorTool>,

    // New comprehensive tools
    pub file_indexer: Arc<FileIndexerTool>,
    pub task_planner: Arc<TaskPlannerTool>,
    pub search_files: Arc<SearchInFilesTool>,
    pub git: Arc<GitTool>,
    pub code_analyzer: Arc<CodeAnalyzerTool>,
    pub dependency_analyzer: Arc<DependencyAnalyzerTool>,
    pub http_client: Arc<HttpClientTool>,
    pub shell_executor: Arc<ShellExecutorTool>,
    pub test_runner: Arc<TestRunnerTool>,
    pub documentation: Arc<DocumentationTool>,
    pub formatter: Arc<FormatterTool>,
    pub refactor: Arc<RefactorTool>,
    pub snippets: Arc<SnippetTool>,
    pub project_context: Arc<tokio::sync::Mutex<ProjectContextTool>>,
    pub environment: Arc<EnvironmentTool>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new tool registry with all default tools
    pub fn new() -> Self {
        Self {
            // Original tools
            file_read: Arc::new(FileReadTool),
            file_write: Arc::new(FileWriteTool),
            list_directory: Arc::new(ListDirectoryTool),
            shell_execute: Arc::new(ShellExecuteTool::new()),
            linter: Arc::new(LinterTool),
            calculator: Arc::new(CalculatorTool),

            // New tools
            file_indexer: Arc::new(FileIndexerTool::new()),
            task_planner: Arc::new(TaskPlannerTool::new()),
            search_files: Arc::new(SearchInFilesTool::new()),
            git: Arc::new(GitTool::new()),
            code_analyzer: Arc::new(CodeAnalyzerTool::new()),
            dependency_analyzer: Arc::new(DependencyAnalyzerTool),
            http_client: Arc::new(HttpClientTool::new()),
            shell_executor: Arc::new(ShellExecutorTool::new()),
            test_runner: Arc::new(TestRunnerTool::new()),
            documentation: Arc::new(DocumentationTool::new()),
            formatter: Arc::new(FormatterTool::new()),
            refactor: Arc::new(RefactorTool::new()),
            snippets: Arc::new(SnippetTool::with_defaults()),
            project_context: Arc::new(tokio::sync::Mutex::new(ProjectContextTool::new())),
            environment: Arc::new(EnvironmentTool::new()),
        }
    }

    /// Create a new tool registry with a custom shell executor
    pub fn with_shell_executor(shell_execute: ShellExecuteTool) -> Self {
        let mut registry = Self::new();
        registry.shell_execute = Arc::new(shell_execute);
        registry
    }

    /// Get a list of all tool names
    pub fn tool_names(&self) -> Vec<&'static str> {
        vec![
            // Original tools
            FileReadTool::NAME,
            FileWriteTool::NAME,
            ListDirectoryTool::NAME,
            ShellExecuteTool::NAME,
            LinterTool::NAME,
            CalculatorTool::NAME,
            // New tools
            FileIndexerTool::NAME,
            TaskPlannerTool::NAME,
            SearchInFilesTool::NAME,
            GitTool::NAME,
            CodeAnalyzerTool::NAME,
            DependencyAnalyzerTool::NAME,
            HttpClientTool::NAME,
            ShellExecutorTool::NAME,
            TestRunnerTool::NAME,
            DocumentationTool::NAME,
            FormatterTool::NAME,
            RefactorTool::NAME,
            SnippetTool::NAME,
            ProjectContextTool::NAME,
            EnvironmentTool::NAME,
        ]
    }

    /// Get tool descriptions for the system prompt
    pub fn tool_descriptions(&self) -> String {
        format!(
            r#"Available tools (20+):

## File System Operations
1. {} - Read file contents, optionally specifying line ranges
2. {} - Write content to files, can create directories
3. {} - List directory contents, optionally recursive
4. {} - Index project files for context
5. {} - Search in files using patterns (grep-like)

## Code Analysis & Quality
6. {} - Analyze code structure, metrics, and issues
7. {} - Run Rust linters (cargo check/clippy)
8. {} - Format code in multiple languages
9. {} - Refactor code (rename, extract, inline)

## Project Management
10. {} - Analyze project dependencies
11. {} - Generate documentation
12. {} - Run tests across frameworks
13. {} - Get project context and structure

## Git Operations
14. {} - Git operations (status, diff, log, commit, blame)

## Shell & Environment
15. {} - Execute shell commands (security-scanned)
16. {} - Advanced shell execution with streaming
17. {} - Get environment and system info

## Planning & Utilities
18. {} - Evaluate mathematical expressions
19. {} - Create and manage task plans
20. {} - Make HTTP requests
21. {} - Code snippets and templates"#,
            FileReadTool::NAME,
            FileWriteTool::NAME,
            ListDirectoryTool::NAME,
            FileIndexerTool::NAME,
            SearchInFilesTool::NAME,
            CodeAnalyzerTool::NAME,
            LinterTool::NAME,
            FormatterTool::NAME,
            RefactorTool::NAME,
            DependencyAnalyzerTool::NAME,
            DocumentationTool::NAME,
            TestRunnerTool::NAME,
            ProjectContextTool::NAME,
            GitTool::NAME,
            ShellExecuteTool::NAME,
            ShellExecutorTool::NAME,
            EnvironmentTool::NAME,
            CalculatorTool::NAME,
            TaskPlannerTool::NAME,
            HttpClientTool::NAME,
            SnippetTool::NAME,
        )
    }

    /// Get tools by category
    pub fn tools_by_category(&self) -> std::collections::HashMap<String, Vec<&'static str>> {
        let mut categories = std::collections::HashMap::new();

        categories.insert(
            "file_system".to_string(),
            vec![
                FileReadTool::NAME,
                FileWriteTool::NAME,
                ListDirectoryTool::NAME,
                FileIndexerTool::NAME,
                SearchInFilesTool::NAME,
            ],
        );

        categories.insert(
            "code_analysis".to_string(),
            vec![
                CodeAnalyzerTool::NAME,
                LinterTool::NAME,
                FormatterTool::NAME,
                RefactorTool::NAME,
            ],
        );

        categories.insert(
            "project".to_string(),
            vec![
                DependencyAnalyzerTool::NAME,
                DocumentationTool::NAME,
                TestRunnerTool::NAME,
                ProjectContextTool::NAME,
            ],
        );

        categories.insert("git".to_string(), vec![GitTool::NAME]);

        categories.insert(
            "shell".to_string(),
            vec![
                ShellExecuteTool::NAME,
                ShellExecutorTool::NAME,
                EnvironmentTool::NAME,
            ],
        );

        categories.insert(
            "utilities".to_string(),
            vec![
                CalculatorTool::NAME,
                TaskPlannerTool::NAME,
                HttpClientTool::NAME,
                SnippetTool::NAME,
            ],
        );

        categories
    }

    /// Check if a tool is enabled
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        self.tool_names().contains(&tool_name)
    }

    /// Get tool count
    pub fn tool_count(&self) -> usize {
        self.tool_names().len()
    }

    /// Convert all registered tools to Ollama function calling format
    ///
    /// This method generates the tools array for Ollama's native function
    /// calling API (0.3+). Each tool includes its name, description, and
    /// JSON Schema parameters.
    ///
    /// Note: Currently only includes tools that implement the full rig::tool::Tool trait.
    /// Other tools will need manual schema definitions or trait implementation.
    pub async fn get_ollama_tools_schema(&self) -> Vec<crate::agent::provider::OllamaTool> {
        use crate::agent::provider::{OllamaFunction, OllamaTool};
        use rig::tool::Tool;

        let mut tools = Vec::new();

        // Helper macro to add a tool definition
        macro_rules! add_tool {
            ($tool:expr) => {{
                let def = $tool.definition(String::new()).await;
                tools.push(OllamaTool {
                    tool_type: "function".to_string(),
                    function: OllamaFunction {
                        name: def.name,
                        description: def.description,
                        parameters: def.parameters,
                    },
                });
            }};
        }

        // File system tools (implement Tool trait)
        add_tool!(&*self.file_read);
        add_tool!(&*self.file_write);
        add_tool!(&*self.list_directory);

        // Code analysis tools (implement Tool trait)
        add_tool!(&*self.linter);

        // Shell tool (implement Tool trait)
        add_tool!(&*self.shell_execute);

        // Utilities (implement Tool trait)
        add_tool!(&*self.calculator);

        // MANUAL SCHEMAS: Tools that don't yet implement Tool trait
        
        // NOTE: RAPTOR tools (build_raptor_tree, query_raptor_tree) and semantic_search
        // are not yet fully integrated. They require PlanningOrchestrator context.
        // For now, focus on the working tools above which provide excellent coverage
        // for most code assistance tasks.
        
        // TODO: Add manual schemas for remaining tools:
        // - search_files (text search in files)
        // - code_analyzer, formatter, refactor
        // - dependency_analyzer, documentation, test_runner, git
        // - http_client, snippets, project_context

        tools
    }
}

// Implement the NAME constants using the Tool trait
impl FileReadTool {
    pub const NAME: &'static str = "read_file";
}

impl FileWriteTool {
    pub const NAME: &'static str = "write_file";
}

impl ListDirectoryTool {
    pub const NAME: &'static str = "list_directory";
}

impl ShellExecuteTool {
    pub const NAME: &'static str = "execute_shell";
}

impl LinterTool {
    pub const NAME: &'static str = "run_linter";
}

impl CalculatorTool {
    pub const NAME: &'static str = "calculator";
}
