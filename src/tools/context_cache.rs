//! Context cache tool - Manages persistent project context cache

use crate::db::{CodeDependency, CodeSymbol, Database, IndexedFile, Project};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Context cache tool
pub struct ContextCacheTool {
    db: Arc<Database>,
}

impl ContextCacheTool {
    pub const NAME: &'static str = "context_cache";

    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Check if there's valid cache for a project
    pub async fn has_valid_cache(&self, root_path: &str) -> Result<bool, CacheError> {
        let project = self.db.get_project_by_path(root_path).await?;
        Ok(project.is_some())
    }

    /// Get cached project context
    pub async fn get_cached_context(
        &self,
        root_path: &str,
    ) -> Result<Option<CachedProjectContext>, CacheError> {
        let project = match self.db.get_project_by_path(root_path).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        let files = self.db.get_project_files(&project.id).await?;
        let symbols = self.db.get_all_symbols(&project.id).await?;
        let dependencies = self.db.get_project_dependencies(&project.id).await?;

        // Compute metrics
        let metrics = self.compute_metrics(&files, &symbols);

        // Generate pre-formatted summaries
        let summary = self.generate_summary(&project, &files, &symbols);
        let insights = self.generate_insights(&project, &files, &symbols, &metrics);
        let recommendations = self.generate_recommendations(&files, &symbols, &metrics);

        Ok(Some(CachedProjectContext {
            project,
            files,
            symbols,
            dependencies,
            summary,
            insights,
            recommendations,
            metrics,
        }))
    }

    /// Cache project context
    pub async fn cache_project_context(
        &self,
        root_path: &str,
        name: &str,
        language: &str,
        files: Vec<IndexedFile>,
        symbols: Vec<CodeSymbol>,
        dependencies: Vec<CodeDependency>,
    ) -> Result<(), CacheError> {
        // Create or update project
        let project = Project::new(root_path, name, language);
        self.db.upsert_project(&project).await?;

        // Insert files
        for file in files {
            self.db.upsert_indexed_file(&file).await?;
        }

        // Insert symbols
        for symbol in symbols {
            self.db.insert_code_symbol(&symbol).await?;
        }

        // Insert dependencies
        for dep in dependencies {
            self.db.insert_dependency(&dep).await?;
        }

        Ok(())
    }

    /// Invalidate cache for a project
    pub async fn invalidate_cache(&self, root_path: &str) -> Result<(), CacheError> {
        self.db.clear_project_cache(root_path).await?;
        Ok(())
    }

