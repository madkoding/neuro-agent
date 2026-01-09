//! Dependencies Command - Manage project dependencies

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::AnalyzeDepsArgs;

pub struct DependenciesCommand;

#[async_trait::async_trait]
impl SlashCommand for DependenciesCommand {
    fn name(&self) -> &str {
        "deps"
    }
    
    fn description(&self) -> &str {
        "Analyze and manage project dependencies"
    }
    
    fn usage(&self) -> &str {
        "/deps [path] - Analyze dependencies in project"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Code
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let path = if args.is_empty() {
            ctx.working_dir.clone()
        } else {
            args.trim().to_string()
        };
        
        let deps_args = AnalyzeDepsArgs {
            path: path.clone(),
            check_outdated: Some(false),
            check_security: Some(false),
        };
        
        match ctx.tools.dependency_analyzer.analyze(deps_args).await {
            Ok(analysis) => {
                let message = format!(
                    "📦 Dependencies Analysis:\n  Total: {}\n  Direct: {}\n  Dev: {}\n  Outdated: {}\n  Security issues: {}",
                    analysis.total_count,
                    analysis.direct_count,
                    analysis.dev_dependencies.len(),
                    analysis.outdated.len(),
                    analysis.security_issues.len()
                );
                
                Ok(CommandResult::success(message).with_metadata("path", &path))
            }
            Err(e) => Ok(CommandResult::error(format!("Dependency analysis failed: {}", e))),
        }
    }
}
