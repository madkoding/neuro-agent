//! Analyze Command - Deep code analysis and explanation

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct AnalyzeCommand;

#[async_trait::async_trait]
impl SlashCommand for AnalyzeCommand {
    fn name(&self) -> &str {
        "analyze"
    }
    
    fn description(&self) -> &str {
        "Analyze and explain code structure, complexity, and patterns"
    }
    
    fn usage(&self) -> &str {
        "/analyze <path> [type] - Types: overview, complexity, functions, dependencies"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Code
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        use crate::tools::AnalyzeFileArgs;
        
        let parts: Vec<&str> = args.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(CommandResult::error("Usage: /analyze <path>"));
        }
        
        let path = parts[0];
        
        let analyzer_args = AnalyzeFileArgs {
            path: path.to_string(),
        };
        
        match ctx.tools.code_analyzer.analyze_file(analyzer_args).await {
            Ok(analysis) => {
                let issues_str = if analysis.issues.is_empty() {
                    "No issues found".to_string()
                } else {
                    format!("{} issues found", analysis.issues.len())
                };
                
                let output = format!(
                    "# Analysis: {}\n\n\
                    **Language:** {:?}\n\
                    **Lines:** {} (code: {}, comments: {})\n\
                    **Complexity:** {}\n\
                    **Functions:** {}\n\
                    **Imports:** {}\n\
                    **Issues:** {}\n\n\
                    ---\n✅ Analysis complete",
                    path,
                    analysis.language,
                    analysis.metrics.total_lines,
                    analysis.metrics.code_lines,
                    analysis.metrics.comment_lines,
                    analysis.metrics.complexity,
                    analysis.symbols.len(),
                    analysis.imports.len(),
                    issues_str
                );
                Ok(CommandResult::success(output).with_metadata("path", path))
            }
            Err(e) => Ok(CommandResult::error(format!("Analysis failed: {}", e))),
        }
    }
}
