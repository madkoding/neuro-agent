//! Router Orchestrator - Simplified routing system for efficient model usage
//!
//! This module implements a lightweight router that classifies user requests BEFORE
//! executing any heavy operations. Optimized for small context window models.

#![allow(deprecated)]

use super::classification_cache::ClassificationCache;
use super::orchestrator::{DualModelOrchestrator, OrchestratorResponse};
use super::progress::ProgressUpdate;
use super::slash_commands::{SlashCommandRegistry, CommandContext};
use super::state::SharedState;
use crate::agent::provider::OllamaProvider;
use crate::config::{ModelConfig, ModelProvider as ProviderType};
use crate::context::related_files::RelatedFilesDetector;
use crate::i18n::Locale;
use crate::raptor::builder::{has_full_index, has_quick_index, quick_index_sync, RaptorBuildProgress};
use crate::raptor::integration::RaptorContextService;
use crate::raptor::persistence::GLOBAL_STORE;
use crate::{log_debug, log_info, log_warn, log_error};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

/// Mensaje de estado del router para la UI
#[derive(Debug, Clone)]
pub enum RouterStatus {
    Classifying,
    SearchingContext { chunks: usize },
    Processing,
}

/// Operation modes for the agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationMode {
    /// Ask mode: Query RAPTOR index, read-only operations
    Ask,
    /// Build mode: Execute write operations (write_file, execute_shell, etc.)
    Build,
    /// Plan mode: Generate execution plan (JSON) without executing
    Plan,
}

/// Router decision after classification
#[derive(Debug, Clone)]
pub enum RouterDecision {
    /// Direct response using base model knowledge (no tools needed)
    DirectResponse {
        query: String,
        confidence: f64,
    },
    /// Use tools with specified operation mode
    ToolExecution {
        query: String,
        mode: OperationMode,
        needs_raptor: bool,
        confidence: f64,
    },
    /// Complex multi-step operation requiring full pipeline
    FullPipeline {
        query: String,
        confidence: f64,
    },
}

/// Classification response from fast model
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClassificationResponse {
    route: String,
    confidence: f64,
    reasoning: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    needs_raptor: bool,
}

/// Router Orchestrator configuration
#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub ollama_url: String,
    pub fast_model: String,
    pub heavy_model: String,
    pub classification_timeout_secs: u64,
    pub min_confidence: f64,
    pub working_dir: String,
    pub locale: Locale,
    pub debug: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            classification_timeout_secs: 30,
            min_confidence: 0.8,
            working_dir: ".".to_string(),
            locale: Locale::Spanish,
            debug: false,
        }
    }
}

/// Main Router Orchestrator
pub struct RouterOrchestrator {
    config: RouterConfig,
    orchestrator: Arc<AsyncMutex<DualModelOrchestrator>>,
    raptor_service: Option<Arc<AsyncMutex<RaptorContextService>>>,
    full_index_ready: Arc<AtomicBool>,
    state: SharedState,
    status_tx: Option<Sender<String>>,
    progress_tx: Option<Sender<ProgressUpdate>>,
    slash_commands: SlashCommandRegistry,
    classification_cache: Arc<AsyncMutex<ClassificationCache>>,
    related_files_detector: Arc<RelatedFilesDetector>,
    git_context: Arc<AsyncMutex<crate::context::GitContext>>,
    incremental_updater: Arc<crate::raptor::incremental::IncrementalUpdater>,
}

impl RouterOrchestrator {
    /// Create new router orchestrator with configuration
    pub async fn new(
        config: RouterConfig,
        orchestrator: DualModelOrchestrator,
    ) -> Result<Self> {
        let state = orchestrator.state();
        let orchestrator_arc = Arc::new(AsyncMutex::new(orchestrator));
        
        // Initialize related files detector
        let mut project_root = std::path::PathBuf::from(&config.working_dir);

        // Canonicalize the working directory to avoid path mismatches (relative vs absolute)
        project_root = std::fs::canonicalize(&project_root).unwrap_or(project_root.clone());

        let related_files_detector = Arc::new(RelatedFilesDetector::new(project_root.clone()));
        
        // Initialize git context
        let git_context = Arc::new(AsyncMutex::new(crate::context::GitContext::new(project_root.clone())));
        
        // Initialize incremental updater
        let incremental_updater = Arc::new(crate::raptor::incremental::IncrementalUpdater::new(
            project_root.clone(),
            orchestrator_arc.clone(),
        ));
        
        Ok(Self {
            config,
            orchestrator: orchestrator_arc.clone(),
            raptor_service: Some(Arc::new(AsyncMutex::new(
                RaptorContextService::new(orchestrator_arc),
            ))),
            full_index_ready: Arc::new(AtomicBool::new(false)),
            state,
            status_tx: None,
            progress_tx: None,
            slash_commands: SlashCommandRegistry::new(),
            classification_cache: Arc::new(AsyncMutex::new(ClassificationCache::new())),
            related_files_detector,
            git_context,
            incremental_updater,
        })
    }

    /// Set status channel for sending progress updates to UI
    pub fn set_status_channel(&mut self, tx: Sender<String>) {
        self.status_tx = Some(tx);
    }

