//! Test Command - Run tests

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::TestArgs;

pub struct TestCommand;

#[async_trait::async_trait]
impl SlashCommand for TestCommand {
    fn name(&self) -> &str {
        "test"
    }
    
    fn description(&self) -> &str {
        "Run project tests with automatic framework detection"
    }
    
    fn usage(&self) -> &str {
        "/test [pattern] - Run tests matching pattern (optional)"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Testing
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let filter = if args.is_empty() {
            None
        } else {
            Some(args.trim().to_string())
        };
        
        let test_args = TestArgs {
            path: ctx.working_dir.clone(),
            filter,
            framework: None,
            verbose: Some(false),
            coverage: Some(false),
            watch: Some(false),
            parallel: Some(false),
        };
        
        match ctx.tools.test_runner.run(test_args).await {
            Ok(output) => {
                let message = format!(
                    "🧪 Test Results:\n  Passed: {}\n  Failed: {}\n  Skipped: {}",
                    output.summary.passed, output.summary.failed, output.summary.skipped
                );
                
                Ok(CommandResult::success(message).with_metadata("pattern", args))
            }
            Err(e) => Ok(CommandResult::error(format!("Tests failed: {}", e))),
        }
    }
}
