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
            return Ok(String::new());
        }

        // Consultar árbol - clonar store para evitar mantener lock durante await
        let embedder = self.embedder.as_ref().unwrap();
        let store_clone = {
            let store_guard = GLOBAL_STORE.lock().unwrap();
            store_guard.clone()
        }; // Lock liberado aquí

        let retriever = TreeRetriever::new(embedder, &store_clone);
        let (summaries, chunks) = retriever
            .retrieve_with_context(task_description, 3, 5, 0.85)
            .await?;

        // Formatear contexto para el planner
        let mut context = String::from("\n=== Contexto Relevante del Proyecto ===\n\n");

        if !summaries.is_empty() {
            context.push_str("📊 Resúmenes de Alto Nivel:\n");
            for (i, (_, _, summary)) in summaries.iter().enumerate() {
                context.push_str(&format!("{}. {}\n", i + 1, summary));
            }
            context.push('\n');
        }

        if !chunks.is_empty() {
            context.push_str("📄 Detalles Específicos:\n");
            for (i, (_, _, text)) in chunks.iter().take(3).enumerate() {
                let truncated = text.chars().take(200).collect::<String>();
                context.push_str(&format!("{}. {}...\n", i + 1, truncated));
            }
        }

        context.push_str("\n=== Fin del Contexto ===\n");

        Ok(context)
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

    /// Limpiar árbol
    pub async fn clear(&self) -> Result<String> {
        self.tool.clear_raptor().await
    }

    /// Verificar si hay contexto RAPTOR disponible
    pub fn has_context(&self) -> bool {
        let store = GLOBAL_STORE.lock().unwrap();
        !store.chunk_map.is_empty()
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
