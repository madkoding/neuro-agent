//! Integración de RAPTOR con el sistema de agentes y orchestrator
//!
//! Este módulo proporciona la integración completa de RAPTOR con:
//! - DualModelOrchestrator
//! - PlanningOrchestrator  
//! - Tool Registry
//! - MCP Servers

#![allow(deprecated)]

use crate::agent::orchestrator::DualModelOrchestrator;
use crate::agent::planning_orchestrator::PlanningOrchestrator;
use crate::embedding::EmbeddingEngine;
use crate::raptor::builder::RaptorBuildProgress;
use crate::raptor::persistence::GLOBAL_STORE;
use crate::raptor::retriever::TreeRetriever;
use crate::tools::{BuildTreeArgs, RaptorTool, RaptorToolCalls};
use crate::log_info;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex as AsyncMutex;

/// Servicio de contexto RAPTOR que se integra con el orchestrator
pub struct RaptorContextService {
    tool: Arc<RaptorTool>,
    embedder: Option<Arc<EmbeddingEngine>>,
}

impl RaptorContextService {
    /// Crear nuevo servicio con orchestrator
    pub fn new(orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>) -> Self {
        Self {
            tool: Arc::new(RaptorTool::new(orchestrator)),
            embedder: None,
        }
    }

    /// Inicializar embedder (lazy loading para ahorrar memoria)
    pub async fn initialize_embedder(&mut self) -> Result<()> {
        if self.embedder.is_none() {
            let embedder = EmbeddingEngine::new().await?;
            self.embedder = Some(Arc::new(embedder));
        }
        Ok(())
    }

    /// Construir árbol RAPTOR desde un directorio
    pub async fn build_tree(
        &mut self,
        path: &str,
        max_chars: Option<usize>,
        threshold: Option<f32>,
    ) -> Result<String> {
        self.initialize_embedder().await?;
        self.tool
            .build_raptor_tree(path, max_chars, threshold)
            .await
    }

    /// Construir árbol RAPTOR desde un directorio con progreso
    pub async fn build_tree_with_progress(
        &mut self,
        path: &str,
        max_chars: Option<usize>,
        threshold: Option<f32>,
        progress_tx: Option<Sender<RaptorBuildProgress>>,
    ) -> Result<String> {
        self.initialize_embedder().await?;
        let args = BuildTreeArgs {
            path: path.to_string(),
            max_chars: max_chars.unwrap_or(500),
            overlap: 50,
            threshold: threshold.unwrap_or(0.7),
        };
        self.tool.build_tree_with_progress(args, progress_tx).await
    }

    /// Consultar el árbol RAPTOR
    pub async fn query(&mut self, query: &str, top_k: Option<usize>) -> Result<String> {
        self.initialize_embedder().await?;
        self.tool.query_raptor_tree(query, top_k).await
    }

