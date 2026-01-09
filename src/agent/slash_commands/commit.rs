//! Commit Commands - Git workflow automation

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::{GitCommitArgs, GitStatusArgs, GitAddArgs};

pub struct CommitCommand;

#[async_trait::async_trait]
impl SlashCommand for CommitCommand {
    fn name(&self) -> &str {
        "commit"
    }
    
    fn description(&self) -> &str {
        "Generate commit message and commit changes"
    }
    
    fn usage(&self) -> &str {
        "/commit [message] - Commit staged changes (auto-generate message if empty)"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Git
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        // First, add all changes
        let add_args = GitAddArgs {
            path: ctx.working_dir.clone(),
            files: vec![".".to_string()],
        };
        
        if let Err(e) = ctx.tools.git.add(add_args).await {
            return Ok(CommandResult::error(format!("Failed to add files: {}", e)));
        }
        
        let message = if args.is_empty() {
            // Auto-generate commit message from git status
            let status_args = GitStatusArgs {
                path: ctx.working_dir.clone(),
            };
            
            match ctx.tools.git.status(status_args).await {
                Ok(status) => {
                    let staged_count = status.staged.len();
                    format!("chore: update {} file{}", staged_count, if staged_count != 1 { "s" } else { "" })
                }
                Err(_) => "chore: update files".to_string(),
            }
        } else {
            args.to_string()
        };
        
        let commit_args = GitCommitArgs {
            path: ctx.working_dir.clone(),
            message: message.clone(),
        };
        
        match ctx.tools.git.commit(commit_args).await {
            Ok(commit_info) => {
                let result_message = format!(
                    "✅ Committed: {}\n  Hash: {}\n  Author: {}\n  Files changed: {}",
                    commit_info.short_hash, commit_info.hash, commit_info.author, commit_info.files_changed
                );
                Ok(CommandResult::success(result_message).with_metadata("commit_message", &message))
            }
            Err(e) => Ok(CommandResult::error(format!("Commit failed: {}", e))),
        }
    }
}

pub struct CommitPushPrCommand;

#[async_trait::async_trait]
impl SlashCommand for CommitPushPrCommand {
    fn name(&self) -> &str {
        "commit-push-pr"
    }
    
    fn description(&self) -> &str {
        "Commit, push, and create pull request"
    }
    
    fn usage(&self) -> &str {
        "/commit-push-pr [message] - Full git workflow"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::Git
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let mut steps = Vec::new();
        
        // 1. Commit
        let commit_cmd = CommitCommand;
        match commit_cmd.execute(args, ctx).await {
            Ok(result) if result.success => {
                steps.push("✅ Committed changes".to_string());
            }
            Ok(result) => return Ok(result),
            Err(e) => return Ok(CommandResult::error(format!("Commit failed: {}", e))),
        }
        
        // 2. Push (nota: GitTool no tiene método push, esto es un stub)
        steps.push("ℹ️  Push: Run `git push` manually or implement push in GitTool".to_string());
        
        // 3. Create PR (if supported)
        steps.push("ℹ️  PR creation: Use GitHub CLI (`gh pr create`) or web interface".to_string());
        
        Ok(CommandResult::success(format!("# Git Workflow\n\n{}", steps.join("\n"))))
    }
}
