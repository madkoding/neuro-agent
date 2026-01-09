//! Plan Command - Generate execution plans

use super::{CommandCategory, CommandContext, CommandResult, SlashCommand};
use anyhow::Result;

pub struct PlanCommand;

#[async_trait::async_trait]
impl SlashCommand for PlanCommand {
    fn name(&self) -> &str {
        "plan"
    }
    
    fn description(&self) -> &str {
        "Generate a task execution plan without executing"
    }
    
    fn usage(&self) -> &str {
        "/plan <task_description> - Generate plan for a task"
    }
    
    fn category(&self) -> CommandCategory {
        CommandCategory::System
    }
    
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            return Ok(CommandResult::error("Usage: /plan <task_description>"));
        }
        
        // Use the planner tool to generate a structured plan
        let planner = &ctx.tools.task_planner;
        
        // Ask planner to create a plan from the task description
        let plan = planner.create_plan(args, args);
        
        let mut output = format!("📋 Generated Plan: {}\n\n", plan.goal);
        output.push_str(&format!("Total steps: {}\n", plan.tasks.len()));
        
        // Estimate total time based on effort
        let total_minutes: usize = plan.tasks.iter().map(|t| {
            match t.estimated_effort {
                crate::tools::TaskEffort::Trivial => 1,
                crate::tools::TaskEffort::Small => 3,
                crate::tools::TaskEffort::Medium => 10,
                crate::tools::TaskEffort::Large => 30,
                crate::tools::TaskEffort::Complex => 60,
            }
        }).sum();
        
        output.push_str(&format!("Estimated time: ~{} minutes\n\n", total_minutes));
        
        output.push_str("## Steps:\n");
        for (i, task) in plan.tasks.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, task.description));
            if !task.dependencies.is_empty() {
                output.push_str(&format!("   Dependencies: {:?}\n", task.dependencies));
            }
            if let Some(ref tool) = task.tool_to_use {
                output.push_str(&format!("   Tool: {}\n", tool));
            }
        }
        
        output.push_str("\n💡 To execute this plan, use the task description in normal chat mode.\n");
        
        Ok(CommandResult::success(output)
            .with_metadata("task", args)
            .with_metadata("steps", plan.tasks.len().to_string()))
    }
}