    /// Obtener contexto enriquecido para el planning orchestrator
    ///
    /// Este método busca en el árbol RAPTOR y formatea los resultados
    /// de manera que puedan ser usados directamente por el planning orchestrator
    pub async fn get_planning_context(&mut self, task_description: &str) -> Result<String> {
        self.initialize_embedder().await?;

        // Verificar si hay árbol construido
        let has_tree = {
            let store = GLOBAL_STORE.lock().unwrap();
            !store.chunk_map.is_empty()
        };

        if !has_tree {
            let diag = "(No RAPTOR context - no chunks indexed. Run /reindex to build the index)".to_string();
            log_info!("⚠ [RAPTOR] No chunks found for project - returning diagnostic message");
            return Ok(diag);
        }

        // Consultar árbol - clonar store para evitar mantener lock durante await
        let embedder = self.embedder.as_ref().unwrap();
        let store_clone = {
            let store_guard = GLOBAL_STORE.lock().unwrap();
            store_guard.clone()
        }; // Lock liberado aquí

        let retriever = TreeRetriever::new(embedder, &store_clone);
        let top_k = 12usize;
        let expand_k = 24usize;
        let (summaries, chunks) = retriever
            .retrieve_with_context(task_description, top_k, expand_k)
            .await?;

        // Si no hay suficiente contexto, devolver diagnóstico
        if summaries.is_empty() && chunks.is_empty() {
            return Ok("(No relevant RAPTOR context found for this query)".to_string());
        }

        // Construir contexto crudo
        let mut raw_context = String::new();

        if !summaries.is_empty() {
            raw_context.push_str("Resúmenes relevantes:\n");
            for (_, _, summary) in summaries.iter().take(8) {
                raw_context.push_str(&format!("• {}\n", summary));
            }
            raw_context.push('\n');
        }

        if !chunks.is_empty() {
            raw_context.push_str("Fragmentos de código relevantes:\n");
            for (_, _, text) in chunks.iter().take(12) {
                let truncated = text.chars().take(800).collect::<String>();
                raw_context.push_str(&format!("• {}\n", truncated));
            }
        }

        // Si el contexto es muy pequeño, añadir fallback
        if raw_context.chars().count() < 1000 {
            let fallback = Self::build_fallback_context_from_chunks(&store_clone, 5);
            if !fallback.is_empty() {
                raw_context.push_str("\nContexto adicional del proyecto:\n");
                raw_context.push_str(&fallback);
            }
        }

        // Usar LLM para sintetizar el contexto
        if !raw_context.is_empty() {
            let synthesis_prompt = format!(
                "Analiza la siguiente información del proyecto y proporciona un resumen coherente y contextualizado \
                que responda a esta consulta: '{}'\n\n\
                Información del proyecto:\n{}\n\n\
                Proporciona un resumen conciso pero completo que integre toda la información relevante. \
                Si hay código, explica qué hace y cómo se relaciona con la consulta. \
                Mantén el contexto técnico pero hazlo accesible.",
                task_description, raw_context
            );

            // Usar el tool para llamar al LLM
            let orch = self.tool.orchestrator().lock().await;
            match orch.call_fast_model_direct(&synthesis_prompt).await {
                Ok(synthesized) => {
                    log_info!("✓ [RAPTOR] Contexto sintetizado por LLM ({} chars)", synthesized.len());
                    Ok(synthesized)
                }
                Err(e) => {
                    log_info!("⚠ [RAPTOR] Error sintetizando contexto: {}. Devolviendo contexto crudo.", e);
                    Ok(format!("Contexto del proyecto (sin sintetizar):\n\n{}", raw_context))
                }
            }
        } else {
            Ok("(No se pudo generar contexto relevante para esta consulta)".to_string())
        }
    }

    /// Build a simple fallback context by selecting up to `limit` raw chunks from the store.
    pub(crate) fn build_fallback_context_from_chunks(store: &crate::raptor::persistence::TreeStore, limit: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (_id, text) in store.chunk_map.iter().take(limit) {
            // Include a larger excerpt for the comprehensive fallback
            let truncated = text.chars().take(1200).collect::<String>();
            parts.push(truncated);
        }
        parts.join("\n---\n")
    }

    /// Enriquecer respuesta del agente con contexto RAPTOR
    ///
    /// Busca información relevante y la añade a la respuesta
    pub async fn enrich_response(&mut self, query: &str, base_response: &str) -> Result<String> {
        let has_tree = {
            let store = GLOBAL_STORE.lock().unwrap();
            !store.chunk_map.is_empty()
        };

        if !has_tree {
            return Ok(base_response.to_string());
        }

        // Buscar contexto adicional
        match self.query(query, Some(2)).await {
            Ok(context) if !context.is_empty() => Ok(format!(
                "{}\n\n--- Información Adicional del Proyecto ---\n{}",
                base_response, context
            )),
            _ => Ok(base_response.to_string()),
        }
    }

    /// Obtener estadísticas
    pub async fn stats(&self) -> Result<String> {
        self.tool.raptor_stats().await
    }

    #[cfg(test)]
    pub fn build_fallback_context_from_chunks_testable(limit: usize) -> String {
        let store = {
            let store_guard = GLOBAL_STORE.lock().unwrap();
            store_guard.clone()
        };
        Self::build_fallback_context_from_chunks(&store, limit)
    }

