//! Refactor Command - Code refactoring operations

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct RefactorCommand;

#[async_trait::async_trait]
impl SlashCommand for RefactorCommand {
    fn name(&self) -> &str {
        "refactor"
    }
    
    fn description(&self) -> &str {
        "Perform code refactoring operations"
    }
    
    fn usage(&self) -> &str {
        "/refactor <operation> <path> - Operations: extract, rename, inline, simplify"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Code
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let parts: Vec<&str> = args.split_whitespace().collect();
        
        if parts.len() < 2 {
            return Ok(CommandResult::error(
                "Usage: /refactor <operation> <path> [additional_args]\nOperations: rename, extract, inline"
            ));
        }
        
        let operation = parts[0];
        let path = parts[1];
        
        // Use RefactorTool to perform the operation
        use crate::tools::{RefactorArgs, RefactorOperation, RefactorScope, ExtractType};
        
        let refactor_op = match operation {
            "extract" => {
                if parts.len() < 3 {
                    return Ok(CommandResult::error("Usage: /refactor extract <path> <function_name>"));
                }
                // For extract, we need code selection - use a placeholder for now
                RefactorOperation::Extract {
                    code: "// Selected code will be extracted".to_string(),
                    name: parts[2].to_string(),
                    extract_type: ExtractType::Function,
                }
            }
            "rename" => {
                if parts.len() < 4 {
                    return Ok(CommandResult::error("Usage: /refactor rename <path> <old_name> <new_name>"));
                }
                RefactorOperation::Rename {
                    old_name: parts[2].to_string(),
                    new_name: parts[3].to_string(),
                    scope: RefactorScope::File(path.to_string()),
                }
            }
            "inline" => {
                if parts.len() < 3 {
                    return Ok(CommandResult::error("Usage: /refactor inline <path> <function_name>"));
                }
                RefactorOperation::Inline {
                    name: parts[2].to_string(),
                }
            }
            _ => {
                return Ok(CommandResult::error(
                    "Unknown operation. Available: extract, rename, inline"
                ));
            }
        };
        
        let args = RefactorArgs {
            operation: refactor_op,
            path: path.to_string(),
            dry_run: Some(true), // Default to dry run for safety
        };
        
        match ctx.tools.refactor.refactor(args).await {
            Ok(result) => {
                let mut output = format!("# Refactoring Complete: {}\n\n", path);
                output.push_str(&format!("Operation: {}\n", operation));
                output.push_str(&format!("Files modified: {}\n", result.files_modified));
                output.push_str(&format!("Total changes: {}\n\n", result.total_changes));
                
                if !result.changes.is_empty() {
                    output.push_str("## Changes (dry run):\n");
                    for change in result.changes.iter().take(10) {
                        output.push_str(&format!("• Line {}: {} → {}\n", 
                            change.line, 
                            change.old_text.chars().take(30).collect::<String>(),
                            change.new_text.chars().take(30).collect::<String>()
                        ));
                    }
                    if result.changes.len() > 10 {
                        output.push_str(&format!("...and {} more changes\n", result.changes.len() - 10));
                    }
                }
                
                if !result.errors.is_empty() {
                    output.push_str("\n## Errors:\n");
                    for error in &result.errors {
                        output.push_str(&format!("• {}\n", error));
                    }
                }
                
                output.push_str("\n💡 This was a dry run. Remove dry_run flag to apply changes.\n");
                
                Ok(CommandResult::success(output)
                    .with_metadata("operation", operation)
                    .with_metadata("path", path))
            }
            Err(e) => {
                Ok(CommandResult::error(format!("Refactoring failed: {}", e)))
            }
        }
    }
}
