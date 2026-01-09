//! Slash Commands System
//!
//! Provides a plugin-like system for executing predefined commands with `/` prefix
//! Inspired by Claude Code's plugin architecture but adapted for neuro-agent

use crate::agent::state::SharedState;
use crate::tools::registry::ToolRegistry;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

// Command modules
mod code_review;
mod analyze;
mod refactor;
mod format;
mod test;
mod docs;
mod commit;
mod dependencies;
mod search;
mod context;
mod plan;
mod shell;
mod reindex;
mod mode;
mod help;

// Re-exports
pub use code_review::CodeReviewCommand;
pub use analyze::AnalyzeCommand;
pub use refactor::RefactorCommand;
pub use format::FormatCommand;
pub use test::TestCommand;
pub use docs::DocsCommand;
pub use commit::{CommitCommand, CommitPushPrCommand};
pub use dependencies::DependenciesCommand;
pub use search::SearchCommand;
pub use context::ContextCommand;
pub use plan::PlanCommand;
pub use shell::ShellCommand;
pub use reindex::ReindexCommand;
pub use mode::ModeCommand;
pub use help::HelpCommand;

/// Context passed to slash commands during execution
#[derive(Clone)]
pub struct CommandContext {
    pub tools: Arc<ToolRegistry>,
    pub state: SharedState,
    pub working_dir: String,
}

/// Trait that all slash commands must implement
#[async_trait::async_trait]
pub trait SlashCommand: Send + Sync {
    /// Command name (without the / prefix)
    fn name(&self) -> &str;
    
    /// Short description for help text
    fn description(&self) -> &str;
    
    /// Detailed usage information
    fn usage(&self) -> &str {
        self.name()
    }
    
    /// Category for grouping in help
    fn category(&self) -> CommandCategory {
        CommandCategory::Other
    }
    
    /// Execute the command with given arguments
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult>;
    
    /// Optional: Validate arguments before execution
    fn validate_args(&self, _args: &str) -> Result<()> {
        Ok(())
    }
}

/// Command execution result
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub output: String,
    pub success: bool,
    pub metadata: HashMap<String, String>,
}

impl CommandResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: true,
            metadata: HashMap::new(),
        }
    }
    
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            output: message.into(),
            success: false,
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Command categories for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    Code,       // Code analysis, review, refactoring
    Testing,    // Test-related commands
    Git,        // Git operations
    Context,    // Context and search
    System,     // System-level operations
    Other,
}

impl CommandCategory {
    pub fn name(&self) -> &str {
        match self {
            Self::Code => "Code",
            Self::Testing => "Testing",
            Self::Git => "Git",
            Self::Context => "Context & Search",
            Self::System => "System",
            Self::Other => "Other",
        }
    }
}

/// Registry that holds all available slash commands
pub struct SlashCommandRegistry {
    commands: HashMap<String, Box<dyn SlashCommand>>,
}

impl SlashCommandRegistry {
    /// Create a new registry with all default commands
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };
        
        // Register all commands
        registry.register(Box::new(CodeReviewCommand));
        registry.register(Box::new(AnalyzeCommand));
        registry.register(Box::new(RefactorCommand));
        registry.register(Box::new(FormatCommand));
        registry.register(Box::new(TestCommand));
        registry.register(Box::new(DocsCommand));
        registry.register(Box::new(CommitCommand));
        registry.register(Box::new(CommitPushPrCommand));
        registry.register(Box::new(DependenciesCommand));
        registry.register(Box::new(SearchCommand));
        registry.register(Box::new(ContextCommand));
        registry.register(Box::new(PlanCommand));
        registry.register(Box::new(ShellCommand));
        registry.register(Box::new(ReindexCommand));
        registry.register(Box::new(ModeCommand));
        registry.register(Box::new(HelpCommand));
        
        registry
    }
    
    /// Register a custom command
    pub fn register(&mut self, command: Box<dyn SlashCommand>) {
        self.commands.insert(command.name().to_string(), command);
    }
    
    /// Get a command by name
    pub fn get(&self, name: &str) -> Option<&Box<dyn SlashCommand>> {
        self.commands.get(name)
    }
    
    /// Check if a string is a slash command
    pub fn is_slash_command(input: &str) -> bool {
        input.trim().starts_with('/')
    }
    
    /// Parse a slash command from input
    /// Returns (command_name, args)
    pub fn parse_command(input: &str) -> Option<(&str, &str)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        
        let without_slash = &trimmed[1..];
        if let Some(space_idx) = without_slash.find(char::is_whitespace) {
            let cmd = &without_slash[..space_idx];
            let args = without_slash[space_idx..].trim();
            Some((cmd, args))
        } else {
            Some((without_slash, ""))
        }
    }
    
    /// Execute a slash command
    pub async fn execute(&self, input: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let (cmd_name, args) = Self::parse_command(input)
            .ok_or_else(|| anyhow::anyhow!("Invalid slash command format"))?;
        
        let command = self.get(cmd_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown command: /{}", cmd_name))?;
        
        command.validate_args(args)?;
        command.execute(args, ctx).await
    }
    
    /// Get all commands grouped by category
    pub fn commands_by_category(&self) -> HashMap<CommandCategory, Vec<&Box<dyn SlashCommand>>> {
        let mut grouped: HashMap<CommandCategory, Vec<&Box<dyn SlashCommand>>> = HashMap::new();
        
        for command in self.commands.values() {
            grouped.entry(command.category())
                .or_insert_with(Vec::new)
                .push(command);
        }
        
        // Sort commands within each category
        for commands in grouped.values_mut() {
            commands.sort_by_key(|c| c.name());
        }
        
        grouped
    }
    
    /// Get all command names for autocomplete
    pub fn command_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys()
            .map(|s| format!("/{}", s))
            .collect();
        names.sort();
        names
    }
}

impl Default for SlashCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}