    #[tokio::test]
    #[ignore] // HEAVY: Requires embedding model (~500MB). Run manually: cargo test -- --ignored
    async fn test_get_planning_context_comprehensive() {
        let embedder = EmbeddingEngine::new().await.unwrap();

        // Prepare a store with some chunks
        {
            let mut store = GLOBAL_STORE.lock().unwrap();
            store.insert_chunk("c1".to_string(), "Contenido extenso sobre la arquitectura del proyecto: módulos, rutas, pruebas, y más...".to_string());
            store.insert_chunk("c2".to_string(), "Notas de diseño: uso de RAPTOR, estrategia de indexado, y consideraciones".to_string());
        }

        let mut service = RaptorContextService::new(Arc::new(tokio::sync::Mutex::new(DualModelOrchestrator::with_config(crate::agent::orchestrator::OrchestratorConfig::default()).await.unwrap())));
        service.initialize_embedder().await.unwrap();

        let ctx = service.get_planning_context("explicar la arquitectura del proyecto").await.unwrap();
        assert!(ctx.len() > 0, "Context should not be empty for prepared store");
    }

    /// Limpiar árbol
    pub async fn clear(&self) -> Result<String> {
        self.tool.clear_raptor().await
    }

    /// Verificar si hay contexto RAPTOR disponible
    pub fn has_context(&self) -> bool {
        let store = GLOBAL_STORE.lock().unwrap();
        !store.chunk_map.is_empty()
    }

    /// Get a debug report showing summaries and chunk scores/ids for a given query.
    pub async fn get_debug_context(&mut self, task_description: &str) -> Result<String> {
        let has_tree = {
            let store = GLOBAL_STORE.lock().unwrap();
            !store.chunk_map.is_empty()
        };

        if !has_tree {
            return Ok("(No RAPTOR context available - no chunks indexed)".to_string());
        }

        // Fast path: quick fallback to raw chunks so /rag-debug returns fast results
        let repo_ctx = {
            let store_guard = GLOBAL_STORE.lock().unwrap();
            Self::build_fallback_context_from_chunks(&store_guard, 6)
        };

        let mut out = String::new();
        out.push_str(&format!("🔍 Debug RAG report for: '{}'\n\n", task_description));

        if !repo_ctx.is_empty() {
            out.push_str("📂 Repo-aware textual context (quick):\n");
            out.push_str(&repo_ctx);
            out.push_str("\n---\n\n");
        }

        // Spawn background task to run embedding-based retrieval and log results when ready.
        // This runs asynchronously so /rag-debug returns immediately and the TUI won't freeze.
        let task_query = task_description.to_string();
        tokio::spawn(async move {
            let embedder = if let Ok(e) = EmbeddingEngine::new().await {
                Some(Arc::new(e))
            } else {
                None
            };

            if let Some(embedder) = embedder {
                // Clone store in background task to avoid holding lock during retrieval
                let store_clone = {
                    let store_guard = GLOBAL_STORE.lock().unwrap();
                    store_guard.clone()
                };
                
                let retriever = TreeRetriever::new(&embedder, &store_clone);
                match tokio::time::timeout(Duration::from_secs(15), retriever.retrieve_with_context(&task_query, 12, 24)).await {
                    Ok(Ok((summaries, chunks))) => {
                        log_info!("🔍 [RAPTOR-BG] Retrieved {} summaries and {} chunks for query: {}", summaries.len(), chunks.len(), task_query);
                    }
                    Ok(Err(e)) => {
                        log_info!("⚠ [RAPTOR-BG] Retrieval error: {}", e);
                    }
                    Err(_) => {
                        log_info!("⚠ [RAPTOR-BG] Retrieval timed out after 15s");
                    }
                }
            } else {
                log_info!("⚠ [RAPTOR-BG] Embedder init failed for query: {}", task_query);
            }
        });

        out.push_str("(Embedding-based summaries and chunk details will be logged asynchronously and may appear in debug logs shortly.)\n");

        Ok(out)
    }
}

/// Extensión para PlanningOrchestrator que añade capacidades RAPTOR
pub trait RaptorPlanningExtension {
    /// Crear plan con contexto RAPTOR enriquecido
    #[allow(async_fn_in_trait)]
    async fn generate_plan_with_raptor(
        &mut self,
        goal: &str,
        raptor_service: &mut RaptorContextService,
    ) -> Result<String>;
}

