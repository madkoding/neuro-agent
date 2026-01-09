//! Documentation Command - Generate documentation

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::{DocGenArgs, DocFormat};

pub struct DocsCommand;

#[async_trait::async_trait]
impl SlashCommand for DocsCommand {
    fn name(&self) -> &str {
        "docs"
    }
    
    fn description(&self) -> &str {
        "Generate or view project documentation"
    }
    
    fn usage(&self) -> &str {
        "/docs [path] - Generate docs for path (default: current directory)"
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
        
        let docs_args = DocGenArgs {
            path: path.clone(),
            output: None,
            format: Some(DocFormat::Markdown),
            include_private: Some(false),
            include_tests: Some(false),
        };
        
        match ctx.tools.documentation.generate(docs_args).await {
            Ok(output) => {
                let message = format!(
                    "📚 Documentation generated:\n  Modules: {}\n  Functions: {}\n  Classes: {}",
                    output.modules.len(),
                    output.modules.iter().map(|m| m.functions.len()).sum::<usize>(),
                    output.modules.iter().map(|m| m.classes.len()).sum::<usize>()
                );
                
                Ok(CommandResult::success(message).with_metadata("path", &path))
            }
            Err(e) => Ok(CommandResult::error(format!("Documentation generation failed: {}", e))),
        }
    }
}
