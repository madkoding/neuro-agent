//! Tools module - MCP-compatible tools for the AI agent
//!
//! This module provides 20+ tools for the neuro agent including:
//! - File system operations
//! - Code analysis and refactoring
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
mod registry;

// New comprehensive tools
mod indexer;
pub mod planner;
mod search;
mod semantic_search;
mod git;
mod analyzer;
mod dependencies;
mod http_client;
mod shell;
mod test_runner;
mod documentation;
mod formatter;
mod refactor;
mod snippets;
mod context;
mod context_cache;
mod environment;
pub mod incremental_indexer;
mod raptor_tool;

// Re-export existing tools
pub use command::{CommandOutput, ShellExecuteTool, ShellExecuteArgs};
pub use filesystem::{
    FileReadTool, FileReadArgs, FileReadOutput,
    FileWriteTool, FileWriteArgs, FileWriteOutput,
    ListDirectoryTool, ListDirectoryArgs, ListDirectoryOutput, DirEntry,
};
pub use linter::{LinterOutput, LinterTool, LinterArgs, LinterDiagnostic, LinterMode};
pub use registry::ToolRegistry;

// Re-export new tools
pub use indexer::{
    FileIndexerTool, ProjectIndex, FileInfo as IndexedFileInfo, 
    ProjectSummary, LanguageStats, IndexerError,
};
pub use planner::{
    TaskPlannerTool, TaskPlan, Task, TaskType, TaskStatus, 
    TaskEffort, PlanStatus,
};
pub use search::{
    SearchInFilesTool, SearchArgs, SearchResult, SearchOutput,
    ReplaceOutput, SearchError,
};
pub use semantic_search::{
    SemanticSearchTool, SemanticSearchArgs, SemanticSearchOutput,
    SearchResultFormatted, SemanticSearchError,
};
pub use git::{
    GitTool, GitStatus, CommitInfo, DiffOutput, FileDiff, 
    BranchInfo, BlameLine, GitError,
};
pub use analyzer::{
    CodeAnalyzerTool, CodeAnalysis, CodeMetrics, CodeSymbol, 
    SymbolType, ImportInfo, CodeIssue, AnalyzerError,
};
pub use dependencies::{
    DependencyAnalyzerTool, DependencyAnalysis, Dependency, 
    DependencySource, OutdatedDependency, SecurityIssue, 
    ProjectType as DepsProjectType, DepsError,
};
pub use http_client::{
    HttpClientTool, HttpRequestArgs, HttpResponse, HttpMethod,
    DownloadResult, ApiClient, HttpError,
};
pub use shell::{
    ShellExecutorTool, ShellArgs, ShellResult, OutputLine, ShellError,
};
pub use test_runner::{
    TestRunnerTool, TestArgs, TestOutput, TestCase, TestSummary,
    TestFramework, TestStatus, TestError,
};
pub use documentation::{
    DocumentationTool, DocGenArgs, DocOutput, ModuleDoc, FunctionDoc,
    ClassDoc, ParamDoc, ProjectInfo, DocFormat, DocError,
};
pub use formatter::{
    FormatterTool, FormatArgs, FormatResult, FormatOutput, 
    FormatConfig, FormatLanguage, QuoteStyle, FormatError,
};
pub use refactor::{
    RefactorTool, RefactorArgs, RefactorOperation, RefactorResult,
    RefactorChange, RefactorScope, ExtractType, RefactorError,
};
pub use snippets::{
    SnippetTool, CodeSnippet, Placeholder, SnippetCollection, 
    SnippetError,
};
pub use context::{
    ProjectContextTool, ProjectContext, PrimaryLanguage,
    ProjectType as ContextProjectType, ImportantFile,
    DirectoryStructure, GitInfo, ContextSummary, ContextError,
};
pub use context_cache::{
    ContextCacheTool, CachedProjectContext, ProjectMetrics, CacheError,
};
pub use environment::{
    EnvironmentTool, EnvironmentInfo, SystemInfo, RuntimeInfo,
    ShellInfo, DiskUsage,
};
pub use incremental_indexer::{IncrementalIndexer, UpdateReport};
pub use raptor_tool::{
    RaptorTool, RaptorToolCalls, BuildTreeArgs, QueryTreeArgs,
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
        "git_status" | "git_diff" | "git_log" | "git_commit" | "git_blame" => {
            ToolCategory::Git
        }
        "execute_shell" | "environment_info" => {
            ToolCategory::Shell
        }
        "http_request" => {
            ToolCategory::Network
        }
        "task_planner" => {
            ToolCategory::Planning
        }
        "build_raptor_tree" | "query_raptor_tree" | "raptor_stats" | "clear_raptor" => {
            ToolCategory::ContextManagement
        }
        _ => ToolCategory::Utilities,
    }
}
