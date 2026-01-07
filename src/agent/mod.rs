//! Agent module - Dual-model orchestration with self-correction

mod classifier;
pub mod confidence;
pub mod orchestrator;
pub mod planning_orchestrator;
pub mod router;
mod self_correction;
mod state;

pub use classifier::TaskType;
pub use confidence::{ParseMethod, ToolCallCandidate};
pub use orchestrator::{DualModelOrchestrator, OrchestratorResponse};
pub use planning_orchestrator::{PlanningOrchestrator, PlanningResponse, TaskProgressInfo, TaskProgressStatus};
pub use router::{ExecutionPlan, ExecutionStep, IntelligentRouter};
pub use self_correction::SelfCorrectionLoop;
pub use state::{AgentState, Message, MessageRole};
