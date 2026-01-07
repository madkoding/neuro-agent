//! Database repository for CRUD operations

use super::migrations::INIT_SCHEMA;
use super::models::{
    CodeDependency, CodeSymbol, CommandExecution, DbMessage, IndexedFile, Project,
    ProjectAnalysisRecord, SecurityConfig, Session,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Query error: {0}")]
    QueryError(String),
}

/// Database connection and operations
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(path: &Path) -> Result<Self, DatabaseError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    /// Create an in-memory database (for testing)
    pub async fn in_memory() -> Result<Self, DatabaseError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<(), DatabaseError> {
        sqlx::query(INIT_SCHEMA)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::Migration(e.to_string()))?;

        Ok(())
    }

    // ========================================================================
    // Session operations
    // ========================================================================

    /// Create a new session
    pub async fn create_session(&self, session: &Session) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO sessions (id, created_at, updated_at, title, fast_model, heavy_model, total_tokens, working_dir)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.created_at)
        .bind(&session.updated_at)
        .bind(&session.title)
        .bind(&session.fast_model)
        .bind(&session.heavy_model)
        .bind(session.total_tokens)
        .bind(&session.working_dir)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a session by ID
    pub async fn get_session(&self, id: &str) -> Result<Session, DatabaseError> {
        sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| DatabaseError::NotFound(format!("Session not found: {}", id)))
    }

    /// Get recent sessions
    pub async fn get_recent_sessions(&self, limit: i32) -> Result<Vec<Session>, DatabaseError> {
        Ok(
            sqlx::query_as::<_, Session>("SELECT * FROM sessions ORDER BY updated_at DESC LIMIT ?")
                .bind(limit)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Update session tokens
    pub async fn update_session_tokens(
        &self,
        session_id: &str,
        tokens: i64,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE sessions 
            SET total_tokens = total_tokens + ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(tokens)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a session and all related data
    pub async fn delete_session(&self, session_id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Message operations
    // ========================================================================

    /// Create a new message
    pub async fn create_message(&self, message: &DbMessage) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO messages (id, session_id, role, content, created_at, model, tool_name, tokens)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(&message.created_at)
        .bind(&message.model)
        .bind(&message.tool_name)
        .bind(message.tokens)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get messages for a session
    pub async fn get_session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<DbMessage>, DatabaseError> {
        Ok(sqlx::query_as::<_, DbMessage>(
            "SELECT * FROM messages WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Get recent messages for a session
    pub async fn get_recent_messages(
        &self,
        session_id: &str,
        limit: i32,
    ) -> Result<Vec<DbMessage>, DatabaseError> {
        Ok(sqlx::query_as::<_, DbMessage>(
            r#"
            SELECT * FROM (
                SELECT * FROM messages 
                WHERE session_id = ? 
                ORDER BY created_at DESC 
                LIMIT ?
            ) ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    // ========================================================================
    // Command execution operations
    // ========================================================================

    /// Create a command execution record
    pub async fn create_command_execution(
        &self,
        execution: &CommandExecution,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO command_executions 
            (id, session_id, message_id, command, exit_code, stdout, stderr, 
             executed_at, retry_count, was_dangerous, risk_level, confirmed_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&execution.id)
        .bind(&execution.session_id)
        .bind(&execution.message_id)
        .bind(&execution.command)
        .bind(execution.exit_code)
        .bind(&execution.stdout)
        .bind(&execution.stderr)
        .bind(&execution.executed_at)
        .bind(execution.retry_count)
        .bind(execution.was_dangerous)
        .bind(&execution.risk_level)
        .bind(&execution.confirmed_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get command executions for a session
    pub async fn get_session_commands(
        &self,
        session_id: &str,
    ) -> Result<Vec<CommandExecution>, DatabaseError> {
        Ok(sqlx::query_as::<_, CommandExecution>(
            "SELECT * FROM command_executions WHERE session_id = ? ORDER BY executed_at DESC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Update retry count for a command
    pub async fn update_command_retry(
        &self,
        id: &str,
        retry_count: i32,
    ) -> Result<(), DatabaseError> {
        sqlx::query("UPDATE command_executions SET retry_count = ? WHERE id = ?")
            .bind(retry_count)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ========================================================================
    // Security configuration operations
    // ========================================================================

    /// Get or create security configuration
    pub async fn get_security_config(&self) -> Result<SecurityConfig, DatabaseError> {
        let config =
            sqlx::query_as::<_, SecurityConfig>("SELECT * FROM security_config WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;

        match config {
            Some(c) => Ok(c),
            None => {
                let default = SecurityConfig::default();
                self.save_security_config(&default).await?;
                Ok(default)
            }
        }
    }

    /// Save security configuration
    pub async fn save_security_config(&self, config: &SecurityConfig) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO security_config 
            (id, password_hash, require_password_for_dangerous, created_at, updated_at)
            VALUES (1, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&config.password_hash)
        .bind(config.require_password_for_dangerous)
        .bind(&config.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update password hash
    pub async fn update_password(&self, password_hash: &str) -> Result<(), DatabaseError> {
        sqlx::query(
            "UPDATE security_config SET password_hash = ?, updated_at = datetime('now') WHERE id = 1",
        )
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ========================================================================
    // PROJECT CACHE OPERATIONS
    // ========================================================================

    /// Create or update a project
    pub async fn upsert_project(&self, project: &Project) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO projects (id, root_path, name, language, project_type, description,
                                 version, last_indexed_at, last_modified_at, config_hash, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                language = excluded.language,
                project_type = excluded.project_type,
                description = excluded.description,
                version = excluded.version,
                last_indexed_at = excluded.last_indexed_at,
                last_modified_at = excluded.last_modified_at,
                config_hash = excluded.config_hash
            "#,
        )
        .bind(&project.id)
        .bind(&project.root_path)
        .bind(&project.name)
        .bind(&project.language)
        .bind(&project.project_type)
        .bind(&project.description)
        .bind(&project.version)
        .bind(&project.last_indexed_at)
        .bind(&project.last_modified_at)
        .bind(&project.config_hash)
        .bind(&project.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get project by root path
    pub async fn get_project_by_path(
        &self,
        root_path: &str,
    ) -> Result<Option<Project>, DatabaseError> {
        let id = Project::compute_id(root_path);
        Ok(
            sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
                .bind(&id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Upsert indexed file
    pub async fn upsert_indexed_file(&self, file: &IndexedFile) -> Result<i64, DatabaseError> {
        // First try to get existing ID
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM indexed_files WHERE project_id = ? AND relative_path = ?",
        )
        .bind(&file.project_id)
        .bind(&file.relative_path)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((existing_id,)) = existing {
            // Update existing
            sqlx::query(
                r#"
                UPDATE indexed_files SET
                    absolute_path = ?,
                    file_hash = ?,
                    file_size = ?,
                    line_count = ?,
                    language = ?,
                    file_type = ?,
                    last_modified = ?,
                    indexed_at = ?,
                    is_valid = ?
                WHERE id = ?
                "#,
            )
            .bind(&file.absolute_path)
            .bind(&file.file_hash)
            .bind(file.file_size)
            .bind(file.line_count)
            .bind(&file.language)
            .bind(&file.file_type)
            .bind(&file.last_modified)
            .bind(&file.indexed_at)
            .bind(file.is_valid)
            .bind(existing_id)
            .execute(&self.pool)
            .await?;

            Ok(existing_id)
        } else {
            // Insert new
            sqlx::query(
                r#"
                INSERT INTO indexed_files
                (project_id, relative_path, absolute_path, file_hash, file_size, line_count,
                 language, file_type, last_modified, indexed_at, is_valid)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&file.project_id)
            .bind(&file.relative_path)
            .bind(&file.absolute_path)
            .bind(&file.file_hash)
            .bind(file.file_size)
            .bind(file.line_count)
            .bind(&file.language)
            .bind(&file.file_type)
            .bind(&file.last_modified)
            .bind(&file.indexed_at)
            .bind(file.is_valid)
            .execute(&self.pool)
            .await?;

            // Get last insert ID
            let id: (i64,) = sqlx::query_as("SELECT last_insert_rowid()")
                .fetch_one(&self.pool)
                .await?;

            Ok(id.0)
        }
    }

    /// Get files for project
    pub async fn get_project_files(
        &self,
        project_id: &str,
    ) -> Result<Vec<IndexedFile>, DatabaseError> {
        Ok(sqlx::query_as::<_, IndexedFile>(
            "SELECT * FROM indexed_files WHERE project_id = ? AND is_valid = 1 ORDER BY relative_path"
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Insert code symbol
    pub async fn insert_code_symbol(&self, symbol: &CodeSymbol) -> Result<i64, DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO code_symbols
            (file_id, project_id, symbol_name, symbol_type, visibility, line_start, line_end,
             signature, documentation, complexity, params_json, return_type, is_async, is_test, parent_symbol_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(symbol.file_id)
        .bind(&symbol.project_id)
        .bind(&symbol.symbol_name)
        .bind(&symbol.symbol_type)
        .bind(&symbol.visibility)
        .bind(symbol.line_start)
        .bind(symbol.line_end)
        .bind(&symbol.signature)
        .bind(&symbol.documentation)
        .bind(symbol.complexity)
        .bind(&symbol.params_json)
        .bind(&symbol.return_type)
        .bind(symbol.is_async)
        .bind(symbol.is_test)
        .bind(symbol.parent_symbol_id)
        .execute(&self.pool)
        .await?;

        let id: (i64,) = sqlx::query_as("SELECT last_insert_rowid()")
            .fetch_one(&self.pool)
            .await?;

        Ok(id.0)
    }

    /// Get symbols for file
    pub async fn get_file_symbols(&self, file_id: i64) -> Result<Vec<CodeSymbol>, DatabaseError> {
        Ok(sqlx::query_as::<_, CodeSymbol>(
            "SELECT * FROM code_symbols WHERE file_id = ? ORDER BY line_start",
        )
        .bind(file_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Get all symbols for a project
    pub async fn get_all_symbols(
        &self,
        project_id: &str,
    ) -> Result<Vec<CodeSymbol>, DatabaseError> {
        Ok(sqlx::query_as::<_, CodeSymbol>(
            "SELECT * FROM code_symbols WHERE project_id = ? ORDER BY symbol_name",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Search symbols by name
    pub async fn search_symbols(
        &self,
        project_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<CodeSymbol>, DatabaseError> {
        let search_pattern = format!("%{}%", query.to_lowercase());
        Ok(sqlx::query_as::<_, CodeSymbol>(
            r#"
            SELECT DISTINCT cs.* FROM code_symbols cs
            JOIN search_index si ON si.entity_type = 'symbol' AND si.entity_id = cs.id
            WHERE cs.project_id = ? AND si.search_text LIKE ?
            ORDER BY cs.symbol_name
            LIMIT ?
            "#,
        )
        .bind(project_id)
        .bind(&search_pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Insert dependency
    pub async fn insert_dependency(&self, dep: &CodeDependency) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO code_dependencies
            (project_id, source_file_id, target_module, import_type, is_external, line_number)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&dep.project_id)
        .bind(dep.source_file_id)
        .bind(&dep.target_module)
        .bind(&dep.import_type)
        .bind(dep.is_external)
        .bind(dep.line_number)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get project dependencies
    pub async fn get_project_dependencies(
        &self,
        project_id: &str,
    ) -> Result<Vec<CodeDependency>, DatabaseError> {
        Ok(sqlx::query_as::<_, CodeDependency>(
            "SELECT * FROM code_dependencies WHERE project_id = ? ORDER BY target_module",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Upsert project analysis
    pub async fn upsert_project_analysis(
        &self,
        analysis: &ProjectAnalysisRecord,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO project_analysis
            (project_id, analysis_type, analyzed_at, results_json, summary, score)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(project_id, analysis_type) DO UPDATE SET
                analyzed_at = excluded.analyzed_at,
                results_json = excluded.results_json,
                summary = excluded.summary,
                score = excluded.score
            "#,
        )
        .bind(&analysis.project_id)
        .bind(&analysis.analysis_type)
        .bind(&analysis.analyzed_at)
        .bind(&analysis.results_json)
        .bind(&analysis.summary)
        .bind(analysis.score)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get project analysis
    pub async fn get_project_analysis(
        &self,
        project_id: &str,
        analysis_type: &str,
    ) -> Result<Option<ProjectAnalysisRecord>, DatabaseError> {
        Ok(sqlx::query_as::<_, ProjectAnalysisRecord>(
            "SELECT * FROM project_analysis WHERE project_id = ? AND analysis_type = ?",
        )
        .bind(project_id)
        .bind(analysis_type)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Add search index entry
    pub async fn add_search_index(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: i64,
        text: &str,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT OR IGNORE INTO search_index (project_id, entity_type, entity_id, search_text) VALUES (?, ?, ?, ?)"
        )
        .bind(project_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(text.to_lowercase())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Clear project cache
    pub async fn clear_project_cache(&self, project_id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(project_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Close the database connection
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let db = Database::in_memory().await.unwrap();

        let session = Session::new("test-id", "qwen3:0.6b", "qwen3:8b");
        db.create_session(&session).await.unwrap();

        let retrieved = db.get_session("test-id").await.unwrap();
        assert_eq!(retrieved.id, "test-id");
        assert_eq!(retrieved.fast_model, "qwen3:0.6b");
    }

    #[tokio::test]
    async fn test_create_and_get_messages() {
        let db = Database::in_memory().await.unwrap();

        let session = Session::new("test-session", "qwen3:0.6b", "qwen3:8b");
        db.create_session(&session).await.unwrap();

        let message = DbMessage::new("msg-1", "test-session", "user", "Hello!");
        db.create_message(&message).await.unwrap();

        let messages = db.get_session_messages("test-session").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");
    }

    #[tokio::test]
    async fn test_security_config() {
        let db = Database::in_memory().await.unwrap();

        let config = db.get_security_config().await.unwrap();
        assert!(!config.has_password());
        assert!(config.requires_password());
    }
}
