//! Database models

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Session record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub title: Option<String>,
    pub fast_model: String,
    pub heavy_model: String,
    pub total_tokens: i64,
    pub working_dir: Option<String>,
}

impl Session {
    pub fn new(
        id: impl Into<String>,
        fast_model: impl Into<String>,
        heavy_model: impl Into<String>,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: id.into(),
            created_at: now.clone(),
            updated_at: now,
            title: None,
            fast_model: fast_model.into(),
            heavy_model: heavy_model.into(),
            total_tokens: 0,
            working_dir: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// Message record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DbMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub model: Option<String>,
    pub tool_name: Option<String>,
    pub tokens: i64,
}

impl DbMessage {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            role: role.into(),
            content: content.into(),
            created_at: Utc::now().to_rfc3339(),
            model: None,
            tool_name: None,
            tokens: 0,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    pub fn with_tokens(mut self, tokens: i64) -> Self {
        self.tokens = tokens;
        self
    }
}

/// Command execution record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommandExecution {
    pub id: String,
    pub session_id: String,
    pub message_id: Option<String>,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub executed_at: String,
    pub retry_count: i32,
    pub was_dangerous: i32,
    pub risk_level: Option<String>,
    pub confirmed_by: Option<String>,
}

impl CommandExecution {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            message_id: None,
            command: command.into(),
            exit_code: None,
            stdout: None,
            stderr: None,
            executed_at: Utc::now().to_rfc3339(),
            retry_count: 0,
            was_dangerous: 0,
            risk_level: None,
            confirmed_by: None,
        }
    }

    pub fn with_result(mut self, exit_code: i32, stdout: String, stderr: String) -> Self {
        self.exit_code = Some(exit_code);
        self.stdout = Some(stdout);
        self.stderr = Some(stderr);
        self
    }

    pub fn with_message(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    pub fn mark_dangerous(mut self, risk_level: impl Into<String>) -> Self {
        self.was_dangerous = 1;
        self.risk_level = Some(risk_level.into());
        self
    }

    pub fn with_confirmation(mut self, confirmed_by: impl Into<String>) -> Self {
        self.confirmed_by = Some(confirmed_by.into());
        self
    }
}

/// Security configuration record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SecurityConfig {
    pub id: i32,
    pub password_hash: Option<String>,
    pub require_password_for_dangerous: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 1,
            password_hash: None,
            require_password_for_dangerous: 1,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl SecurityConfig {
    pub fn has_password(&self) -> bool {
        self.password_hash.is_some()
    }

    pub fn requires_password(&self) -> bool {
        self.require_password_for_dangerous == 1
    }
}

// ========================================================================
// PROJECT CONTEXT CACHE MODELS
// ========================================================================

use sha2::{Digest, Sha256};

/// Project metadata record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub root_path: String,
    pub name: String,
    pub language: String,
    pub project_type: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub last_indexed_at: String,
    pub last_modified_at: String,
    pub config_hash: String,
    pub created_at: String,
}

impl Project {
    pub fn new(
        root_path: impl Into<String>,
        name: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        let root_path = root_path.into();
        let id = Self::compute_id(&root_path);
        let now = Utc::now().to_rfc3339();

        Self {
            id,
            root_path,
            name: name.into(),
            language: language.into(),
            project_type: None,
            description: None,
            version: None,
            last_indexed_at: now.clone(),
            last_modified_at: now.clone(),
            config_hash: String::new(),
            created_at: now,
        }
    }

    pub fn compute_id(root_path: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(root_path.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Indexed file record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IndexedFile {
    pub id: i64,
    pub project_id: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub file_hash: String,
    pub file_size: i64,
    pub line_count: Option<i64>,
    pub language: Option<String>,
    pub file_type: Option<String>,
    pub last_modified: String,
    pub indexed_at: String,
    pub is_valid: i32,
}

impl IndexedFile {
    pub fn is_outdated(&self, current_hash: &str) -> bool {
        self.file_hash != current_hash
    }
}

/// Code symbol record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodeSymbol {
    pub id: i64,
    pub file_id: i64,
    pub project_id: String,
    pub symbol_name: String,
    pub symbol_type: String,
    pub visibility: String,
    pub line_start: i64,
    pub line_end: i64,
    pub signature: Option<String>,
    pub documentation: Option<String>,
    pub complexity: i64,
    pub params_json: Option<String>,
    pub return_type: Option<String>,
    pub is_async: i32,
    pub is_test: i32,
    pub parent_symbol_id: Option<i64>,
}

impl CodeSymbol {
    pub fn params(&self) -> Vec<String> {
        self.params_json
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    pub fn is_public(&self) -> bool {
        self.visibility == "public"
    }

    pub fn is_complex(&self) -> bool {
        self.complexity > 10
    }
}

/// Code dependency record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodeDependency {
    pub id: i64,
    pub project_id: String,
    pub source_file_id: i64,
    pub target_module: String,
    pub import_type: String,
    pub is_external: i32,
    pub line_number: i64,
}

/// Code relationship record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodeRelationship {
    pub id: i64,
    pub project_id: String,
    pub source_symbol_id: i64,
    pub target_symbol_id: Option<i64>,
    pub target_name: Option<String>,
    pub relationship_type: String,
    pub confidence: f64,
}

/// Project analysis record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectAnalysisRecord {
    pub id: i64,
    pub project_id: String,
    pub analysis_type: String,
    pub analyzed_at: String,
    pub results_json: String,
    pub summary: Option<String>,
    pub score: Option<f64>,
}

impl ProjectAnalysisRecord {
    pub fn results<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.results_json)
    }
}

/// Documentation cache record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentationCache {
    pub id: i64,
    pub project_id: String,
    pub scope: String,
    pub scope_identifier: String,
    pub format: String,
    pub content: String,
    pub generated_at: String,
    pub content_hash: String,
}

/// Search index record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SearchIndexEntry {
    pub id: i64,
    pub project_id: String,
    pub entity_type: String,
    pub entity_id: i64,
    pub search_text: String,
}

/// Model configuration record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelConfigRow {
    pub id: i64,
    pub config_type: String, // "fast" or "heavy"
    pub provider: String,
    pub url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl ModelConfigRow {
    pub fn new(config_type: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 0,
            config_type: config_type.into(),
            provider: "ollama".to_string(),
            url: "http://localhost:11434".to_string(),
            model: "qwen3:8b".to_string(),
            api_key: None,
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Tool configuration record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ToolConfigRow {
    pub id: i64,
    pub tool_id: String,
    pub enabled: i64, // SQLite uses INTEGER for boolean
    pub created_at: String,
    pub updated_at: String,
}

impl ToolConfigRow {
    pub fn new(tool_id: impl Into<String>, enabled: bool) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 0,
            tool_id: tool_id.into(),
            enabled: if enabled { 1 } else { 0 },
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled != 0
    }
}

/// Application configuration record (general settings)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppConfigRow {
    pub id: i64,
    pub heavy_timeout_secs: i64,
    pub max_concurrent_heavy: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl AppConfigRow {
    pub fn new() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 1, // Singleton
            heavy_timeout_secs: 1200,
            max_concurrent_heavy: 2,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl Default for AppConfigRow {
    fn default() -> Self {
        Self::new()
    }
}
