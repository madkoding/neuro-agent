//! Módulo de Herramientas - Sistema extensible de herramientas MCP
//!
//! Este módulo provee más de 20 herramientas especializadas para el agente,
//! compatibles con el protocolo MCP (Model Context Protocol).
//!
//! # Categorías de Herramientas
//!
//! ## Análisis de Código
//! - [`analyzer`] - Análisis de complejidad y métricas
//! - [`linter`] - Ejecución de linters (Rust, Python, etc.)
//! - [`dependencies`] - Análisis de dependencias
//!
//! ## Modificación de Código
//! - [`refactor`] - Refactorización automatizada
//! - [`formatter`] - Formateo de código
//!
//! ## Búsqueda
//! - [`search`] - Búsqueda de texto
//! - [`semantic_search`] - Búsqueda semántica con embeddings
//! - [`raptor_tool`] - Búsqueda jerárquica con RAPTOR
//!
//! ## Control de Versiones
//! - [`git`] - Operaciones git (status, diff, blame, etc.)
//!
//! ## Sistema
//! - [`filesystem`] - Operaciones de archivos
//! - [`shell`] - Ejecución de comandos shell
//! - [`environment`] - Variables de entorno
//!
//! # Ejemplo de Uso
//!
//! ```rust,no_run
//! use neuro::tools::registry::ToolRegistry;
//! use rig::tool::Tool;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let registry = ToolRegistry::new();
//! let tools = registry.get_enabled_tools();
//! 
//! for tool in tools {
//!     println!("Tool: {}", tool.name());
//! }
//! # Ok(())
//! # }
//! ```
//! - Git operations
//! - Shell execution
//! - Project context management
//! - Test running
//! - Documentation generation
//! - HTTP client
//! - And more...

mod command;
mod filesystem;
mod linter;
pub mod registry;

// New comprehensive tools
mod analyzer;
mod calculator;
mod context;
mod context_cache;
mod dependencies;
mod documentation;
mod environment;
mod formatter;
mod git;
mod http_client;
pub mod incremental_indexer;
mod indexer;
pub mod planner;
mod raptor_tool;
mod refactor;
mod search;
// mod semantic_search; // Deprecated: Use Raptor instead
mod shell;
mod snippets;
mod test_runner;

// Re-export existing tools
pub use calculator::CalculatorTool;
pub use command::{CommandOutput, ShellExecuteArgs, ShellExecuteTool};
pub use filesystem::{
    DirEntry, FileReadArgs, FileReadOutput, FileReadTool, FileWriteArgs, FileWriteOutput,
    FileWriteTool, ListDirectoryArgs, ListDirectoryOutput, ListDirectoryTool,
};
pub use linter::{LinterArgs, LinterDiagnostic, LinterMode, LinterOutput, LinterTool};
pub use registry::ToolRegistry;