    /// Search for symbols in cache
    pub async fn search_symbols(
        &self,
        root_path: &str,
        query: &str,
    ) -> Result<Vec<CodeSymbol>, CacheError> {
        let project = match self.db.get_project_by_path(root_path).await? {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        Ok(self.db.search_symbols(&project.id, query, 100).await?)
    }

    /// Get executive summary (pre-formatted for AI)
    pub async fn get_executive_summary(&self, root_path: &str) -> Result<String, CacheError> {
        let context = match self.get_cached_context(root_path).await? {
            Some(c) => c,
            None => return Ok("No cached context available for this project.".to_string()),
        };

        Ok(context.summary)
    }

    /// Get quick insights (pre-formatted for AI)
    pub async fn get_quick_insights(&self, root_path: &str) -> Result<Vec<String>, CacheError> {
        let context = match self.get_cached_context(root_path).await? {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        Ok(context.insights)
    }

    // ========================================================================
    // INTERNAL HELPERS - Generate pre-formatted content
    // ========================================================================

    /// Generate executive summary
    fn generate_summary(
        &self,
        project: &Project,
        files: &[IndexedFile],
        symbols: &[CodeSymbol],
    ) -> String {
        let total_lines: i64 = files.iter().filter_map(|f| f.line_count).sum();
        let public_symbols = symbols.iter().filter(|s| s.is_public()).count();

        format!(
            r#"Este es un proyecto {} ({} líneas) con {} archivos y {} símbolos públicos.
Lenguaje principal: {}
Última indexación: {}

El proyecto está bien estructurado con una organización clara de archivos."#,
            project.name,
            format_number(total_lines),
            files.len(),
            public_symbols,
            project.language,
            &project.last_indexed_at[..10] // Solo fecha
        )
    }

    /// Generate insights list
    fn generate_insights(
        &self,
        project: &Project,
        files: &[IndexedFile],
        symbols: &[CodeSymbol],
        metrics: &ProjectMetrics,
    ) -> Vec<String> {
        let mut insights = Vec::new();

        // Language distribution
        let total_lines: i64 = files.iter().filter_map(|f| f.line_count).sum();
        if total_lines > 0 {
            let main_lang_lines: i64 = files
                .iter()
                .filter(|f| {
                    f.language
                        .as_ref()
                        .map(|l| l == &project.language)
                        .unwrap_or(false)
                })
                .filter_map(|f| f.line_count)
                .sum();
            let percentage = (main_lang_lines * 100) / total_lines;
            insights.push(format!(
                "{}% del código está en {}",
                percentage, project.language
            ));
        }

        // Complexity insights
        let complex_functions = symbols.iter().filter(|s| s.is_complex()).count();
        if complex_functions > 0 {
            insights.push(format!(
                "{} funciones con complejidad alta detectadas",
                complex_functions
            ));
        } else {
            insights.push("Complejidad general del código es manejable".to_string());
        }

        // Documentation coverage
        let documented = symbols.iter().filter(|s| s.documentation.is_some()).count();
        if !symbols.is_empty() {
            let doc_percentage = (documented * 100) / symbols.len();
            insights.push(format!("{}% de símbolos documentados", doc_percentage));
        }

        // Test coverage estimate
        let test_symbols = symbols.iter().filter(|s| s.is_test == 1).count();
        if test_symbols > 0 {
            insights.push(format!("{} tests detectados", test_symbols));
        }

        // Async usage
        let async_functions = symbols.iter().filter(|s| s.is_async == 1).count();
        if async_functions > 0 {
            insights.push(format!(
                "{} funciones asíncronas (uso de async/await)",
                async_functions
            ));
        }

        // Metrics insights
        if metrics.maintainability_index >= 70.0 {
            insights.push("Índice de mantenibilidad: Alto (código fácil de mantener)".to_string());
        } else if metrics.maintainability_index >= 50.0 {
            insights.push("Índice de mantenibilidad: Medio".to_string());
        } else {
            insights.push("Índice de mantenibilidad: Bajo (considerar refactoring)".to_string());
        }

        insights
    }

    /// Generate recommendations
    fn generate_recommendations(
        &self,
        files: &[IndexedFile],
        symbols: &[CodeSymbol],
        metrics: &ProjectMetrics,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Documentation recommendations
        let undocumented_public = symbols
            .iter()
            .filter(|s| s.is_public() && s.documentation.is_none())
            .count();
        if undocumented_public > 0 {
            recommendations.push(format!(
                "Agregar documentación a {} símbolos públicos sin documentar",
                undocumented_public
            ));
        }

        // Complexity recommendations
        let very_complex = symbols.iter().filter(|s| s.complexity > 15).count();
        if very_complex > 0 {
            recommendations.push(format!(
                "Refactorizar {} funciones con complejidad muy alta (>15)",
                very_complex
            ));
        }

        // Large files
        let large_files = files.iter().filter(|f| f.file_size > 10000).count();
        if large_files > 5 {
            recommendations.push(format!(
                "Considerar dividir {} archivos grandes (>10KB)",
                large_files
            ));
        }

        // Maintainability
        if metrics.maintainability_index < 50.0 {
            recommendations
                .push("Mejorar índice de mantenibilidad mediante refactoring".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("El proyecto está en buen estado general".to_string());
        }

        recommendations
    }

    /// Compute project metrics
    fn compute_metrics(&self, files: &[IndexedFile], symbols: &[CodeSymbol]) -> ProjectMetrics {
        // Complexity score (0-100, lower is better)
        let avg_complexity = if !symbols.is_empty() {
            let total: i64 = symbols.iter().map(|s| s.complexity).sum();
            total as f64 / symbols.len() as f64
        } else {
            1.0
        };
        let complexity_score = ((avg_complexity - 1.0) * 10.0).clamp(0.0, 100.0);

        // Maintainability index (0-100, higher is better)
        // Simplified formula based on code size and complexity
        let total_lines: i64 = files.iter().filter_map(|f| f.line_count).sum();
        let lines_penalty = (total_lines as f64 / 1000.0).min(30.0);
        let complexity_penalty = complexity_score / 2.0;
        let maintainability_index = (100.0 - lines_penalty - complexity_penalty).max(0.0);

        // Documentation coverage (0-100)
        let documentation_coverage = if !symbols.is_empty() {
            let documented = symbols.iter().filter(|s| s.documentation.is_some()).count();
            (documented as f64 / symbols.len() as f64) * 100.0
        } else {
            0.0
        };

        // Test coverage estimate (based on test functions found)
        let test_coverage_estimate = {
            let test_count = symbols.iter().filter(|s| s.is_test == 1).count();
            let total_functions = symbols
                .iter()
                .filter(|s| s.symbol_type == "function")
                .count();
            if total_functions > 0 {
                Some((test_count as f64 / total_functions as f64) * 100.0)
            } else {
                None
            }
        };

        ProjectMetrics {
            complexity_score,
            maintainability_index,
            documentation_coverage,
            test_coverage_estimate,
        }
    }
}

/// Cached project context with pre-computed insights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedProjectContext {
    pub project: Project,
    pub files: Vec<IndexedFile>,
    pub symbols: Vec<CodeSymbol>,
    pub dependencies: Vec<CodeDependency>,

    // Pre-formatted content for AI
    pub summary: String,
    pub insights: Vec<String>,
    pub recommendations: Vec<String>,
    pub metrics: ProjectMetrics,
}

/// Project metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetrics {
    /// Complexity score (0-100, lower is better)
    pub complexity_score: f64,
    /// Maintainability index (0-100, higher is better)
    pub maintainability_index: f64,
    /// Documentation coverage (0-100)
    pub documentation_coverage: f64,
    /// Estimated test coverage (0-100, if available)
    pub test_coverage_estimate: Option<f64>,
}

/// Cache errors
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] crate::db::DatabaseError),
    #[error("Project not found: {0}")]
    ProjectNotFound(String),
    #[error("Cache error: {0}")]
    CacheError(String),
}

// Utility functions
fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
