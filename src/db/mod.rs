//! Database module for SQLite persistence

mod migrations;
mod models;
mod repository;

pub use models::{
    CodeDependency, CodeRelationship, CodeSymbol, CommandExecution, DbMessage, DocumentationCache,
    IndexedFile, Project, ProjectAnalysisRecord, SearchIndexEntry, SecurityConfig, Session,
};
pub use repository::{Database, DatabaseError};
