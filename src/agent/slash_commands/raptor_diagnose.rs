use super::*;
use crate::raptor::persistence::GLOBAL_STORE;

pub struct RaptorDiagnoseCommand;

impl RaptorDiagnoseCommand {
    pub const NAME: &'static str = "raptor-diagnose";
}

#[async_trait::async_trait]
impl SlashCommand for RaptorDiagnoseCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn description(&self) -> &str {
        "Diagnose RAPTOR index state for the current project"
    }

    fn usage(&self) -> &str {
        "/raptor-diagnose"
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }

    async fn execute(&self, _args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        // Gather basic RAPTOR stats
        let working_dir = &ctx.working_dir;

        let (chunk_count, indexed_files, embeddings, indexing_complete) = {
            let store = GLOBAL_STORE.lock().unwrap();
            (
                store.chunk_map.len(),
                store.indexed_files.len(),
                !store.chunk_embeddings.is_empty(),
                store.indexing_complete,
            )
        };

        let mut output = format!("📊 RAPTOR Diagnose for: {}\n", working_dir);
        output.push_str(&format!("• Chunks stored: {}\n", chunk_count));
        output.push_str(&format!("• Indexed files: {}\n", indexed_files));
        output.push_str(&format!("• Embeddings generated: {}\n", if embeddings { "yes" } else { "no" }));
        output.push_str(&format!("• Indexing complete: {}\n", if indexing_complete { "yes" } else { "no" }));

        if chunk_count == 0 {
            output.push_str("\n⚠️ No chunks detected. Try: /reindex or !reindex to build the index.\n");
        } else if !embeddings {
            output.push_str("\nℹ️ Quick index exists but embeddings are not yet generated. Wait for RAPTOR background indexing to finish.\n");
        } else if indexing_complete {
            output.push_str("\n✅ Index appears complete.\n");
        }

        Ok(CommandResult::success(output))
    }
}
