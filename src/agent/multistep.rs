//! Multi-step Task Execution with Checkpoints and Rollback
//!
//! This module provides functionality for breaking down complex tasks into
//! manageable steps, executing them with checkpoints, and rolling back on failure.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;

use crate::log_debug;

/// Status of an entire task plan
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    /// Plan is being created
    Planning,
    /// Plan is ready to execute
    Ready,
    /// Plan is currently executing
    Running,
    /// Plan completed successfully
    Completed,
    /// Plan failed at a step
    Failed { step_index: usize, error: String },
    /// Plan was paused (can be resumed)
    Paused { at_step: usize },
    /// Plan was cancelled by user
    Cancelled,
}

/// Status of an individual task step
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    /// Step not yet started
    Pending,
    /// Step is currently running
    Running,
    /// Step completed successfully
    Completed,
    /// Step failed with error
    Failed(String),
    /// Step was skipped (e.g., conditional)
    Skipped,
}

/// A checkpoint represents the state before executing a step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// When this checkpoint was created
    pub timestamp: SystemTime,
    /// Snapshot of relevant state (files, variables, etc.)
    pub state_snapshot: StateSnapshot,
    /// Files that were modified by this step
    pub files_modified: Vec<PathBuf>,
}

/// Snapshot of system state for rollback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// File contents before modification (path -> content)
    pub file_backups: HashMap<String, String>,
    /// Working directory
    pub working_dir: PathBuf,
    /// Environment variables (if needed)
    pub env_vars: HashMap<String, String>,
}

/// A single step in a task plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    /// Unique step ID
    pub id: usize,
    /// Human-readable description
    pub description: String,
    /// Current status
    pub status: StepStatus,
    /// Tool calls to execute for this step
    pub tool_calls: Vec<String>, // Tool names
    /// Checkpoint before this step (for rollback)
    pub checkpoint: Option<Checkpoint>,
    /// Result/output of this step
    pub result: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
}

/// Complete task plan with multiple steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    /// Unique plan ID
    pub id: String,
    /// High-level goal description
    pub goal: String,
    /// All steps in the plan
    pub steps: Vec<TaskStep>,
    /// Index of current step being executed
    pub current_step: usize,
    /// Overall plan status
    pub status: PlanStatus,
    /// When plan was created
    pub created_at: SystemTime,
    /// When plan was last updated
    pub updated_at: SystemTime,
}

impl TaskPlan {
    /// Create a new task plan
    pub fn new(goal: String, steps: Vec<TaskStep>) -> Self {
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4().to_string(),
            goal,
            steps,
            current_step: 0,
            status: PlanStatus::Ready,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get the current step being executed
    pub fn current_step(&self) -> Option<&TaskStep> {
        self.steps.get(self.current_step)
    }

    /// Get mutable reference to current step
    pub fn current_step_mut(&mut self) -> Option<&mut TaskStep> {
        self.steps.get_mut(self.current_step)
    }

    /// Advance to next step
    pub fn advance(&mut self) -> bool {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            self.updated_at = SystemTime::now();
            true
        } else {
            false
        }
    }

    /// Mark current step as completed and advance
    pub fn complete_step(&mut self, result: Option<String>) -> Result<bool> {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Completed;
            step.result = result;
            self.updated_at = SystemTime::now();
            
            let has_next = self.advance();
            
            if !has_next {
                self.status = PlanStatus::Completed;
            }
            
            Ok(has_next)
        } else {
            Ok(false)
        }
    }

    /// Mark current step as failed
    pub fn fail_step(&mut self, error: String) {
        if let Some(step) = self.current_step_mut() {
            step.status = StepStatus::Failed(error.clone());
            self.status = PlanStatus::Failed {
                step_index: self.current_step,
                error,
            };
            self.updated_at = SystemTime::now();
        }
    }

    /// Pause execution at current step
    pub fn pause(&mut self) {
        self.status = PlanStatus::Paused {
            at_step: self.current_step,
        };
        self.updated_at = SystemTime::now();
    }

    /// Resume execution from paused state
    pub fn resume(&mut self) -> Result<()> {
        match &self.status {
            PlanStatus::Paused { .. } => {
                self.status = PlanStatus::Running;
                self.updated_at = SystemTime::now();
                Ok(())
            }
            _ => anyhow::bail!("Cannot resume plan that is not paused"),
        }
    }

    /// Cancel the entire plan
    pub fn cancel(&mut self) {
        self.status = PlanStatus::Cancelled;
        self.updated_at = SystemTime::now();
    }

    /// Get progress as percentage (0-100)
    pub fn progress_percent(&self) -> u8 {
        if self.steps.is_empty() {
            return 100;
        }
        
        let completed = self.steps.iter()
            .filter(|s| matches!(s.status, StepStatus::Completed))
            .count();
        
        ((completed as f32 / self.steps.len() as f32) * 100.0) as u8
    }

    /// Check if plan can be executed
    pub fn can_execute(&self) -> bool {
        matches!(
            self.status,
            PlanStatus::Ready | PlanStatus::Running | PlanStatus::Paused { .. }
        )
    }

    /// Get summary of plan execution
    pub fn summary(&self) -> String {
        let completed = self.steps.iter()
            .filter(|s| matches!(s.status, StepStatus::Completed))
            .count();
        let failed = self.steps.iter()
            .filter(|s| matches!(s.status, StepStatus::Failed(_)))
            .count();
        let pending = self.steps.iter()
            .filter(|s| matches!(s.status, StepStatus::Pending))
            .count();

        format!(
            "Plan: {} ({})\nSteps: {} total, {} completed, {} failed, {} pending\nProgress: {}%",
            self.goal,
            match &self.status {
                PlanStatus::Planning => "Planning",
                PlanStatus::Ready => "Ready",
                PlanStatus::Running => "Running",
                PlanStatus::Completed => "Completed",
                PlanStatus::Failed { .. } => "Failed",
                PlanStatus::Paused { .. } => "Paused",
                PlanStatus::Cancelled => "Cancelled",
            },
            self.steps.len(),
            completed,
            failed,
            pending,
            self.progress_percent()
        )
    }
}

