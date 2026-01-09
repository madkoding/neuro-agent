//! Módulo de Agentes - Sistema de orquestación inteligente
//!
//! Este módulo implementa la lógica de orquestación de modelos de IA,
//! incluyendo routing inteligente, clasificación de tareas y optimizaciones de performance.
//!
//! # Componentes Principales
//!
//! - [`orchestrator::DualModelOrchestrator`] - Orquestador dual con modelo rápido y pesado
//! - [`router_orchestrator::RouterOrchestrator`] - Router simplificado optimizado para modelos pequeños
//! - [`planning_orchestrator::PlanningOrchestrator`] - Sistema de planificación de tareas (deprecated)
//! - [`classifier`] - Clasificación de complejidad de consultas
//! - [`router`] - Routing entre modelo rápido y pesado
//! - [`classification_cache`] - Cache de clasificaciones para respuestas rápidas
//! - [`progress`] - Sistema de tracking de progreso en tiempo real

mod classification_cache;
mod classifier;
pub mod orchestrator;
mod progress;
#[deprecated(since = "2.0.0", note = "Use RouterOrchestrator instead. Will be removed in v2.0 (target: Feb 2026)")]
pub mod planning_orchestrator;
pub mod prompts;
pub mod provider;
pub mod router;
pub mod router_orchestrator;
pub mod slash_commands;
mod state;

pub use classification_cache::{ClassificationCache, CacheStats};
pub use classifier::TaskType;
pub use orchestrator::{DualModelOrchestrator, OrchestratorResponse};
#[allow(deprecated)]
pub use planning_orchestrator::{
    PlanningOrchestrator, PlanningResponse, TaskProgressInfo, TaskProgressStatus,
};
pub use progress::{ProgressStage, ProgressTracker, ProgressUpdate};
pub use prompts::{
    build_minimal_system_prompt, build_proactive_validation_prompt, ProactiveValidationResponse,
    PromptConfig,
};
pub use provider::{
    OllamaFunction, OllamaFunctionCall, OllamaMessage, OllamaTool, OllamaToolCall,
};
pub use router::{ExecutionPlan, ExecutionStep, IntelligentRouter};
pub use router_orchestrator::{OperationMode, RouterConfig, RouterDecision, RouterOrchestrator};
pub use state::{AgentState, Message, MessageRole};

