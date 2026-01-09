//! Shell Command - Execute shell commands with security checks

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;
use crate::tools::ShellArgs;

pub struct ShellCommand;

#[async_trait::async_trait]
impl SlashCommand for ShellCommand {
    fn name(&self) -> &str {
        "shell"
    }
    
    fn description(&self) -> &str {
        "Execute shell command with security analysis"
    }
    
    fn usage(&self) -> &str {
        "/shell <command> - Execute shell command"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }
    
    fn validate_args(&self, args: &str) -> Result<()> {
        if args.is_empty() {
            anyhow::bail!("Usage: /shell <command>");
        }
        Ok(())
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        let shell_args = ShellArgs {
            command: args.to_string(),
            args: None,
            working_dir: Some(ctx.working_dir.clone()),
            env: None,
            timeout_secs: Some(300),
            capture_stderr: Some(true),
            shell: None,
        };
        
        match ctx.tools.shell_executor.execute(shell_args).await {
            Ok(result) => {
                let output = if result.success {
                    format!("$ {}\n\n{}\n\nExit code: {}", args, result.stdout, result.exit_code)
                } else {
                    format!("$ {}\n\nError:\n{}\n\nExit code: {}", args, result.stderr, result.exit_code)
                };
                
                Ok(CommandResult::success(output).with_metadata("command", args))
            }
            Err(e) => Ok(CommandResult::error(format!("Command failed: {}", e))),
        }
    }
}
