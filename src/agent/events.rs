//! Defines events for communication between the agent core and the UI layer.

use crate::agent::{OrchestratorResponse, PlanningResponse, progress::ProgressUpdate, task_progress::TaskProgressInfo};

/// Events sent from background agent tasks to the UI for processing.
/// This enum lives in the agent module but is designed to be used by the UI,
/// acting as a public API for agent-to-UI communication.
#[derive(Debug)]
pub enum AgentEvent {
    /// The final, complete response from a non-streaming operation.
    Response(Result<OrchestratorResponse, String>),
    
    /// The final, complete response from a planning operation.
    PlanningResponse(Result<PlanningResponse, String>),
    
    /// A high-level status or "thinking" message.
    Status(String),
    
    /// A detailed progress update for a multi-step operation.
    Progress(ProgressUpdate),
    
    /// A single chunk of a streaming response.
    Chunk(String),

    /// The end of a stream.
    StreamEnd,
    
    /// An error message from an agent task.
    Error(String),
    
    /// Progress update for a specific sub-task within a larger plan.
    TaskProgress(TaskProgressInfo),

    // --- RAPTOR Specific Events ---
    /// A high-level status update during RAPTOR indexing.
    RaptorStatus(String),
    /// A detailed progress update during RAPTOR indexing.
    RaptorProgress {
        stage: String,
        current: usize,
        total: usize,
        detail: String,
    },
    /// Signals that RAPTOR indexing is complete.
    RaptorComplete,
}