impl TaskStep {
    /// Create a new task step
    pub fn new(id: usize, description: String, tool_calls: Vec<String>) -> Self {
        Self {
            id,
            description,
            status: StepStatus::Pending,
            tool_calls,
            checkpoint: None,
            result: None,
            duration_ms: None,
        }
    }

    /// Create a checkpoint before executing this step
    pub fn create_checkpoint(&mut self, files_to_backup: Vec<PathBuf>) -> Result<()> {
        let mut file_backups = HashMap::new();
        
        for path in &files_to_backup {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to backup file: {:?}", path))?;
                file_backups.insert(path.to_string_lossy().to_string(), content);
            }
        }

        self.checkpoint = Some(Checkpoint {
            timestamp: SystemTime::now(),
            state_snapshot: StateSnapshot {
                file_backups,
                working_dir: std::env::current_dir()?,
                env_vars: HashMap::new(), // Can be populated if needed
            },
            files_modified: files_to_backup,
        });

        Ok(())
    }

    /// Rollback this step using its checkpoint
    pub fn rollback(&self) -> Result<()> {
        if let Some(ref checkpoint) = self.checkpoint {
            log_debug!("Rolling back step {}: {}", self.id, self.description);
            
            // Restore file contents
            for (path_str, content) in &checkpoint.state_snapshot.file_backups {
                let path = PathBuf::from(path_str);
                std::fs::write(&path, content)
                    .with_context(|| format!("Failed to restore file: {:?}", path))?;
                log_debug!("Restored: {:?}", path);
            }
            
            Ok(())
        } else {
            anyhow::bail!("No checkpoint available for rollback")
        }
    }

    /// Check if step has a checkpoint
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint.is_some()
    }
}

impl Checkpoint {
    /// Get number of files backed up
    pub fn backup_count(&self) -> usize {
        self.state_snapshot.file_backups.len()
    }
}

impl StateSnapshot {
    /// Create an empty snapshot
    pub fn empty() -> Self {
        Self {
            file_backups: HashMap::new(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            env_vars: HashMap::new(),
        }
    }
}

/// Multi-step executor
pub struct MultiStepExecutor {
    /// Currently active plans
    active_plans: HashMap<String, TaskPlan>,
}

impl MultiStepExecutor {
    /// Create a new executor
    pub fn new() -> Self {
        Self {
            active_plans: HashMap::new(),
        }
    }

    /// Register a new plan
    pub fn register_plan(&mut self, plan: TaskPlan) -> String {
        let id = plan.id.clone();
        self.active_plans.insert(id.clone(), plan);
        id
    }

    /// Get a plan by ID
    pub fn get_plan(&self, plan_id: &str) -> Option<&TaskPlan> {
        self.active_plans.get(plan_id)
    }

    /// Get mutable plan by ID
    pub fn get_plan_mut(&mut self, plan_id: &str) -> Option<&mut TaskPlan> {
        self.active_plans.get_mut(plan_id)
    }

    /// Remove a completed or cancelled plan
    pub fn remove_plan(&mut self, plan_id: &str) -> Option<TaskPlan> {
        self.active_plans.remove(plan_id)
    }

    /// List all active plans
    pub fn list_plans(&self) -> Vec<&TaskPlan> {
        self.active_plans.values().collect()
    }

    /// Execute a single step of a plan
    pub async fn execute_step(&mut self, plan_id: &str) -> Result<StepExecutionResult> {
        let plan = self.get_plan_mut(plan_id)
            .ok_or_else(|| anyhow::anyhow!("Plan not found: {}", plan_id))?;

        if !plan.can_execute() {
            return Ok(StepExecutionResult::PlanNotExecutable);
        }

        let step = plan.current_step_mut()
            .ok_or_else(|| anyhow::anyhow!("No current step"))?;

        // Mark as running
        step.status = StepStatus::Running;
        
        // In a real implementation, this would execute the tool calls
        // For now, we simulate execution
        let start = SystemTime::now();
        
        // Simulate success
        step.status = StepStatus::Completed;
        step.duration_ms = start.elapsed().ok().map(|d| d.as_millis() as u64);

        Ok(StepExecutionResult::StepCompleted {
            step_id: step.id,
            has_next: plan.current_step + 1 < plan.steps.len(),
        })
    }

