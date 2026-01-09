//! Code Review Command - Automated code analysis and review

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use rig::tool::Tool;

pub struct CodeReviewCommand;

#[async_trait::async_trait]
impl SlashCommand for CodeReviewCommand {
    fn name(&self) -> &str {
        "code-review"
    }
    
    fn description(&self) -> &str {
        "Perform automated code review on files or directories"
    }
    
    fn usage(&self) -> &str {
        "/code-review [path] - Review code at path (default: current directory)"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Code
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        use crate::tools::{LinterArgs, LinterMode, AnalyzeFileArgs};
        
        let path = if args.is_empty() {
            "."
        } else {
            args.trim()
        };
        
        let mut review_results = Vec::new();
        
        // 1. Run linter if it's a directory with Cargo.toml
        let linter_args = LinterArgs {
            project_path: path.to_string(),
            mode: LinterMode::Clippy,
            extra_args: vec![],
            auto_fix: false,
        };
        
        match ctx.tools.linter.call(linter_args).await {
            Ok(result) => {
                review_results.push(format!(
                    "📋 **Linter Results:**\nSuccess: {}\nErrors: {}\nWarnings: {}",
                    result.success, result.error_count, result.warning_count
                ));
            }
            Err(e) => {
                review_results.push(format!("📋 **Linter:** {}", e));
            }
        }
        
        // 2. Analyze code if it's a file
        if std::path::Path::new(path).is_file() {
            let analyzer_args = AnalyzeFileArgs {
                path: path.to_string(),
            };
            
            match ctx.tools.code_analyzer.analyze_file(analyzer_args).await {
                Ok(analysis) => {
                    review_results.push(format!(
                        "\n🔍 **Code Analysis:**\nComplexity: {}\nLines: {}\nFunctions: {}",
                        analysis.metrics.complexity,
                        analysis.metrics.total_lines,
                        analysis.symbols.len()
                    ));
                }
                Err(e) => {
                    review_results.push(format!("🔍 **Analysis:** {}", e));
                }
            }
        }
        
        let output = format!("# Code Review Report: {}\n\n{}\n\n---\n✅ Review complete", 
                           path, review_results.join("\n"));
        
        Ok(CommandResult::success(output).with_metadata("path", path))
    }
}
