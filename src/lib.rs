//! Neuro - High-performance CLI AI Agent for programmers
//!
//! Uses dual-model architecture:
//! - qwen3:8b for both fast interactions and heavy tasks

pub mod agent;
pub mod ast;
pub mod context;
pub mod db;
pub mod embedding;
pub mod i18n;
pub mod mcp;
pub mod search;
pub mod security;
pub mod tools;
pub mod ui;

// RAPTOR recursive summarization & retriever
pub mod raptor;

pub use raptor::retriever::TreeRetriever;
pub use raptor::summarizer::SummaryNode;
pub use agent::orchestrator::DualModelOrchestrator;
pub use context::ContextManager;
pub use db::Database;
pub use i18n::{current_locale, init_locale, t, Locale, Text};
pub use mcp::NeuroMcpServer;
pub use security::CommandScanner;
pub use ui::ModernApp;
