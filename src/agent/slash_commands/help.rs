//! Help Command - Show available commands

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct HelpCommand;

#[async_trait::async_trait]
impl SlashCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    
    fn description(&self) -> &str {
        "Show all available slash commands"
    }
    
    fn usage(&self) -> &str {
        "/help [command] - Show help for all or specific command"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }
    
    async fn execute(&self, args: &str, _ctx: &CommandContext) -> Result<CommandResult> {
        if !args.is_empty() {
            // Show help for specific command
            return Ok(CommandResult::success(format!(
                "For detailed help on a specific command, use: /{} with no args or check usage.",
                args
            )));
        }
        
        // Show all commands grouped by category
        let mut output = String::from("# Available Slash Commands\n\n");
        
        // Code commands
        output.push_str("## 📝 Code\n");
        output.push_str("- `/code-review [path]` - Automated code review\n");
        output.push_str("- `/analyze <path>` - Deep code analysis\n");
        output.push_str("- `/refactor <op> <path>` - Refactoring operations\n");
        output.push_str("- `/format <path>` - Format code\n");
        output.push_str("- `/deps [action]` - Manage dependencies\n\n");
        
        // Testing
        output.push_str("## 🧪 Testing\n");
        output.push_str("- `/test [pattern]` - Run tests\n\n");
        
        // Git
        output.push_str("## 🔀 Git\n");
        output.push_str("- `/commit [message]` - Commit changes\n");
        output.push_str("- `/commit-push-pr` - Full git workflow\n\n");
        
        // Context
        output.push_str("## 🔍 Context & Search\n");
        output.push_str("- `/search <query>` - Semantic code search\n");
        output.push_str("- `/context` - Show project info\n");
        output.push_str("- `/docs [path]` - Generate documentation\n\n");
        
        // System
        output.push_str("## ⚙️  System\n");
        output.push_str("- `/shell <cmd>` - Execute shell command\n");
        output.push_str("- `/plan <task>` - Generate execution plan\n");
        output.push_str("- `/mode <ask|build|plan>` - Change mode\n");
        output.push_str("- `/reindex` - Rebuild code index\n");
        output.push_str("- `/help [cmd]` - Show this help\n\n");
        
        output.push_str("---\n💡 Tip: Use Tab for autocompletion");
        
        Ok(CommandResult::success(output))
    }
}
