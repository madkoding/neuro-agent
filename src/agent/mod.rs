//! Módulo de Agentes - Sistema de orquestación inteligente
//!
//! Este módulo implementa la lógica de orquestación de modelos de IA,
//! incluyendo routing inteligente, clasificación de tareas y optimizaciones de performance.
//!
//! # Componentes Principales
//!
//! - [`orchestrator::DualModelOrchestrator`] - Orquestador dual con modelo rápido y pesado
//! - [`router_orchestrator::RouterOrchestrator`] - Router simplificado optimizado para modelos pequeños
//! - [`parallel_executor`] - Sistema de ejecución paralela de herramientas (2-3x speedup)
//! - [`classifier`] - Clasificación de complejidad de consultas
//! - [`router`] - Routing entre modelo rápido y pesado
//! - [`classification_cache`] - Cache de clasificaciones para respuestas rápidas
//! - [`progress`] - Sistema de tracking de progreso en tiempo real
//! - [`multistep`] - Ejecución multi-paso con checkpoints y rollback
//! - [`diff_preview`] - Preview interactivo de cambios antes de aplicar
//! - [`undo_stack`] - Sistema de deshacer/rehacer operaciones
//! - [`session`] - Gestión de sesiones de conversación persistentes
//! - [`preloader`] - Pre-carga de contexto para reducir latencia
//! - [`monitoring`] - Sistema de monitoreo y observability
//! - [`error_recovery`] - Sistema de recuperación automática de errores

mod classification_cache;
mod classifier;
pub mod diff_preview;
pub mod error_recovery;
pub mod monitoring;
pub mod multistep;
pub mod orchestrator;
pub mod preloader;
pub mod session;
pub mod undo_stack;
mod parallel_executor;
#[deprecated(since = "2.0.0", note = "Use RouterOrchestrator instead. Will be removed in v2.0 (Feb 2026)")]
pub mod planning_orchestrator;
mod progress;
mod streaming;
mod task_progress;
pub mod prompts;
pub mod provider;
pub mod router;
pub mod router_orchestrator;
pub mod slash_commands;
mod state;

pub use classification_cache::{ClassificationCache, CacheStats};
pub use classifier::TaskType;
pub use diff_preview::{DiffAction, DiffHunk, DiffPreview, DiffStats};
pub use error_recovery::{
    ErrorPattern, ErrorRecovery, ErrorType, RecoveryStats, RetryStrategy, RollbackOperation,
};
pub use monitoring::{
    LatencyPercentiles, LogEvent, LogFormat, LogLevel, MetricsCollector, MetricsSnapshot,
    MonitoringSystem, StructuredLogger,
};
pub use multistep::{
    MultiStepExecutor, PlanStatus, StateSnapshot, StepExecutionResult, StepStatus, TaskPlan,
    TaskStep, Checkpoint,
};
pub use preloader::{ContextPreloader, EmbeddingCache, PreloaderCacheStats, PreloaderState, RaptorCache};
pub use session::{Session, SessionContext, SessionInfo, SessionManager, SessionMessage};
pub use undo_stack::{Operation, OperationType, UndoStack};
pub use orchestrator::{DualModelOrchestrator, OrchestratorResponse};
pub use parallel_executor::{ToolRequest, ToolResult, execute_parallel, combine_results};
#[allow(deprecated)]
pub use planning_orchestrator::{PlanningOrchestrator, PlanningResponse};
pub use progress::{ProgressStage, ProgressTracker, ProgressUpdate};
pub use streaming::StreamChunk;
pub use task_progress::{TaskProgressInfo, TaskProgressStatus};
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

