//! # PlanningOrchestrator (DEPRECATED)
//!
//! ⚠️ **THIS MODULE IS DEPRECATED AND WILL BE REMOVED**
//!
//! Use `RouterOrchestrator` instead. This stub exists only for backward compatibility.
//!
//! ## Migration Path
//! 1. Set `use_router_orchestrator: true` in your config
//! 2. Use `RouterOrchestrator` directly in your code
//! 3. Remove all references to `PlanningOrchestrator`
//!
//! **Target Removal:** v2.0 (Feb 2026)

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

use super::orchestrator::{DualModelOrchestrator, OrchestratorResponse};
use super::state::SharedState;
use crate::tools::ToolRegistry;

/// # DEPRECATED: PlanningOrchestrator
///
/// Use `RouterOrchestrator` instead.
#[deprecated(since = "2.0.0", note = "Use RouterOrchestrator instead. Will be removed in v2.0")]
pub struct PlanningOrchestrator {
    _phantom: (),
}

impl PlanningOrchestrator {
    /// Creates a new PlanningOrchestrator (DEPRECATED)
    #[deprecated(since = "2.0.0", note = "Use RouterOrchestrator::new() instead")]
    pub fn new(
        _orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
        _tools: Arc<ToolRegistry>,
        _state: SharedState,
        _working_dir: PathBuf,
    ) -> Self {
        eprintln!("❌ FATAL ERROR: PlanningOrchestrator is deprecated and removed!");
        eprintln!("   Use RouterOrchestrator instead.");
        eprintln!("   Set use_router_orchestrator: true in config or NEURO_USE_ROUTER=true");
        panic!("PlanningOrchestrator is deprecated. Use RouterOrchestrator.");
    }
    
    /// Process query (DEPRECATED)
    #[allow(dead_code)]
    pub async fn process(&self, _query: &str) -> Result<OrchestratorResponse> {
        unreachable!("PlanningOrchestrator::process called on deprecated stub")
    }
    
    /// Initialize RAPTOR with progress (DEPRECATED)
    #[allow(dead_code)]
    pub async fn initialize_raptor_with_progress(
        &mut self,
        _progress_tx: Option<tokio::sync::mpsc::Sender<crate::agent::TaskProgressInfo>>,
    ) -> Result<bool> {
        unreachable!("PlanningOrchestrator::initialize_raptor_with_progress called on deprecated stub")
    }
    
    /// Process with planning and progress (DEPRECATED)
    #[allow(dead_code)]
    pub async fn process_with_planning_and_progress(
        &mut self,
        _query: &str,
        _progress_tx: Option<tokio::sync::mpsc::Sender<crate::agent::TaskProgressInfo>>,
    ) -> Result<PlanningResponse> {
        unreachable!("PlanningOrchestrator::process_with_planning_and_progress called on deprecated stub")
    }
}

/// Planning response (DEPRECATED)
#[deprecated(since = "2.0.0", note = "Use OrchestratorResponse instead")]
#[derive(Debug, Clone)]
pub enum PlanningResponse {
    /// Simple orchestrator response
    Simple(OrchestratorResponse),
    /// Plan started
    PlanStarted {
        /// Goal description
        goal: String,
        /// Total tasks
        total_tasks: usize,
    },
    /// Plan completed
    PlanCompleted {
        /// Final result
        result: String,
    },
    /// Plan failed
    PlanFailed {
        /// Error message
        error: String,
        /// Tasks completed before failure
        tasks_completed: usize,
    },
    /// Task completed
    TaskCompleted {
        /// Task index
        task_index: usize,
        /// Total tasks
        total_tasks: usize,
    },
}

/// Task progress information (moved to task_progress.rs)
#[deprecated(since = "2.0.0", note = "Use crate::agent::TaskProgressInfo instead")]
pub type TaskProgressInfo = crate::agent::TaskProgressInfo;

/// Task progress status (moved to task_progress.rs)
#[deprecated(since = "2.0.0", note = "Use crate::agent::TaskProgressStatus instead")]
pub type TaskProgressStatus = crate::agent::TaskProgressStatus;
