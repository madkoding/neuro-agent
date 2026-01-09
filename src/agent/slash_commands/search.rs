//! Search Command - Intelligent code search

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::SearchArgs;

pub struct SearchCommand;

#[async_trait::async_trait]
impl SlashCommand for SearchCommand {
    fn name(&self) -> &str {
        "search"
    }
    
    fn description(&self) -> &str {
        "Search codebase with semantic understanding"
    }
    
    fn usage(&self) -> &str {
        "/search <query> [--regex] - Search code (use --regex for regex search)"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Context
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            return Ok(CommandResult::error("Usage: /search <query>"));
        }
        
        let (query, use_regex) = if args.contains("--regex") {
            (args.replace("--regex", "").trim().to_string(), true)
        } else {
            (args.to_string(), false)
        };
        
        let search_args = SearchArgs {
            path: ctx.working_dir.clone(),
            pattern: query.clone(),
            is_regex: Some(use_regex),
            case_insensitive: Some(true),
            file_pattern: None,
            max_results: Some(50),
            context_lines: Some(2),
            max_depth: None,
        };
        
        match ctx.tools.search_files.search(search_args).await {
            Ok(output) => {
                let message = format!(
                    "🔍 Search Results: '{}'\n  Files matched: {}\n  Total matches: {}",
                    query, 
                    output.results.len(),
                    output.results.iter().map(|r| r.line_number).count()
                );
                
                Ok(CommandResult::success(message).with_metadata("query", &query))
            }
            Err(e) => Ok(CommandResult::error(format!("Search failed: {}", e))),
        }
    }
}