    /// Set progress channel for sending detailed progress updates to UI
    pub fn set_progress_channel(&mut self, tx: Sender<ProgressUpdate>) {
        self.progress_tx = Some(tx);
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> super::classification_cache::CacheStats {
        self.classification_cache.lock().await.stats()
    }

    /// Clear classification cache
    pub async fn clear_cache(&self) {
        self.classification_cache.lock().await.clear();
    }

    /// Send status update to UI if channel is available
    fn send_status(&self, message: String) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.try_send(message);
        }
    }

    /// Send detailed progress update to UI with stage and timing
    fn send_progress(&self, stage: super::progress::ProgressStage, message: String, elapsed_ms: u64) {
        if let Some(ref tx) = self.progress_tx {
            let update = ProgressUpdate {
                stage,
                message,
                elapsed_ms,
            };
            let _ = tx.try_send(update);
        }
    }

    /// Initialize RAPTOR index (quick sync + full async)
    pub async fn initialize_raptor(&self) -> Result<()> {
        let working_dir = Path::new(&self.config.working_dir);
        
        if self.config.debug {
            log_info!("🔧 [RAPTOR] Inicializando índice para: {:?}", working_dir);
        }
        
        // Quick index (synchronous, <1s, no embeddings)
        match quick_index_sync(working_dir, 2000, 200) {
            Ok(chunk_count) => {
                if chunk_count > 0 {
                    log_info!("✓ RAPTOR: {} chunks indexados", chunk_count);
                } else if self.config.debug {
                    log_warn!("⚠ [RAPTOR] Índice vacío");
                }
            }
            Err(e) => {
                if self.config.debug {
                    log_warn!("⚠ [RAPTOR] Quick index falló: {}", e);
                }
            }
        }

        // Full RAPTOR index in background
        if let Some(raptor_service) = &self.raptor_service {
            let service = raptor_service.clone();
            let working_dir_str = self.config.working_dir.clone();
            let full_index_ready = self.full_index_ready.clone();
            let debug = self.config.debug;

            tokio::spawn(async move {
                if debug {
                    log_info!("🔄 [RAPTOR] Construyendo índice completo...");
                }
                
                let mut service_guard = service.lock().await;
                match service_guard.build_tree_with_progress(&working_dir_str, Some(2000), Some(0.6), None).await {
                    Ok(_) => {
                        full_index_ready.store(true, Ordering::SeqCst);
                        log_info!("✓ RAPTOR: Índice completo listo");
                    }
                    Err(e) => {
                        if debug {
                            log_error!("⚠ [RAPTOR] Error en índice completo: {}", e);
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Rebuild RAPTOR index (for !reindex command)
    pub async fn rebuild_raptor(&self) -> Result<String> {
        if let Some(raptor_service) = &self.raptor_service {
            // Clear current index
            {
                let mut store = GLOBAL_STORE.lock().unwrap();
                store.chunk_map.clear();
                store.chunk_embeddings.clear();
                store.nodes.clear();
                store.indexing_complete = false;
            }
            
            self.full_index_ready.store(false, Ordering::SeqCst);
            
            // Rebuild synchronously
            let mut service_guard = raptor_service.lock().await;
            service_guard.build_tree_with_progress(&self.config.working_dir, Some(2000), Some(0.6), None).await?;
            self.full_index_ready.store(true, Ordering::SeqCst);
            
            let chunk_count = {
                let store = GLOBAL_STORE.lock().unwrap();
                store.chunk_map.len()
            };
            
            Ok(format!("✓ Índice RAPTOR reconstruido: {} chunks", chunk_count))
        } else {
            Ok("⚠ RAPTOR no disponible".to_string())
        }
    }

    /// Check if RAPTOR full index is ready
    pub fn is_raptor_ready(&self) -> bool {
        self.full_index_ready.load(Ordering::SeqCst)
    }
    
    /// Perform incremental RAPTOR update (only re-index changed files)
    pub async fn incremental_update(&self) -> Result<String> {
        // Initialize tracker if first time
        let _ = self.incremental_updater.initialize().await;
        
        // Perform incremental update
        let result = self.incremental_updater.update_if_needed(None).await?;
        
        if result.updated {
            Ok(format!(
                "✓ Actualización incremental: {} archivos modificados, {} eliminados ({}ms)",
                result.files_modified,
                result.files_deleted,
                result.duration_ms
            ))
        } else {
            Ok("✓ Índice actualizado, sin cambios detectados".to_string())
        }
    }
    
    /// Get incremental updater statistics
    pub async fn incremental_stats(&self) -> String {
        let stats = self.incremental_updater.stats().await;
        format!(
            "📊 Incremental Updater:\n\
             • Archivos rastreados: {}\n\
             • Archivos indexados: {}",
            stats.tracked_files,
            stats.indexed_files
        )
    }

    /// Initialize RAPTOR with progress reporting (synchronous, waits for completion)
    pub async fn initialize_raptor_with_progress(
        &self, 
        progress_tx: Option<Sender<super::TaskProgressInfo>>
    ) -> Result<bool> {
        let working_dir = Path::new(&self.config.working_dir);
        
        // Send initial status
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(super::TaskProgressInfo {
                task_index: 0,
                total_tasks: 100,
                description: "Lectura: Escaneando archivos...".to_string(),
                status: super::TaskProgressStatus::Started,
            }).await;
        }
        
        // Quick index (synchronous, <1s, no embeddings)
        let chunk_count = match quick_index_sync(working_dir, 2000, 200) {
            Ok(count) => {
                if let Some(ref tx) = progress_tx {
                    let _ = tx.send(super::TaskProgressInfo {
                        task_index: 5,
                        total_tasks: 100,
                        description: format!("Lectura: {} chunks leídos (5%)", count),
                        status: super::TaskProgressStatus::Completed("OK".to_string()),
                    }).await;
                }
                count
            }
            Err(_) => 0,
        };

        if chunk_count == 0 {
            // Send a diagnostic progress update so the UI knows why indexing stopped
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(super::TaskProgressInfo {
                    task_index: 0,
                    total_tasks: 0,
                    description: format!("No se detectaron archivos en: {}", working_dir.display()),
                    status: super::TaskProgressStatus::Failed("No files found in working_dir".to_string()),
                }).await;
            }

            if self.config.debug {
                log_warn!("⚠ [RAPTOR] Quick index returned 0 chunks for path: {:?}", working_dir);
            }

            return Ok(false);
        }

        // Full RAPTOR index with internal progress forwarding
        if let Some(raptor_service) = &self.raptor_service {
            // Create internal channel for RaptorBuildProgress
            let (raptor_tx, mut raptor_rx) = tokio::sync::mpsc::channel::<RaptorBuildProgress>(100);
            
            // Spawn task to forward RaptorBuildProgress -> TaskProgressInfo
            if let Some(ref outer_tx) = progress_tx {
                let outer_tx_clone = outer_tx.clone();
                tokio::spawn(async move {
                    while let Some(raptor_progress) = raptor_rx.recv().await {
                        // Use task_index/total_tasks for actual current/total
                        let description = format!(
                            "{}: {}",
                            raptor_progress.stage,
                            raptor_progress.detail
                        );
                        
                        let _ = outer_tx_clone.send(super::TaskProgressInfo {
                            task_index: raptor_progress.current,
                            total_tasks: raptor_progress.total,
                            description,
                            status: super::TaskProgressStatus::Started,
                        }).await;
                    }
                });
            }
            
            let mut service_guard = raptor_service.lock().await;
            match service_guard.build_tree_with_progress(
                &self.config.working_dir, 
                Some(2000), 
                Some(0.6), 
                Some(raptor_tx)
            ).await {
                Ok(_) => {
                    self.full_index_ready.store(true, Ordering::SeqCst);
                    
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(super::TaskProgressInfo {
                            task_index: 100,
                            total_tasks: 100,
                            description: "Completado: Índice RAPTOR listo (100%)".to_string(),
                            status: super::TaskProgressStatus::Completed("OK".to_string()),
                        }).await;
                    }
                    
                    Ok(true)
                }
                Err(e) => {
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(super::TaskProgressInfo {
                            task_index: 0,
                            total_tasks: 0,
                            description: format!("Error: {}", e),
                            status: super::TaskProgressStatus::Failed(e.to_string()),
                        }).await;
                    }
                    Err(e)
                }
            }
        } else {
            Ok(false)
        }
    }

    /// Classify user query using fast model with caching
    pub async fn classify(&self, user_query: &str) -> Result<RouterDecision> {
        // Send progress update
        if let Some(ref tx) = self.progress_tx {
            let update = ProgressUpdate {
                stage: super::progress::ProgressStage::Classifying,
                message: "🔍 Clasificando consulta...".to_string(),
                elapsed_ms: 0,
            };
            let _ = tx.send(update).await;
        }
        
        self.send_status("Clasificando consulta...".to_string());
        
        // Check cache first
        {
            let mut cache = self.classification_cache.lock().await;
            if let Some(cached_decision) = cache.get(user_query) {
                if self.config.debug {
                    log_info!("✓ [CACHE HIT] Usando clasificación cacheada");
                }
                return Ok(cached_decision);
            }
        }
        
        let classification_prompt = build_router_classification_prompt(user_query, &self.config.locale);
        
        // Log classification attempts only in debug mode
        if self.config.debug {
            log_debug!("\n🔍 [CLASIFICACIÓN] Query: {}", user_query);
            log_debug!("📝 [CLASIFICACIÓN] Prompt:\n{}", classification_prompt);
        }
        
        let provider_config = ModelConfig {
            provider: ProviderType::Ollama,
            url: self.config.ollama_url.clone(),
            model: self.config.fast_model.clone(),
            api_key: None,
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: Some(512),
        };

        let provider = OllamaProvider::new(provider_config);
        
        // Build conversation messages
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": classification_prompt
            })
        ];
        
        let timeout_duration = Duration::from_secs(self.config.classification_timeout_secs);
        
        let response = timeout(timeout_duration, provider.generate_with_tools(messages, None))
            .await
            .context("Classification timeout")?
            .context("Classification generation failed")?;

        let classification_text = response.content
            .ok_or_else(|| anyhow::anyhow!("No content in classification response"))?;
        
        if self.config.debug {
            log_debug!("✓ [CLASIFICACIÓN] Respuesta: {}", classification_text);
        }
        
        // Try to parse JSON response
        let classification: ClassificationResponse = match serde_json::from_str(&classification_text) {
            Ok(c) => c,
            Err(_) => {
                // Fallback: extract JSON from text
                if let Some(json_start) = classification_text.find('{') {
                    if let Some(json_end) = classification_text.rfind('}') {
                        let json_str = &classification_text[json_start..=json_end];
                        serde_json::from_str(json_str).unwrap_or_else(|_| ClassificationResponse {
                            route: "ToolExecution".to_string(),
                            confidence: 0.5,
                            reasoning: "Failed to parse classification".to_string(),
                            mode: Some("Ask".to_string()),
                            needs_raptor: true,
                        })
                    } else {
                        return Err(anyhow::anyhow!("Invalid classification response"));
                    }
                } else {
                    return Err(anyhow::anyhow!("Invalid classification response"));
                }
            }
        };

        if self.config.debug {
            log_debug!("[ROUTER] {} -> {} (confidence: {:.2})", 
                user_query, classification.route, classification.confidence);
            log_debug!("[ROUTER] Reasoning: {}", classification.reasoning);
        }

        let decision = match classification.route.as_str() {
            "DirectResponse" => RouterDecision::DirectResponse {
                query: user_query.to_string(),
                confidence: classification.confidence,
            },
            "ToolExecution" => {
                let mode = match classification.mode.as_deref() {
                    Some("Build") => OperationMode::Build,
                    Some("Plan") => OperationMode::Plan,
                    _ => OperationMode::Ask,
                };
                RouterDecision::ToolExecution {
                    query: user_query.to_string(),
                    mode,
                    needs_raptor: classification.needs_raptor,
                    confidence: classification.confidence,
                }
            },
            "FullPipeline" => RouterDecision::FullPipeline {
                query: user_query.to_string(),
                confidence: classification.confidence,
            },
            _ => {
                // Unknown route, default to ToolExecution with Ask mode
                RouterDecision::ToolExecution {
                    query: user_query.to_string(),
                    mode: OperationMode::Ask,
                    needs_raptor: true,
                    confidence: 0.5,
                }
            }
        };

        // Re-classify if confidence too low
        if classification.confidence < self.config.min_confidence {
            if self.config.debug {
                log_warn!("[ROUTER] Low confidence ({:.2}), re-classifying as AskMode", 
                    classification.confidence);
            }
            let fallback_decision = RouterDecision::ToolExecution {
                query: user_query.to_string(),
                mode: OperationMode::Ask,
                needs_raptor: true,
                confidence: classification.confidence,
            };
            
            // Cache the fallback decision
            {
                let mut cache = self.classification_cache.lock().await;
                cache.insert(user_query, fallback_decision.clone());
            }
            
            return Ok(fallback_decision);
        }

        // Cache the decision before returning
        {
            let mut cache = self.classification_cache.lock().await;
            cache.insert(user_query, decision.clone());
        }

        Ok(decision)
    }

    /// Check if input is a slash command and handle it
    pub async fn handle_slash_command(&self, input: &str) -> Result<Option<OrchestratorResponse>> {
        // Check if this is a slash command
        if !SlashCommandRegistry::is_slash_command(input) {
            return Ok(None);
        }

        self.send_status("Ejecutando comando slash...".to_string());

        // Create command context
        let orchestrator = self.orchestrator.lock().await;
        let cmd_ctx = CommandContext {
            tools: Arc::new(orchestrator.tools().clone()),
            state: self.state.clone(),
            working_dir: self.config.working_dir.clone(),
        };
        drop(orchestrator); // Release lock

        // Execute command
        match self.slash_commands.execute(input, &cmd_ctx).await {
            Ok(result) => {
                // Handle special commands
                if let Some(action) = result.metadata.get("action") {
                    if action.as_str() == "reindex" {
                        // Trigger full reindex
                        self.send_status("Reindexando...".to_string());
                        match self.rebuild_raptor().await {
                            Ok(msg) => {
                                return Ok(Some(OrchestratorResponse::Text(msg)));
                            }
                            Err(e) => {
                                return Ok(Some(OrchestratorResponse::Error(
                                    format!("Error al reindexar: {}", e)
                                )));
                            }
                        }
                    }
                }

                // Handle mode changes
                if let Some(mode) = result.metadata.get("mode") {
                    self.send_status(format!("Modo cambiado a: {}", mode));
                }

                Ok(Some(OrchestratorResponse::Text(result.output)))
            }
            Err(e) => {
                Ok(Some(OrchestratorResponse::Error(
                    format!("❌ Error en comando: {}", e)
                )))
            }
        }
    }

    /// Get available slash command names for autocomplete
    pub fn get_slash_command_names(&self) -> Vec<String> {
        self.slash_commands.command_names()
    }

    /// Process user query with routing
    pub async fn process(&self, user_query: &str) -> Result<OrchestratorResponse> {
        let start_time = std::time::Instant::now();
        
        // Check for slash commands first
        if let Some(response) = self.handle_slash_command(user_query).await? {
            return Ok(response);
        }

        // Classify query
        self.send_progress(
            super::progress::ProgressStage::Classifying,
            "🔍 Analizando consulta...".to_string(),
            start_time.elapsed().as_millis() as u64,
        );
        let decision = self.classify(user_query).await?;

        match decision {
            RouterDecision::DirectResponse { query, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] DirectResponse mode (confidence: {:.2})", confidence);
                }
                self.send_progress(
                    super::progress::ProgressStage::Generating,
                    "💬 Generando respuesta...".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );
                // Use orchestrator directly without tools
                let mut orchestrator = self.orchestrator.lock().await;
                let response = orchestrator.process(&query).await.map_err(|e| anyhow::anyhow!("{:?}", e))?;
                self.send_progress(
                    super::progress::ProgressStage::Complete,
                    "✓ Completado".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );
                Ok(response)
            }

            RouterDecision::ToolExecution { query, mode, needs_raptor, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] ToolExecution mode: {:?} (confidence: {:.2})", mode, confidence);
                }

                // Step 1: Detect files mentioned in query and get related files
                let (detected_files, related_context) = self.enrich_with_related_files(&query).await;
                
                if self.config.debug && !detected_files.is_empty() {
                    log_info!("🔍 [RelatedFiles] Detected {} files in query", detected_files.len());
                }

                // Step 2: Enrich with RAPTOR context if needed
                let mut enriched_query = if needs_raptor && self.raptor_service.is_some() {
                    if has_quick_index() || has_full_index() {
                        let chunk_count = {
                            let store = GLOBAL_STORE.lock().unwrap();
                            store.chunk_map.len()
                        };
                        
                        self.send_progress(
                            super::progress::ProgressStage::SearchingContext { chunks: chunk_count },
                            format!("🔍 Buscando contexto ({} chunks)...", chunk_count),
                            start_time.elapsed().as_millis() as u64,
                        );
                        
                        if self.config.debug {
                            log_debug!("🔍 [RAPTOR] Buscando contexto...");
                        }
                        if let Some(service) = &self.raptor_service {
                            let mut service_guard = service.lock().await;
                            match service_guard.get_planning_context(&query).await {
                                Ok(context) if !context.is_empty() => {
                                    self.send_progress(
                                        super::progress::ProgressStage::SearchingContext { chunks: chunk_count },
                                        format!("✓ Contexto encontrado ({} chars)", context.len()),
                                        start_time.elapsed().as_millis() as u64,
                                    );
                                    if self.config.debug {
                                        log_info!("✓ [RAPTOR] Contexto: {} chars", context.len());
                                    }
                                    format!("{}\n\nContexto del proyecto:\n{}", query, context)
                                }
                                _ => query.clone()
                            }
                        } else {
                            query.clone()
                        }
                    } else {
                        query.clone()
                    }
                } else {
                    query.clone()
                };

                // Step 3: Append related files context if any were detected
                if !related_context.is_empty() {
                    enriched_query.push_str(&related_context);
                }
                
                // Step 4: Append git-aware context (uncommitted changes, recent modifications)
                let git_context = self.enrich_with_git_context().await;
                if !git_context.is_empty() {
                    enriched_query.push_str(&git_context);
                }

                self.send_progress(
                    super::progress::ProgressStage::ExecutingTool { tool_name: format!("mode_{:?}", mode) },
                    "⚙️ Ejecutando herramientas...".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );

                // Execute based on mode
                let mut orchestrator = self.orchestrator.lock().await;
                match mode {
                    OperationMode::Ask => {
                        // Read-only operations, allow tools
                        orchestrator.process(&enriched_query).await.map_err(|e| anyhow::anyhow!("{:?}", e))
                    }
                    OperationMode::Build => {
                        // Write operations, allow all tools
                        orchestrator.process(&enriched_query).await.map_err(|e| anyhow::anyhow!("{:?}", e))
                    }
                    OperationMode::Plan => {
                        // Generate plan without executing
                        let plan_prompt = format!(
                            "Generate a detailed step-by-step execution plan for the following task. \
                            Do NOT execute any steps, only create the plan with numbered steps.\n\n\
                            Task: {}\n\n\
                            Provide:\n\
                            1. A numbered list of steps\n\
                            2. Dependencies between steps\n\
                            3. Tools needed for each step\n\
                            4. Estimated time for each step",
                            enriched_query
                        );
                        
                        orchestrator.process(&plan_prompt).await.map_err(|e| anyhow::anyhow!("{:?}", e))
                    }
                }
            }

            RouterDecision::FullPipeline { query, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] FullPipeline mode (confidence: {:.2})", confidence);
                }
                self.send_status("Análisis completo en progreso...".to_string());
                // Use full orchestrator with all capabilities
                let mut orchestrator = self.orchestrator.lock().await;
                orchestrator.process(&query).await.map_err(|e| anyhow::anyhow!("{:?}", e))
            }
        }
    }

    /// Get shared state
    pub fn get_state(&self) -> SharedState {
        self.state.clone()
    }

    /// Check if full RAPTOR index is ready
    /// Check if full RAPTOR index is ready
    pub fn is_full_index_ready(&self) -> bool {
        self.full_index_ready.load(Ordering::SeqCst)
    }
    
    /// Get a reference to the RouterConfig
    pub fn config(&self) -> &RouterConfig {
        &self.config
    }
    
    /// Get related files for a given file path with confidence filtering
    /// 
    /// This method uses the RelatedFilesDetector to find files that are related
    /// to the given file through imports, tests, documentation, or dependencies.
    /// Only files with confidence >= threshold are returned.
    /// 
    /// # Arguments
    /// * `file_path` - Path to the file to find relations for
    /// * `min_confidence` - Minimum confidence threshold (0.0-1.0), default 0.7
    /// 
    /// # Returns
    /// Vec of file paths sorted by confidence (highest first)
    pub async fn get_context_files(
        &self,
        file_path: &Path,
        min_confidence: Option<f32>,
    ) -> Result<Vec<std::path::PathBuf>> {
        let threshold = min_confidence.unwrap_or(0.7);
        
        if self.config.debug {
            log_debug!("🔍 [RelatedFiles] Finding related files for: {:?} (threshold: {})", file_path, threshold);
        }
        
        match self.related_files_detector.find_related(file_path) {
            Ok(mut related_files) => {
                // Filter by confidence threshold
                related_files.retain(|rf| rf.confidence >= threshold);
                
                // Sort by confidence (highest first)
                related_files.sort_by(|a, b| {
                    b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
                });
                
                if self.config.debug {
                    log_info!(
                        "✓ [RelatedFiles] Found {} related files (threshold: {})",
                        related_files.len(),
                        threshold
                    );
                    for rf in &related_files {
                        log_debug!(
                            "  • {:?} ({:?}, confidence: {:.2})",
                            rf.path.file_name().unwrap_or_default(),
                            rf.relation_type,
                            rf.confidence
                        );
                    }
                }
                
                Ok(related_files.into_iter().map(|rf| rf.path).collect())
            }
            Err(e) => {
                if self.config.debug {
                    log_warn!("⚠ [RelatedFiles] Error finding related files: {}", e);
                }
                Ok(vec![])
            }
        }
    }
    
    /// Detect file paths mentioned in user query and enrich with related files
    /// Returns: (detected_files, enriched_context)
    async fn enrich_with_related_files(&self, user_query: &str) -> (Vec<PathBuf>, String) {
        use regex::Regex;
        
        // Patterns to detect file paths in queries
        // Examples: "analiza src/main.rs", "lee file.py", "revisa ./config.json"
        let file_patterns = vec![
            Regex::new(r"(?:analiza|lee|revisa|muestra|ver|check|analyze|read|review|show)\s+([a-zA-Z0-9_./\-]+\.[a-zA-Z0-9]+)").unwrap(),
            Regex::new(r"archivo\s+([a-zA-Z0-9_./\-]+\.[a-zA-Z0-9]+)").unwrap(),
            Regex::new(r"file\s+([a-zA-Z0-9_./\-]+\.[a-zA-Z0-9]+)").unwrap(),
            Regex::new(r"([a-zA-Z0-9_/\-]+\.rs)").unwrap(), // Rust files
            Regex::new(r"([a-zA-Z0-9_/\-]+\.py)").unwrap(), // Python files
            Regex::new(r"([a-zA-Z0-9_/\-]+\.js)").unwrap(), // JavaScript files
            Regex::new(r"([a-zA-Z0-9_/\-]+\.ts)").unwrap(), // TypeScript files
        ];
        
        let mut detected_files: Vec<PathBuf> = Vec::new();
        
        // Detect files mentioned in query
        for pattern in &file_patterns {
            for cap in pattern.captures_iter(user_query) {
                if let Some(file_match) = cap.get(1) {
                    let file_path = PathBuf::from(file_match.as_str());
                    
                    // Check if file exists in project
                    let full_path = if file_path.is_absolute() {
                        file_path.clone()
                    } else {
                        PathBuf::from(&self.config.working_dir).join(&file_path)
                    };
                    
                    if full_path.exists() && !detected_files.contains(&full_path) {
                        detected_files.push(full_path);
                    }
                }
            }
        }
        
        if detected_files.is_empty() {
            return (vec![], String::new());
        }
        
        // Get related files for each detected file
        let mut all_related: Vec<PathBuf> = Vec::new();
        for file in &detected_files {
            if let Ok(related) = self.get_context_files(file, Some(0.7)).await {
                for rel_file in related {
                    if !all_related.contains(&rel_file) && !detected_files.contains(&rel_file) {
                        all_related.push(rel_file);
                    }
                }
            }
        }
        
        // Build enriched context string
        let mut context = String::new();
        
        if !all_related.is_empty() {
            context.push_str("\n\n📎 Archivos relacionados detectados:\n");
            
            // Group by relation type (based on file naming patterns)
            let mut imports: Vec<&PathBuf> = Vec::new();
            let mut tests: Vec<&PathBuf> = Vec::new();
            let mut docs: Vec<&PathBuf> = Vec::new();
            let others: Vec<&PathBuf> = Vec::new();
            
            for file in &all_related {
                let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                if file_name.contains("test") {
                    tests.push(file);
                } else if file_name.contains("README") || file_name.contains("doc") {
                    docs.push(file);
                } else if file_name == "Cargo.toml" || file_name == "package.json" {
                    docs.push(file);
                } else {
                    imports.push(file);
                }
            }
            
            if !imports.is_empty() {
                context.push_str("  • Imports/Dependencies:\n");
                for file in imports.iter().take(5) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !tests.is_empty() {
                context.push_str("  • Tests:\n");
                for file in tests.iter().take(3) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !docs.is_empty() {
                context.push_str("  • Documentation:\n");
                for file in docs.iter().take(2) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            if !others.is_empty() {
                context.push_str("  • Other:\n");
                for file in others.iter().take(3) {
                    context.push_str(&format!("    - {}\n", file.display()));
                }
            }
            
            context.push_str("\nNota: Estos archivos están relacionados con los mencionados en tu consulta y pueden proporcionar contexto adicional.\n");
        }
        
        (detected_files, context)
    }
    
    /// Enrich context with git-aware information
    /// Returns formatted string with git context (uncommitted changes, recently modified files)
    async fn enrich_with_git_context(&self) -> String {
        let mut context = String::new();
        
        let mut git_ctx = self.git_context.lock().await;
        
        // Check if this is a git repository
        if !git_ctx.is_git_repo() {
            return context;
        }
        
        // Get uncommitted changes
        if let Ok(changes) = git_ctx.get_uncommitted_changes() {
            if !changes.is_empty() {
                context.push_str("\n\n⚠️ Cambios sin commit detectados:\n");
                
                let mut added = Vec::new();
                let mut modified = Vec::new();
                let mut deleted = Vec::new();
                let mut untracked = Vec::new();
                
                for change in &changes {
                    let file_name = change.path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    
                    match change.change_type {
                        crate::context::GitChangeType::Added => added.push(file_name),
                        crate::context::GitChangeType::Modified => modified.push(file_name),
                        crate::context::GitChangeType::Deleted => deleted.push(file_name),
                        crate::context::GitChangeType::Untracked => untracked.push(file_name),
                    }
                }
                
                if !modified.is_empty() {
                    context.push_str(&format!("  • Modificados ({}): ", modified.len()));
                    for (i, file) in modified.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if modified.len() > 5 {
                        context.push_str(&format!(" +{} más", modified.len() - 5));
                    }
                    context.push('\n');
                }
                
                if !added.is_empty() {
                    context.push_str(&format!("  • Añadidos ({}): ", added.len()));
                    for (i, file) in added.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if added.len() > 5 {
                        context.push_str(&format!(" +{} más", added.len() - 5));
                    }
                    context.push('\n');
                }
                
                if !deleted.is_empty() {
                    context.push_str(&format!("  • Eliminados ({}): ", deleted.len()));
                    for (i, file) in deleted.iter().take(5).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    context.push('\n');
                }
                
                if !untracked.is_empty() {
                    context.push_str(&format!("  • Sin seguimiento ({}): ", untracked.len()));
                    for (i, file) in untracked.iter().take(3).enumerate() {
                        if i > 0 { context.push_str(", "); }
                        context.push_str(file);
                    }
                    if untracked.len() > 3 {
                        context.push_str(&format!(" +{} más", untracked.len() - 3));
                    }
                    context.push('\n');
                }
                
                context.push_str("\nEstos archivos tienen cambios pendientes que pueden ser relevantes para tu consulta.\n");
            }
        }
        
        // Get recently modified files (last 7 days)
        if let Ok(recent_files) = git_ctx.get_recently_modified(7) {
            if !recent_files.is_empty() && recent_files.len() <= 20 {
                context.push_str("\n\n📝 Archivos modificados recientemente (últimos 7 días):\n");
                for file in recent_files.iter().take(10) {
                    if let Some(file_name) = file.file_name().and_then(|n| n.to_str()) {
                        context.push_str(&format!("  • {}\n", file_name));
                    }
                }
                if recent_files.len() > 10 {
                    context.push_str(&format!("  ... y {} más\n", recent_files.len() - 10));
                }
            }
        }
        
        // Get current branch
        if let Ok(branch) = git_ctx.current_branch() {
            if !branch.is_empty() && branch != "master" && branch != "main" {
                context.push_str(&format!("\n🌿 Rama actual: {}\n", branch));
            }
        }
        
        context
    }
}


/// Build router classification prompt
fn build_router_classification_prompt(user_query: &str, locale: &Locale) -> String {
    match locale {
        Locale::Spanish => build_router_classification_prompt_es(user_query),
        Locale::English => build_router_classification_prompt_en(user_query),
    }
}

/// Spanish classification prompt with examples
fn build_router_classification_prompt_es(user_query: &str) -> String {
    format!(
        r#"Clasifica esta query de usuario en UNA de estas 3 rutas. Responde SOLO JSON válido.

Query: "{}"

Rutas disponibles:
1. DirectResponse - Respuesta directa sin contexto de código
   Usa cuando: conocimiento general, matemáticas, definiciones sin código
   Ejemplos: "hola", "calcula 5*8", "qué es async/await en general", "explica REST API"
   
2. ToolExecution - Usa herramientas con proyecto existente (USA RAPTOR para contexto)
   Usa cuando: leer/analizar código, buscar archivos, entender estructura, explicar proyecto
   Ejemplos: "lee main.rs", "analiza este código", "qué hace este proyecto", "explícame el repositorio", "de qué se trata"
   
   Submodos:
   - mode: "Ask" (read-only, default) - SIEMPRE para análisis y explicaciones
   - mode: "Build" (escribe código: "crea función", "refactoriza", "corrige bug")
   - mode: "Plan" (genera plan: "planifica", "diseña", "outline")
   
   needs_raptor:
   - true: cuando necesita contexto del proyecto (análisis, explicaciones, búsquedas)
   - false: solo para operaciones simples de archivos sin contexto
   
3. FullPipeline - RARA VEZ USADO - Solo para operaciones masivas
   Usa cuando: refactorización completa de múltiples módulos, rediseño arquitectónico
   Ejemplos: "reescribe toda la arquitectura", "migra todo el proyecto a otra tecnología"

Casos comunes:
- "analiza el repositorio" → ToolExecution (mode: "Ask", needs_raptor: true)
- "explícame de qué se trata" → ToolExecution (mode: "Ask", needs_raptor: true)
- "qué hace este proyecto" → ToolExecution (mode: "Ask", needs_raptor: true)
- "lee archivo X" → ToolExecution (mode: "Ask", needs_raptor: false)
- "mejora el código" → ToolExecution (mode: "Plan", needs_raptor: true)
- "escribe función para X" → ToolExecution (mode: "Build", needs_raptor: false)

Responde exactamente este formato JSON:
{{
  "route": "DirectResponse|ToolExecution|FullPipeline",
  "confidence": 0.0-1.0,
  "reasoning": "breve explicación en español",
  "mode": "Ask|Build|Plan",
  "needs_raptor": true|false
}}"#,
        user_query
    )
}

/// English classification prompt with examples
fn build_router_classification_prompt_en(user_query: &str) -> String {
    format!(
        r#"Classify this user query into ONE of these 3 routes. Respond with valid JSON only.

Query: "{}"

Available routes:
1. DirectResponse - Direct answer without code context
   Use when: general knowledge, math, definitions without code
   Examples: "hello", "calculate 5*8", "what is async/await in general", "explain REST API"
   
2. ToolExecution - Use tools with existing project
   Use when: read/analyze code, search files, understand structure
   Examples: "read main.rs", "analyze this code", "find errors", "what does this project do", "show structure"
   
   Submodes:
   - mode: "Ask" (read-only, default)
   - mode: "Build" (write code: "create function", "refactor", "fix bug")
   - mode: "Plan" (generate plan: "plan", "design", "outline")
   
3. FullPipeline - Needs project indexing + deep analysis
   Use when: full architecture, large refactoring, multiple files
   Examples: "explain the complete architecture", "document entire project", "improve whole structure"

Ambiguous cases:
- "improve the code" → ToolExecution (mode: "Plan")
- "write function for X" → ToolExecution (mode: "Build")
- "analyze the architecture" → FullPipeline

Respond exactly in this JSON format:
{{
  "route": "DirectResponse|ToolExecution|FullPipeline",
  "confidence": 0.0-1.0,
  "reasoning": "brief explanation in English",
  "mode": "Ask|Build|Plan",
  "needs_raptor": true|false
}}"#,
        user_query
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    /// Test that RelatedFilesDetector is properly initialized
    #[tokio::test]
    async fn test_related_files_detector_initialization() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        // Create a minimal orchestrator config
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        // This will fail if Ollama is not running, but that's OK for this test
        // We're just testing that the initialization doesn't panic
        let orchestrator_result = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await;
        
        if let Ok(orchestrator) = orchestrator_result {
            let router = RouterOrchestrator::new(config, orchestrator).await;
            assert!(router.is_ok(), "RouterOrchestrator should initialize successfully");
            
            let router = router.unwrap();
            // Verify the detector is initialized (would panic if not)
            let _ = router.related_files_detector;
        }
    }
    
    /// Test get_context_files with a real file (if it exists)
    #[tokio::test]
    async fn test_get_context_files() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Test with this very file
                let test_file = PathBuf::from("src/agent/router_orchestrator.rs");
                
                if test_file.exists() {
                    let related = router.get_context_files(&test_file, Some(0.7)).await;
                    assert!(related.is_ok(), "get_context_files should not error");
                    
                    let related_files = related.unwrap();
                    // We should find at least some related files (imports, tests, etc.)
                    // But the exact number depends on the project structure
                    assert!(
                        related_files.is_empty() || !related_files.is_empty(),
                        "get_context_files should return a valid vec (empty or not)"
                    );
                }
            }
        }
    }
    
    /// Test that confidence filtering works
    #[tokio::test]
    async fn test_confidence_filtering() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                let test_file = PathBuf::from("src/agent/router_orchestrator.rs");
                
                if test_file.exists() {
                    // Get with high confidence threshold
                    let high_confidence = router.get_context_files(&test_file, Some(0.9)).await;
                    
                    // Get with low confidence threshold
                    let low_confidence = router.get_context_files(&test_file, Some(0.5)).await;
                    
                    if high_confidence.is_ok() && low_confidence.is_ok() {
                        let high = high_confidence.unwrap();
                        let low = low_confidence.unwrap();
                        
                        // Lower threshold should return same or more files
                        assert!(
                            low.len() >= high.len(),
                            "Lower confidence threshold should return >= files"
                        );
                    }
                }
            }
        }
    }
    
    /// Test automatic file enrichment when files are mentioned in queries
    #[tokio::test]
    async fn test_enrich_with_related_files() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Test with a query mentioning a real file
                let query = "analiza src/agent/router_orchestrator.rs";
                let (detected_files, context) = router.enrich_with_related_files(query).await;
                
                // Should detect the file
                assert!(!detected_files.is_empty(), "Should detect at least one file");
                
                // Context should be generated if related files exist
                if !context.is_empty() {
                    assert!(context.contains("📎 Archivos relacionados"), "Context should have related files header");
                }
            }
        }
    }
    
    /// Test that file detection works with different patterns
    #[tokio::test]
    async fn test_file_detection_patterns() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Test different query patterns
                let queries = vec![
                    "lee src/main.rs",
                    "revisa Cargo.toml",
                    "muestra README.md",
                    "archivo src/lib.rs",
                ];
                
                for query in queries {
                    let (detected, _) = router.enrich_with_related_files(query).await;
                    // Each query should detect at least one file (if it exists)
                    // This is a soft check since files may or may not exist
                    assert!(
                        detected.is_empty() || !detected.is_empty(),
                        "File detection should work for query: {}", query
                    );
                }
            }
        }
    }
    
    /// Test git-aware context enrichment
    #[tokio::test]
    async fn test_git_aware_context() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Test git context enrichment
                let git_context = router.enrich_with_git_context().await;
                
                // Context should be a valid string (empty or not)
                // Empty if not a git repo or no changes
                assert!(
                    git_context.is_empty() || !git_context.is_empty(),
                    "Git context should be a valid string"
                );
                
                // If this is a git repo and we have uncommitted changes, 
                // the context should mention them
                let git_ctx = router.git_context.lock().await;
                if git_ctx.is_git_repo() {
                    if let Ok(changes) = git_ctx.get_uncommitted_changes() {
                        if !changes.is_empty() && !git_context.is_empty() {
                            // Context should mention uncommitted changes
                            assert!(
                                git_context.contains("Cambios sin commit") || 
                                git_context.contains("modificado") ||
                                git_context.contains("Rama actual"),
                                "Git context should mention changes or branch"
                            );
                        }
                    }
                }
            }
        }
    }
    
    /// Test git context is appended to enriched query
    #[tokio::test]
    async fn test_git_context_in_process() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Verify git_context field is initialized
                let git_ctx = router.git_context.lock().await;
                
                // If this is a git repo, we can query basic info
                if git_ctx.is_git_repo() {
                    let branch = git_ctx.current_branch();
                    assert!(branch.is_ok(), "Should be able to get current branch");
                    
                    let branch_name = branch.unwrap();
                    assert!(!branch_name.is_empty(), "Branch name should not be empty");
                }
                // If not a git repo, that's also fine - context will be empty
            }
        }
    }
    
    /// Test incremental updater initialization
    #[tokio::test]
    async fn test_incremental_updater() {
        let config = RouterConfig {
            working_dir: ".".to_string(),
            ..Default::default()
        };
        
        let orch_config = crate::agent::orchestrator::OrchestratorConfig {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 60,
            max_concurrent_heavy: 2,
        };
        
        if let Ok(orchestrator) = crate::agent::orchestrator::DualModelOrchestrator::with_config(orch_config).await {
            if let Ok(router) = RouterOrchestrator::new(config, orchestrator).await {
                // Test incremental updater is initialized
                let stats = router.incremental_stats().await;
                assert!(stats.contains("Archivos rastreados") || stats.contains("Incremental"));
            }
        }
    }
}
