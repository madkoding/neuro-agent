//! Agent state management with shared context

use crate::tools::TaskPlan;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Role of a message in the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// A single message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier
    pub id: Uuid,
    /// Role of the message sender
    pub role: MessageRole,
    /// Content of the message
    pub content: String,
    /// Timestamp when the message was created
    pub timestamp: DateTime<Utc>,
    /// Which model generated this (for assistant messages)
    pub model: Option<String>,
    /// Tool name if this is a tool response
    pub tool_name: Option<String>,
    /// Token count (if available)
    pub tokens: Option<u32>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.into(),
            timestamp: Utc::now(),
            model: None,
            tool_name: None,
            tokens: None,
        }
    }

    pub fn assistant(content: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Utc::now(),
            model: Some(model.into()),
            tool_name: None,
            tokens: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: content.into(),
            timestamp: Utc::now(),
            model: None,
            tool_name: None,
            tokens: None,
        }
    }

    pub fn tool(tool_name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Tool,
            content: content.into(),
            timestamp: Utc::now(),
            model: None,
            tool_name: Some(tool_name.into()),
            tokens: None,
        }
    }
}

/// Pending task being processed by the heavy model
#[derive(Debug, Clone)]
pub struct PendingTask {
    /// Unique identifier for tracking
    pub id: Uuid,
    /// Description of what's being done
    pub description: String,
    /// When the task was started
    pub started_at: DateTime<Utc>,
    /// Estimated completion time in seconds
    pub estimated_secs: u64,
    /// Cancellation token
    pub cancelled: bool,
}

impl PendingTask {
    pub fn new(description: impl Into<String>, estimated_secs: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            started_at: Utc::now(),
            estimated_secs,
            cancelled: false,
        }
    }

    /// Get elapsed time in seconds
    pub fn elapsed_secs(&self) -> u64 {
        (Utc::now() - self.started_at).num_seconds().max(0) as u64
    }

    /// Get remaining estimated time
    pub fn remaining_secs(&self) -> u64 {
        self.estimated_secs.saturating_sub(self.elapsed_secs())
    }

    /// Check if task has exceeded timeout
    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        self.elapsed_secs() > timeout_secs
    }
}

/// State of the current thinking/streaming process
#[derive(Debug, Clone, Default)]
pub struct StreamingState {
    /// Whether we're currently receiving a stream
    pub is_streaming: bool,
    /// Content accumulated so far
    pub accumulated_content: String,
    /// Whether we're inside a <think> block
    pub in_think_block: bool,
    /// Content of the current think block
    pub think_content: String,
}

impl StreamingState {
    pub fn start(&mut self) {
        self.is_streaming = true;
        self.accumulated_content.clear();
        self.in_think_block = false;
        self.think_content.clear();
    }

    pub fn append(&mut self, chunk: &str) {
        // Check for think block markers
        if chunk.contains("<think>") {
            self.in_think_block = true;
        }
        if chunk.contains("</think>") {
            self.in_think_block = false;
        }

        if self.in_think_block {
            self.think_content.push_str(chunk);
        } else {
            self.accumulated_content.push_str(chunk);
        }
    }

    pub fn finish(&mut self) -> String {
        self.is_streaming = false;
        std::mem::take(&mut self.accumulated_content)
    }
}

/// Shared agent state
#[derive(Debug)]
pub struct AgentState {
    /// Current session ID
    pub session_id: Uuid,
    /// Conversation history
    pub messages: Vec<Message>,
    /// Currently pending tasks (from heavy model)
    pub pending_tasks: Vec<PendingTask>,
    /// Active task plans (for multi-step execution)
    pub active_plans: HashMap<Uuid, TaskPlan>,
    /// Streaming state
    pub streaming: StreamingState,
    /// Working directory
    pub working_dir: String,
    /// Maximum history messages to keep in context
    pub max_history: usize,
    /// Total tokens used in this session
    pub total_tokens: u64,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4(),
            messages: Vec::new(),
            pending_tasks: Vec::new(),
            active_plans: HashMap::new(),
            streaming: StreamingState::default(),
            working_dir: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            max_history: 50,
            total_tokens: 0,
        }
    }

    /// Add a message to the history
    pub fn add_message(&mut self, message: Message) {
        if let Some(tokens) = message.tokens {
            self.total_tokens += tokens as u64;
        }
        self.messages.push(message);

        // Trim history if needed (keep system messages)
        while self.messages.len() > self.max_history {
            // Find first non-system message to remove
            if let Some(idx) = self
                .messages
                .iter()
                .position(|m| m.role != MessageRole::System)
            {
                self.messages.remove(idx);
            } else {
                break;
            }
        }
    }

    /// Get recent messages for context (excluding system)
    pub fn get_context_messages(&self, count: usize) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .rev()
            .take(count)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Clear conversation history (keep system messages)
    pub fn clear_history(&mut self) {
        self.messages.retain(|m| m.role == MessageRole::System);
    }

    /// Add a pending task
    pub fn add_pending_task(&mut self, task: PendingTask) -> Uuid {
        let id = task.id;
        self.pending_tasks.push(task);
        id
    }

    /// Cancel a pending task
    pub fn cancel_task(&mut self, task_id: Uuid) -> bool {
        if let Some(task) = self.pending_tasks.iter_mut().find(|t| t.id == task_id) {
            task.cancelled = true;
            true
        } else {
            false
        }
    }

    /// Remove completed/cancelled tasks
    pub fn cleanup_tasks(&mut self) {
        self.pending_tasks.retain(|t| !t.cancelled);
    }

    /// Get the currently active pending task (if any)
    pub fn active_task(&self) -> Option<&PendingTask> {
        self.pending_tasks.iter().find(|t| !t.cancelled)
    }

    /// Store a task plan
    pub fn store_plan(&mut self, plan: TaskPlan) -> Uuid {
        let id = Uuid::parse_str(&plan.id).unwrap_or_else(|_| Uuid::new_v4());
        self.active_plans.insert(id, plan);
        id
    }

    /// Get a task plan by ID
    pub fn get_plan(&self, id: &Uuid) -> Option<&TaskPlan> {
        self.active_plans.get(id)
    }

    /// Get a mutable task plan by ID
    pub fn get_plan_mut(&mut self, id: &Uuid) -> Option<&mut TaskPlan> {
        self.active_plans.get_mut(id)
    }

    /// Remove a completed or failed plan
    pub fn remove_plan(&mut self, id: &Uuid) -> Option<TaskPlan> {
        self.active_plans.remove(id)
    }

    /// Get all active plans
    pub fn get_all_plans(&self) -> Vec<&TaskPlan> {
        self.active_plans.values().collect()
    }
}

/// Thread-safe shared state
pub type SharedState = Arc<Mutex<AgentState>>;

/// Create a new shared state
pub fn create_shared_state() -> SharedState {
    Arc::new(Mutex::new(AgentState::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let user_msg = Message::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = Message::assistant("Hi there!", "qwen3:0.6b");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);
        assert_eq!(assistant_msg.model, Some("qwen3:0.6b".to_string()));
    }

    #[test]
    fn test_streaming_state() {
        let mut state = StreamingState::default();
        state.start();

        state.append("Hello ");
        state.append("<think>processing</think>");
        state.append("World");

        let result = state.finish();
        assert!(result.contains("Hello "));
        assert!(result.contains("World"));
    }

    #[test]
    fn test_pending_task() {
        let task = PendingTask::new("Test task", 60);
        assert!(!task.cancelled);
        assert!(task.elapsed_secs() < 2);
    }
}
