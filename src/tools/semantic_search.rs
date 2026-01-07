//! Semantic Search Tool
//!
//! Enables semantic code search using embeddings for the AI agent.

use crate::db::Database;
use crate::embedding::EmbeddingEngine;
use crate::search::SemanticSearch;
use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Semantic search tool for AI agent
pub struct SemanticSearchTool {
    searcher: Arc<SemanticSearch>,
}

impl SemanticSearchTool {
    pub const NAME: &'static str = "semantic_search";

    /// Create a new semantic search tool
    pub fn new(db: Arc<Database>, embedder: Arc<EmbeddingEngine>) -> Self {
        let searcher = Arc::new(SemanticSearch::new(db, embedder));
        Self { searcher }
    }

    /// Get the searcher (for testing or direct use)
    pub fn searcher(&self) -> &Arc<SemanticSearch> {
        &self.searcher
    }
}

/// Arguments for semantic search
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchArgs {
    /// Search query describing what you're looking for
    /// Example: "function that handles HTTP requests"
    pub query: String,

    /// Project ID to search in
    #[schemars(description = "Project identifier (use project root path)")]
    pub project_id: String,

    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_limit")]
    #[schemars(description = "Maximum results to return (1-20)")]
    pub limit: Option<usize>,

    /// Filter by programming language
    #[schemars(description = "Optional language filter (e.g., 'rust', 'python')")]
    pub language: Option<String>,
}

fn default_limit() -> Option<usize> {
    Some(5)
}

/// Search result output
#[derive(Debug, Serialize)]
pub struct SemanticSearchOutput {
    pub results: Vec<SearchResultFormatted>,
    pub total_found: usize,
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResultFormatted {
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub chunk_type: String,
    pub line_range: String,
    pub score: f32,
    pub summary: String,
    pub content_preview: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SemanticSearchError {
    #[error("Search failed: {0}")]
    SearchFailed(String),

    #[error("Invalid limit: must be between 1 and 20")]
    InvalidLimit,

    #[error("Project not found: {0}")]
    ProjectNotFound(String),
}

impl Tool for SemanticSearchTool {
    const NAME: &'static str = Self::NAME;

    type Args = SemanticSearchArgs;
    type Output = String;
    type Error = SemanticSearchError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Busca código semánticamente usando embeddings. Útil cuando no conoces la ruta exacta de un archivo pero sabes qué tipo de código necesitas. Ejemplo: 'función que maneja autenticación', 'struct para configuración', 'código que parsea JSON'".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Descripción de lo que buscas (en lenguaje natural)"
                    },
                    "project_id": {
                        "type": "string",
                        "description": "ID del proyecto (ruta raíz del proyecto)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Número máximo de resultados (default: 5, max: 20)",
                        "minimum": 1,
                        "maximum": 20
                    },
                    "language": {
                        "type": "string",
                        "description": "Filtrar por lenguaje (opcional): 'rust', 'python', 'typescript', 'javascript'"
                    }
                },
                "required": ["query", "project_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Validate limit
        let limit = args.limit.unwrap_or(5).clamp(1, 20);

        // Clone Arc for the spawned task
        let searcher = self.searcher.clone();
        let query = args.query.clone();
        let project_id = args.project_id.clone();

        // Perform search in a spawned task to avoid Sync issues
        let results =
            tokio::task::spawn(async move { searcher.search(&query, &project_id, limit).await })
                .await
                .map_err(|e| SemanticSearchError::SearchFailed(format!("Task join error: {}", e)))?
                .map_err(|e| SemanticSearchError::SearchFailed(e.to_string()))?;

        // Format results for LLM
        if results.is_empty() {
            return Ok(format!(
                "No se encontraron resultados para la búsqueda: \"{}\"\n\n\
                 Sugerencias:\n\
                 - Intenta usar términos más generales\n\
                 - Verifica que el proyecto esté indexado\n\
                 - Prueba con sinónimos o descripciones alternativas",
                args.query
            ));
        }

        let mut output = format!(
            "Encontré {} resultado(s) para: \"{}\"\n\n",
            results.len(),
            args.query
        );

        for (i, result) in results.iter().enumerate() {
            let symbol_display = result
                .symbol_name
                .as_ref()
                .map(|s| format!(" `{}`", s))
                .unwrap_or_default();

            output.push_str(&format!(
                "{}. **{}{}** en `{}` (líneas {}-{})\n",
                i + 1,
                result.chunk_type,
                symbol_display,
                result.file_path,
                result.line_range.0,
                result.line_range.1
            ));

            output.push_str(&format!("   📊 Score: {:.2}\n", result.score));
            output.push_str(&format!("   📝 {}\n\n", result.summary));

            // Add code preview for top results
            if i < 2 {
                let preview = if result.content.len() > 300 {
                    format!("{}...", &result.content[..300])
                } else {
                    result.content.clone()
                };

                output.push_str(&format!(
                    "   ```{}\n   {}\n   ```\n\n",
                    result.language, preview
                ));
            }
        }

        // Add usage tips
        if results.len() == limit {
            output.push_str(
                "\n💡 Hay más resultados disponibles. Aumenta el `limit` si necesitas ver más.\n",
            );
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_serialization() {
        let args = SemanticSearchArgs {
            query: "HTTP handler".to_string(),
            project_id: "/path/to/project".to_string(),
            limit: Some(10),
            language: Some("rust".to_string()),
        };

        let json = serde_json::to_string(&args).unwrap();
        assert!(json.contains("HTTP handler"));
    }

    #[test]
    fn test_default_limit() {
        let args: SemanticSearchArgs =
            serde_json::from_str(r#"{"query": "test", "project_id": "/path"}"#).unwrap();

        assert_eq!(args.limit, Some(5));
    }
}