// Re-export new tools
pub use analyzer::{
    AnalyzeFileArgs, AnalyzeSymbolArgs, AnalyzerError, CodeAnalysis, CodeAnalyzerTool, CodeIssue, 
    CodeMetrics, CodeSymbol, ImportInfo, SymbolType,
};
pub use context::{
    ContextError, ContextSummary, DirectoryStructure, GitInfo, ImportantFile, PrimaryLanguage,
    ProjectContext, ProjectContextTool, ProjectType as ContextProjectType,
};
pub use context_cache::{CacheError, CachedProjectContext, ContextCacheTool, ProjectMetrics};
pub use dependencies::{
    AnalyzeDepsArgs, Dependency, DependencyAnalysis, DependencyAnalyzerTool, DependencySource, DepsError,
    OutdatedDependency, ProjectType as DepsProjectType, SecurityIssue,
};
pub use documentation::{
    ClassDoc, DocError, DocFormat, DocGenArgs, DocOutput, DocumentationTool, FunctionDoc,
    ModuleDoc, ParamDoc, ProjectInfo,
};
pub use environment::{
    DiskUsage, EnvironmentInfo, EnvironmentTool, RuntimeInfo, ShellInfo, SystemInfo,
};
pub use formatter::{
    FormatArgs, FormatConfig, FormatError, FormatLanguage, FormatOutput, FormatResult,
    FormatterTool, QuoteStyle,
};
pub use git::{
    BlameLine, BranchInfo, CommitInfo, DiffOutput, FileDiff, GitAddArgs, GitCommitArgs,
    GitError, GitStatus, GitStatusArgs, GitDiffArgs, GitTool,
};
pub use http_client::{
    ApiClient, DownloadResult, HttpClientTool, HttpError, HttpMethod, HttpRequestArgs, HttpResponse,
};
pub use incremental_indexer::{IncrementalIndexer, UpdateReport};
pub use indexer::{
    FileIndexerTool, FileInfo as IndexedFileInfo, IndexerError, LanguageStats, ProjectIndex,
    ProjectSummary,
};
pub use planner::{PlanStatus, Task, TaskEffort, TaskPlan, TaskPlannerTool, TaskStatus, TaskType};
pub use raptor_tool::{BuildTreeArgs, QueryTreeArgs, RaptorTool, RaptorToolCalls};
pub use refactor::{
    ExtractType, RefactorArgs, RefactorChange, RefactorError, RefactorOperation, RefactorResult,
    RefactorScope, RefactorTool,
};
pub use search::{
    ReplaceOutput, SearchArgs, SearchError, SearchInFilesTool, SearchOutput, SearchResult,
};
// pub use semantic_search::{ // Deprecated: Use Raptor instead
//     SearchResultFormatted, SemanticSearchArgs, SemanticSearchError, SemanticSearchOutput,
//     SemanticSearchTool,
// };
pub use shell::{OutputLine, ShellArgs, ShellError, ShellExecutorTool, ShellResult};
pub use snippets::{CodeSnippet, Placeholder, SnippetCollection, SnippetError, SnippetTool};
pub use test_runner::{
    TestArgs, TestCase, TestError, TestFramework, TestOutput, TestRunnerTool, TestStatus,
    TestSummary,
};

/// All available tool names
pub const AVAILABLE_TOOLS: &[&str] = &[
    // File operations
    "read_file",
    "write_file",
    "list_directory",
    "search_files",
    "file_indexer",
    // Code operations
    "analyze_code",
    "format_code",
    "refactor_code",
    "lint_code",
    // Project operations
    "project_context",
    "analyze_dependencies",
    "generate_documentation",
    "run_tests",
    // Git operations
    "git_status",
    "git_diff",
    "git_log",
    "git_commit",
    "git_blame",
    // Shell operations
    "execute_shell",
    "environment_info",
    // Planning
    "task_planner",
    // HTTP
    "http_request",
    // Snippets
    "snippets",
    // RAPTOR - Context Management
    "build_raptor_tree",
    "query_raptor_tree",
    "raptor_stats",
    "clear_raptor",
];

/// Tool category
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCategory {
    FileSystem,
    CodeAnalysis,
    ProjectManagement,
    Git,
    Shell,
    Network,
    Planning,
    ContextManagement, // RAPTOR
    Utilities,
}

/// Get tool category
pub fn get_tool_category(tool_name: &str) -> ToolCategory {
    match tool_name {
        "read_file" | "write_file" | "list_directory" | "search_files" | "file_indexer" => {
            ToolCategory::FileSystem
        }
        "analyze_code" | "format_code" | "refactor_code" | "lint_code" => {
            ToolCategory::CodeAnalysis
        }
        "project_context" | "analyze_dependencies" | "generate_documentation" | "run_tests" => {
            ToolCategory::ProjectManagement
        }
        "git_status" | "git_diff" | "git_log" | "git_commit" | "git_blame" => ToolCategory::Git,
        "execute_shell" | "environment_info" => ToolCategory::Shell,
        "http_request" => ToolCategory::Network,
        "task_planner" => ToolCategory::Planning,
        "build_raptor_tree" | "query_raptor_tree" | "raptor_stats" | "clear_raptor" => {
            ToolCategory::ContextManagement
        }
        _ => ToolCategory::Utilities,
    }
}
