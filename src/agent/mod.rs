//! Módulo de Agentes - Sistema de orquestación inteligente
//!
//! Este módulo implementa la lógica de orquestación de modelos de IA,
//! incluyendo routing inteligente, clasificación de tareas y auto-corrección.
//!
//! # Componentes Principales
//!
//! - [`orchestrator::DualModelOrchestrator`] - Orquestador dual con modelo rápido y pesado
//! - [`planning_orchestrator::PlanningOrchestrator`] - Sistema de planificación de tareas
//! - [`classifier`] - Clasificación de complejidad de consultas
//! - [`router`] - Routing entre modelo rápido y pesado
//! - [`self_correction`] - Sistema de auto-corrección de errores

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
pub use planning_orchestrator::{
    PlanningOrchestrator, PlanningResponse, TaskProgressInfo, TaskProgressStatus,
};
pub use router::{ExecutionPlan, ExecutionStep, IntelligentRouter};
pub use self_correction::SelfCorrectionLoop;
pub use state::{AgentState, Message, MessageRole};
