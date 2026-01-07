//! Task planner - Plans complex tasks into subtasks

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A task in the plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub dependencies: Vec<String>,
    pub tool_to_use: Option<String>,
    pub tool_args: Option<serde_json::Value>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub priority: u8,
    pub estimated_effort: TaskEffort,
}

/// Type of task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    /// Analyze/understand something
    Analysis,
    /// Read files or gather information
    Research,
    /// Write or modify code
    Implementation,
    /// Test or validate
    Testing,
    /// Review or refactor
    Review,
    /// Execute commands
    Execution,
    /// Documentation
    Documentation,
    /// Planning subtask
    Planning,
}

/// Status of a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
    Blocked,
}

/// Estimated effort
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskEffort {
    Trivial,    // < 1 min
    Small,      // 1-5 min
    Medium,     // 5-15 min  
    Large,      // 15-60 min
    Complex,    // > 1 hour
}

/// A plan containing multiple tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub id: String,
    pub goal: String,
    pub tasks: Vec<Task>,
    pub current_task_index: usize,
    pub status: PlanStatus,
    pub context: HashMap<String, String>,
    pub created_at: u64,
}

/// Status of the overall plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Draft,
    Created,
    InProgress,
    Completed,
    Failed,
    Paused,
    Cancelled,
}

/// Task planner tool
#[derive(Debug, Clone, Default)]
pub struct TaskPlannerTool;

impl TaskPlannerTool {
    pub const NAME: &'static str = "task_planner";

    pub fn new() -> Self {
        Self
    }

    /// Create a plan from a goal description
    pub fn create_plan(&self, goal: &str, analysis: &str) -> TaskPlan {
        let id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Parse the analysis to extract tasks
        let tasks = self.parse_tasks_from_analysis(analysis);

        TaskPlan {
            id,
            goal: goal.to_string(),
            tasks,
            current_task_index: 0,
            status: PlanStatus::Created,
            context: HashMap::new(),
            created_at: now,
        }
    }

    /// Parse a plan from LLM response (alias for create_plan)
    pub fn parse_plan(&self, goal: &str, response: &str) -> Result<TaskPlan, String> {
        Ok(self.create_plan(goal, response))
    }

    /// Parse tasks from LLM analysis
    fn parse_tasks_from_analysis(&self, analysis: &str) -> Vec<Task> {
        let mut tasks = Vec::new();
        let mut task_num = 0;

        for line in analysis.lines() {
            let line = line.trim();
            
            // Look for numbered tasks or bullet points
            if line.starts_with(|c: char| c.is_ascii_digit()) || 
               line.starts_with('-') || 
               line.starts_with('*') ||
               line.starts_with("Task") ||
               line.starts_with("Step") {
                
                // Extract task description
                let description = line
                    .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '*' || c == ' ' || c == ':')
                    .trim();

                if description.len() > 5 {
                    task_num += 1;
                    let task_type = self.infer_task_type(description);
                    let tool = self.infer_tool(description, &task_type);
                    let effort = self.estimate_effort(description, &task_type);

                    tasks.push(Task {
                        id: format!("task_{}", task_num),
                        title: format!("Task {}", task_num),
                        description: description.to_string(),
                        task_type,
                        status: TaskStatus::Pending,
                        dependencies: if task_num > 1 {
                            vec![format!("task_{}", task_num - 1)]
                        } else {
                            vec![]
                        },
                        tool_to_use: tool,
                        tool_args: None,
                        result: None,
                        error: None,
                        priority: (100 - task_num as u8).max(1),
                        estimated_effort: effort,
                    });
                }
            }
        }

        // If no tasks were parsed, create a single task
        if tasks.is_empty() {
            tasks.push(Task {
                id: "task_1".to_string(),
                title: "Main Task".to_string(),
                description: analysis.to_string(),
                task_type: TaskType::Implementation,
                status: TaskStatus::Pending,
                dependencies: vec![],
                tool_to_use: None,
                tool_args: None,
                result: None,
                error: None,
                priority: 100,
                estimated_effort: TaskEffort::Medium,
            });
        }