    /// Rollback last completed step
    pub fn rollback_last_step(&mut self, plan_id: &str) -> Result<()> {
        let plan = self.get_plan(plan_id)
            .ok_or_else(|| anyhow::anyhow!("Plan not found: {}", plan_id))?;

        if plan.current_step == 0 {
            anyhow::bail!("No steps to rollback");
        }

        let prev_step_index = plan.current_step - 1;
        let step = &plan.steps[prev_step_index];
        
        step.rollback()?;

        // Update plan state
        let plan = self.get_plan_mut(plan_id).unwrap();
        plan.current_step = prev_step_index;
        plan.updated_at = SystemTime::now();
        
        if let Some(step) = plan.steps.get_mut(prev_step_index) {
            step.status = StepStatus::Pending;
            step.result = None;
        }

        Ok(())
    }
}

impl Default for MultiStepExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing a step
#[derive(Debug)]
pub enum StepExecutionResult {
    /// Step completed successfully
    StepCompleted { step_id: usize, has_next: bool },
    /// Step failed
    StepFailed { step_id: usize, error: String },
    /// Plan is not in executable state
    PlanNotExecutable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_plan_creation() {
        let steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec!["tool1".to_string()]),
            TaskStep::new(1, "Step 2".to_string(), vec!["tool2".to_string()]),
        ];
        
        let plan = TaskPlan::new("Test goal".to_string(), steps);
        
        assert_eq!(plan.goal, "Test goal");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.current_step, 0);
        assert_eq!(plan.status, PlanStatus::Ready);
    }

    #[test]
    fn test_step_completion() {
        let steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec![]),
            TaskStep::new(1, "Step 2".to_string(), vec![]),
        ];
        
        let mut plan = TaskPlan::new("Test".to_string(), steps);
        
        let has_next = plan.complete_step(Some("Result 1".to_string())).unwrap();
        assert!(has_next);
        assert_eq!(plan.current_step, 1);
        assert_eq!(plan.steps[0].status, StepStatus::Completed);
        
        let has_next = plan.complete_step(Some("Result 2".to_string())).unwrap();
        assert!(!has_next);
        assert_eq!(plan.status, PlanStatus::Completed);
    }

    #[test]
    fn test_step_failure() {
        let steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec![]),
        ];
        
        let mut plan = TaskPlan::new("Test".to_string(), steps);
        
        plan.fail_step("Something went wrong".to_string());
        
        assert_eq!(plan.steps[0].status, StepStatus::Failed("Something went wrong".to_string()));
        assert!(matches!(plan.status, PlanStatus::Failed { .. }));
    }

    #[test]
    fn test_pause_resume() {
        let steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec![]),
        ];
        
        let mut plan = TaskPlan::new("Test".to_string(), steps);
        
        plan.pause();
        assert!(matches!(plan.status, PlanStatus::Paused { .. }));
        
        plan.resume().unwrap();
        assert_eq!(plan.status, PlanStatus::Running);
    }

    #[test]
    fn test_progress_calculation() {
        let mut steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec![]),
            TaskStep::new(1, "Step 2".to_string(), vec![]),
            TaskStep::new(2, "Step 3".to_string(), vec![]),
            TaskStep::new(3, "Step 4".to_string(), vec![]),
        ];
        
        steps[0].status = StepStatus::Completed;
        steps[1].status = StepStatus::Completed;
        
        let plan = TaskPlan::new("Test".to_string(), steps);
        
        assert_eq!(plan.progress_percent(), 50);
    }

    #[test]
    fn test_checkpoint_creation() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Original content").unwrap();
        let path = temp_file.path().to_path_buf();

        let mut step = TaskStep::new(0, "Test step".to_string(), vec![]);
        step.create_checkpoint(vec![path.clone()]).unwrap();

        assert!(step.has_checkpoint());
        assert_eq!(step.checkpoint.as_ref().unwrap().backup_count(), 1);
    }

    #[test]
    fn test_rollback() {
        use tempfile::NamedTempFile;
        use std::io::Write;

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Original content").unwrap();
        let path = temp_file.path().to_path_buf();

        let mut step = TaskStep::new(0, "Test step".to_string(), vec![]);
        step.create_checkpoint(vec![path.clone()]).unwrap();

        // Modify file
        std::fs::write(&path, "Modified content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Modified content");

        // Rollback
        step.rollback().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Original content\n");
    }

    #[test]
    fn test_executor_registration() {
        let mut executor = MultiStepExecutor::new();
        
        let steps = vec![
            TaskStep::new(0, "Step 1".to_string(), vec![]),
        ];
        let plan = TaskPlan::new("Test".to_string(), steps);
        let plan_id = plan.id.clone();
        
        executor.register_plan(plan);
        
        assert!(executor.get_plan(&plan_id).is_some());
        assert_eq!(executor.list_plans().len(), 1);
    }
}
