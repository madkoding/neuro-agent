//! Context Command - Project context information

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct ContextCommand;

#[async_trait::async_trait]
impl SlashCommand for ContextCommand {
    fn name(&self) -> &str {
        "context"
    }
    
    fn description(&self) -> &str {
        "Show project structure, languages, and configuration"
    }
    
    fn usage(&self) -> &str {
        "/context - Display project context information"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Context
    }
    
    async fn execute(&self, _args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let mut context_tool = ctx.tools.project_context.lock().await;
        match context_tool.analyze(&ctx.working_dir).await {
            Ok(result) => {
                let output = format!(
                    "📊 Project Context\n\n\
                    **Name:** {}\n\
                    **Type:** {:?}\n\
                    **Language:** {:?}\n\
                    **Files:** {}\n\
                    **Dependencies:** {}",
                    result.name,
                    result.project_type,
                    result.language,
                    result.file_count,
                    result.dependencies_count
                );
                Ok(CommandResult::success(output))
            }
            Err(e) => Ok(CommandResult::error(format!("Failed to get context: {}", e))),
        }
    }
}