        tasks
    }

    /// Infer task type from description
    fn infer_task_type(&self, description: &str) -> TaskType {
        let lower = description.to_lowercase();

        if lower.contains("analyz") || lower.contains("understand") || lower.contains("review") || lower.contains("check") {
            TaskType::Analysis
        } else if lower.contains("read") || lower.contains("find") || lower.contains("search") || lower.contains("look") || lower.contains("gather") {
            TaskType::Research
        } else if lower.contains("write") || lower.contains("create") || lower.contains("implement") || lower.contains("add") || lower.contains("modify") || lower.contains("update") || lower.contains("fix") {
            TaskType::Implementation
        } else if lower.contains("test") || lower.contains("verify") || lower.contains("validate") || lower.contains("run") {
            TaskType::Testing
        } else if lower.contains("refactor") || lower.contains("clean") || lower.contains("optimize") {
            TaskType::Review
        } else if lower.contains("execute") || lower.contains("command") || lower.contains("shell") || lower.contains("install") || lower.contains("build") {
            TaskType::Execution
        } else if lower.contains("document") || lower.contains("readme") || lower.contains("comment") {
            TaskType::Documentation
        } else if lower.contains("plan") || lower.contains("design") || lower.contains("architect") {
            TaskType::Planning
        } else {
            TaskType::Implementation
        }
    }

    /// Infer which tool to use
    fn infer_tool(&self, description: &str, task_type: &TaskType) -> Option<String> {
        let lower = description.to_lowercase();

        match task_type {
            TaskType::Research => {
                if lower.contains("file") || lower.contains("read") {
                    Some("read_file".to_string())
                } else if lower.contains("director") || lower.contains("list") || lower.contains("find") {
                    Some("list_directory".to_string())
                } else if lower.contains("search") || lower.contains("grep") {
                    Some("search_in_files".to_string())
                } else if lower.contains("index") || lower.contains("project") {
                    Some("index_project".to_string())
                } else {
                    None
                }
            }
            TaskType::Implementation => {
                if lower.contains("write") || lower.contains("create") || lower.contains("modify") {
                    Some("write_file".to_string())
                } else {
                    None
                }
            }
            TaskType::Testing => {
                if lower.contains("lint") || lower.contains("check") {
                    Some("run_linter".to_string())
                } else if lower.contains("test") || lower.contains("cargo test") {
                    Some("execute_shell".to_string())
                } else {
                    None
                }
            }
            TaskType::Execution => {
                Some("execute_shell".to_string())
            }
            TaskType::Analysis => {
                if lower.contains("code") || lower.contains("function") || lower.contains("class") {
                    Some("analyze_code".to_string())
                } else if lower.contains("depend") {
                    Some("analyze_dependencies".to_string())
                } else if lower.contains("git") {
                    Some("git_tool".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Estimate effort based on description
    fn estimate_effort(&self, description: &str, task_type: &TaskType) -> TaskEffort {
        let lower = description.to_lowercase();
        let word_count = description.split_whitespace().count();

        // Complex indicators
        if lower.contains("refactor") || lower.contains("redesign") || lower.contains("migrate") || lower.contains("complete") {
            return TaskEffort::Complex;
        }

        // Large indicators
        if lower.contains("implement") || lower.contains("create new") || lower.contains("build") {
            return TaskEffort::Large;
        }

        // Based on task type
        match task_type {
            TaskType::Research | TaskType::Analysis => TaskEffort::Small,
            TaskType::Documentation => TaskEffort::Medium,
            TaskType::Implementation => {
                if word_count > 20 {
                    TaskEffort::Large
                } else {
                    TaskEffort::Medium
                }
            }
            TaskType::Testing => TaskEffort::Small,
            TaskType::Execution => TaskEffort::Trivial,
            TaskType::Review => TaskEffort::Medium,
            TaskType::Planning => TaskEffort::Small,
        }
    }

    /// Get the next task to execute
    pub fn get_next_task<'a>(&self, plan: &'a TaskPlan) -> Option<&'a Task> {
        plan.tasks.iter().find(|t| t.status == TaskStatus::Pending)
    }

    /// Mark a task as completed
    pub fn complete_task(&self, plan: &mut TaskPlan, task_id: &str, result: String) {
        if let Some(task) = plan.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
        }
        self.update_plan_status(plan);
    }

    /// Mark a task as failed
    pub fn fail_task(&self, plan: &mut TaskPlan, task_id: &str, error: String) {
        if let Some(task) = plan.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Failed;
            task.error = Some(error);
        }
        self.update_plan_status(plan);
    }

    /// Update overall plan status
    fn update_plan_status(&self, plan: &mut TaskPlan) {
        let all_completed = plan.tasks.iter().all(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped);
        let any_failed = plan.tasks.iter().any(|t| t.status == TaskStatus::Failed);
        let any_in_progress = plan.tasks.iter().any(|t| t.status == TaskStatus::InProgress);

        if all_completed {
            plan.status = PlanStatus::Completed;
        } else if any_failed {
            plan.status = PlanStatus::Failed;
        } else if any_in_progress {
            plan.status = PlanStatus::InProgress;
        }
    }

    /// Generate a summary of the plan
    pub fn summarize_plan(&self, plan: &TaskPlan) -> String {
        let mut summary = format!("# Plan: {}\n\n", plan.goal);
        summary.push_str(&format!("Status: {:?}\n\n", plan.status));
        summary.push_str("## Tasks:\n");

        for task in &plan.tasks {
            let status_icon = match task.status {
                TaskStatus::Completed => "✅",
                TaskStatus::Failed => "❌",
                TaskStatus::InProgress => "🔄",
                TaskStatus::Pending => "⏳",
                TaskStatus::Skipped => "⏭️",
                TaskStatus::Blocked => "🚫",
            };
            summary.push_str(&format!("{} {} - {}\n", status_icon, task.id, task.description));
            if let Some(ref result) = task.result {
                let preview: String = result.chars().take(100).collect();
                summary.push_str(&format!("   Result: {}...\n", preview));
            }
            if let Some(ref error) = task.error {
                summary.push_str(&format!("   Error: {}\n", error));
            }
        }

        summary
    }
}