impl RaptorPlanningExtension for PlanningOrchestrator {
    async fn generate_plan_with_raptor(
        &mut self,
        goal: &str,
        raptor_service: &mut RaptorContextService,
    ) -> Result<String> {
        // Obtener contexto del proyecto desde RAPTOR
        let context = raptor_service.get_planning_context(goal).await?;

        // Enriquecer el goal con contexto
        let enriched_goal = if !context.is_empty() {
            format!("{}\n\n{}", goal, context)
        } else {
            goal.to_string()
        };

        // Generar plan con contexto enriquecido
        // Nota: Esto requeriría modificar el PlanningOrchestrator para aceptar contexto adicional
        // Por ahora retornamos el contexto para que pueda ser usado manualmente
        Ok(enriched_goal)
    }
}

/// Comandos CLI para RAPTOR
pub mod cli_commands {
    use super::*;

    /// Construir árbol RAPTOR desde CLI
    pub async fn build_tree_command(service: &mut RaptorContextService, path: &str) -> Result<()> {
        log_info!("🔨 Construyendo árbol RAPTOR para: {}", path);
        log_info!("⏳ Esto puede tomar algunos minutos dependiendo del tamaño...\n");

        let result = service.build_tree(path, Some(500), Some(0.7)).await?;
        log_info!("{}", result);

        Ok(())
    }

    /// Consultar árbol RAPTOR desde CLI
    pub async fn query_tree_command(service: &mut RaptorContextService, query: &str) -> Result<()> {
        log_info!("🔍 Consultando: {}\n", query);

        let result = service.query(query, Some(5)).await?;
        log_info!("{}", result);

        Ok(())
    }

    /// Mostrar estadísticas desde CLI
    pub async fn stats_command(service: &RaptorContextService) -> Result<()> {
        let stats = service.stats().await?;
        log_info!("{}", stats);
        Ok(())
    }

    /// Limpiar árbol desde CLI
    pub async fn clear_command(service: &RaptorContextService) -> Result<()> {
        println!("🗑️ Limpiando árbol RAPTOR...");
        let result = service.clear().await?;
        println!("{}", result);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::orchestrator::OrchestratorConfig;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    use crate::raptor::persistence::GLOBAL_STORE;

    #[test]
    fn test_build_fallback_context_from_chunks() {
        // Prepare store with some chunks
        {
            let mut store = GLOBAL_STORE.lock().unwrap();
            store.chunk_map.clear();
            store.insert_chunk("c1".to_string(), "fn main() { println!(\"hello\"); }".to_string());
            store.insert_chunk("c2".to_string(), "// helper function\nfn help() {}".to_string());
        }

        let ctx = RaptorContextService::build_fallback_context_from_chunks_testable(2);
        assert!(ctx.contains("fn main"));
        assert!(ctx.contains("helper function"));
    }

    #[tokio::test]
    #[ignore] // Heavy test: loads embedding model and LLM. Run with: cargo test -- --ignored
    async fn test_raptor_service_integration() {
        // Setup
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let mut f = File::create(&file).unwrap();
        write!(
            f,
            "Este es un documento de prueba sobre gatos y perros.\n\
                   Los gatos son animales independientes que disfrutan de la soledad.\n\
                   Los perros son animales sociales que necesitan compañía constante."
        )
        .unwrap();

        let config = OrchestratorConfig::default();
        let orch = DualModelOrchestrator::with_config(config).await.unwrap();
        let mut service = RaptorContextService::new(Arc::new(AsyncMutex::new(orch)));

        // Build tree
        let build_result = service
            .build_tree(dir.path().to_str().unwrap(), Some(200), Some(0.7))
            .await;
        assert!(build_result.is_ok());

        // Check context is available
        assert!(service.has_context());

        // Query tree
        let query_result = service.query("información sobre gatos", Some(3)).await;
        assert!(query_result.is_ok());

        // Get planning context
        let planning_context = service
            .get_planning_context("analizar comportamiento de animales")
            .await;
        assert!(planning_context.is_ok());
        assert!(!planning_context.unwrap().is_empty());

        // Stats
        let stats = service.stats().await;
        assert!(stats.is_ok());

        // Clear
        let clear_result = service.clear().await;
        assert!(clear_result.is_ok());
        assert!(!service.has_context());
    }
}
