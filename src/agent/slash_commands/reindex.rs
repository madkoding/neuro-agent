//! Reindex Command - Rebuild RAPTOR index

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct ReindexCommand;

#[async_trait::async_trait]
impl SlashCommand for ReindexCommand {
    fn name(&self) -> &str {
        "reindex"
    }
    
    fn description(&self) -> &str {
        "Rebuild RAPTOR semantic index for better context retrieval"
    }
    
    fn usage(&self) -> &str {
        "/reindex - Rebuild the code index"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }
    
    async fn execute(&self, _args: &str, _ctx: &CommandContext) -> Result<CommandResult> {
        // Note: Actual reindexing will be handled by RouterOrchestrator
        // This command just signals the intent
        Ok(CommandResult::success(
            "🔄 Reindexing initiated...\n\nThis will rebuild the RAPTOR index for better semantic search."
        ).with_metadata("action", "reindex"))
    }
}
