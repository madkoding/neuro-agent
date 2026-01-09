//! Task Progress Tracking Types
//!
//! Common types for tracking task execution progress across different orchestrators.

use serde::{Deserialize, Serialize};

/// Information about task progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressInfo {
    /// Index of the current task (0-based)
    pub task_index: usize,
    /// Total number of tasks
    pub total_tasks: usize,
    /// Description of the task
    pub description: String,
    /// Current status
    pub status: TaskProgressStatus,
}

/// Status of a task in execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskProgressStatus {
    /// Task has started
    Started,
    /// Task has completed successfully with optional result
    Completed(String),
    /// Task has failed with error message
    Failed(String),
}
