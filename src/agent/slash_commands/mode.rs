//! Mode Command - Change operation mode

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct ModeCommand;

#[async_trait::async_trait]
impl SlashCommand for ModeCommand {
    fn name(&self) -> &str {
        "mode"
    }
    
    fn description(&self) -> &str {
        "Change agent operation mode (ask/build/plan)"
    }
    
    fn usage(&self) -> &str {
        "/mode <ask|build|plan> - Set operation mode"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }
    
    fn validate_args(&self, args: &str) -> Result<()> {
        let mode = args.trim().to_lowercase();
        if !["ask", "build", "plan"].contains(&mode.as_str()) {
            anyhow::bail!("Invalid mode. Use: ask, build, or plan");
        }
        Ok(())
    }
    
    async fn execute(&self, args: &str, _ctx: &CommandContext) -> Result<CommandResult> {
        let mode = args.trim().to_lowercase();
        
        let description = match mode.as_str() {
            "ask" => "🔍 **Ask Mode**: Read-only queries, semantic search, analysis",
            "build" => "🔨 **Build Mode**: Write operations, refactoring, file modifications",
            "plan" => "📋 **Plan Mode**: Generate plans without execution",
            _ => unreachable!(),
        };
        
        Ok(CommandResult::success(format!(
            "Mode changed to: **{}**\n\n{}",
            mode.to_uppercase(),
            description
        )).with_metadata("mode", &mode))
    }
}
