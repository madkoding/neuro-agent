//! Format Command - Code formatting

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::FormatArgs;

pub struct FormatCommand;

#[async_trait::async_trait]
impl SlashCommand for FormatCommand {
    fn name(&self) -> &str {
        "format"
    }
    
    fn description(&self) -> &str {
        "Format code using language-specific formatters"
    }
    
    fn usage(&self) -> &str {
        "/format <path> - Format files at path"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Code
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let path = if args.is_empty() {
            ".".to_string()
        } else {
            args.trim().to_string()
        };
        
        let format_args = FormatArgs {
            path: path.clone(),
            language: None,
            config: None,
            check_only: Some(false),
            recursive: Some(true),
        };
        
        match ctx.tools.formatter.format(format_args).await {
            Ok(output) => {
                let formatted_count = output.results.iter().filter(|r| r.formatted).count();
                let changed_count = output.results.iter().filter(|r| r.changed).count();
                
                let message = format!(
                    "✨ Formatting complete:\n  Files processed: {}\n  Files changed: {}",
                    formatted_count, changed_count
                );
                
                Ok(CommandResult::success(message).with_metadata("path", &path))
            }
            Err(e) => Ok(CommandResult::error(format!("Formatting failed: {}", e))),
        }
    }
}