/// Arguments for creating a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanArgs {
    pub goal: String,
    pub context: Option<String>,
}

/// Arguments for executing a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutePlanArgs {
    pub plan_id: String,
    pub max_tasks: Option<usize>,
}

/// Prompt template for task planning
pub const PLANNING_PROMPT: &str = r#"You are a task planning assistant. Given a goal, break it down into clear, actionable tasks.

For each task, specify:
1. What needs to be done (clear description)
2. Any dependencies on other tasks
3. The type of action (read, write, execute, analyze, test)

Format your response as a numbered list of tasks, with each task being a single clear action.

Goal: {goal}

Context: {context}

Please provide a step-by-step plan:"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_plan() {
        let planner = TaskPlannerTool;
        let analysis = r#"
        1. Read the current configuration file
        2. Analyze the existing code structure
        3. Write the new feature implementation
        4. Test the changes
        5. Update documentation
        "#;

        let plan = planner.create_plan("Add new feature", analysis);
        assert_eq!(plan.tasks.len(), 5);
        assert_eq!(plan.status, PlanStatus::Created);
    }

    #[test]
    fn test_task_type_inference() {
        let planner = TaskPlannerTool;
        
        assert_eq!(planner.infer_task_type("Read the configuration"), TaskType::Research);
        assert_eq!(planner.infer_task_type("Write new code"), TaskType::Implementation);
        assert_eq!(planner.infer_task_type("Test the function"), TaskType::Testing);
        assert_eq!(planner.infer_task_type("Execute the build"), TaskType::Execution);
    }

    #[test]
    fn test_plan_progression() {
        let planner = TaskPlannerTool;
        let mut plan = planner.create_plan("Test", "1. Task one\n2. Task two");

        planner.complete_task(&mut plan, "task_1", "Done".to_string());
        assert_eq!(plan.tasks[0].status, TaskStatus::Completed);

        planner.complete_task(&mut plan, "task_2", "Done".to_string());
        assert_eq!(plan.status, PlanStatus::Completed);
    }
}
