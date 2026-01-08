//! Neuro - AI Programming Assistant
//!
//! Neuro es un asistente de programación con IA que combina un sistema de orquestación
//! dual de modelos con capacidades avanzadas de análisis de código y RAG.
//!
//! # Arquitectura
//!
//! - **Dual Model Orchestration**: Sistema que combina un modelo rápido (para tareas simples)
//!   con un modelo pesado (para tareas complejas)
//! - **Planning Orchestrator**: Sistema de planificación que descompone tareas complejas
//!   en subtareas ejecutables
//! - **RAPTOR Integration**: Indexación recursiva jerárquica para mejorar la búsqueda semántica
//! - **Tool System**: Sistema extensible de herramientas para análisis, refactoring, git, etc.
//!
//! # Módulos Principales
//!
//! - [`agent`] - Orquestación de modelos y routing inteligente
//! - [`tools`] - Herramientas para análisis, búsqueda, refactoring, etc.
//! - [`raptor`] - Sistema RAPTOR para RAG mejorado
//! - [`ui`] - Interfaz TUI moderna con ratatui
//! - [`db`] - Persistencia de sesiones e índices
//!
//! # Ejemplo de Uso
//!
//! ```rust,no_run
//! use neuro::agent::orchestrator::{DualModelOrchestrator, OrchestratorConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = OrchestratorConfig {
//!     ollama_url: "http://localhost:11434".to_string(),
//!     fast_model: "qwen3:8b".to_string(),
//!     heavy_model: "qwen3:8b".to_string(),
//!     heavy_timeout_secs: 300,
//!     max_concurrent_heavy: 2,
//! };
//!
//! let orchestrator = DualModelOrchestrator::new(config).await?;
//! let response = orchestrator.process("analiza este proyecto").await?;
//! # Ok(())
//! # }
//! ```

pub mod agent;
pub mod ast;
pub mod config;
pub mod context;
pub mod db;
pub mod embedding;
pub mod i18n;
pub mod logging;
pub mod mcp;
pub mod search;
pub mod security;
pub mod tools;
pub mod ui;

// RAPTOR recursive summarization & retriever
pub mod raptor;

pub use agent::orchestrator::DualModelOrchestrator;
pub use context::ContextManager;
pub use db::Database;
pub use i18n::{current_locale, init_locale, t, Locale, Text};
pub use mcp::NeuroMcpServer;
pub use raptor::retriever::TreeRetriever;
pub use raptor::summarizer::SummaryNode;
pub use security::CommandScanner;
pub use ui::ModernApp;
