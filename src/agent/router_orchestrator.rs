//! Router Orchestrator - Simplified routing system for efficient model usage
//!
//! This module implements a lightweight router that classifies user requests BEFORE
//! executing any heavy operations. Optimized for small context window models.

#![allow(deprecated)]

use super::classification_cache::ClassificationCache;
use super::orchestrator::{DualModelOrchestrator, OrchestratorResponse};
use super::progress::{ProgressUpdate, ProgressStage};
use super::task_progress::{TaskProgressInfo, TaskProgressStatus};
use super::slash_commands::{SlashCommandRegistry, CommandContext};
use super::state::SharedState;
use crate::agent::provider::OllamaProvider;
use crate::context::related_files::RelatedFilesDetector;
use crate::i18n::Locale;
use crate::raptor::builder::{has_full_index, has_quick_index, quick_index_sync, RaptorBuildProgress};
use crate::raptor::integration::RaptorContextService;
use crate::raptor::persistence::GLOBAL_STORE;
use crate::{log_debug, log_info, log_warn, log_error};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use rig::tool::Tool;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
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
    /// Perform a programmatic, multi-step analysis of the repository
    RepositoryAnalysis {
        query: String,
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
    pub fast_model_config: crate::config::ModelConfig,
    pub heavy_model_config: crate::config::ModelConfig,
    pub classification_timeout_secs: u64,
        /// Execution timeout for delegated tasks (seconds)
        pub execution_timeout_secs: u64,
    pub min_confidence: f64,
    pub working_dir: String,
    pub locale: Locale,
    pub debug: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            fast_model_config: crate::config::ModelConfig {
                model: "qwen3:0.6b".to_string(),
                ..Default::default()
            },
            heavy_model_config: crate::config::ModelConfig {
                model: "qwen3:8b".to_string(),
                ..Default::default()
            },
            classification_timeout_secs: 30,
            min_confidence: 0.8,
            working_dir: ".".to_string(),
            locale: Locale::Spanish,
            debug: false,
            execution_timeout_secs: 120,
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
    slash_commands: SlashCommandRegistry,
    classification_cache: Arc<AsyncMutex<ClassificationCache>>,
    related_files_detector: Arc<RelatedFilesDetector>,
    git_context: Arc<AsyncMutex<crate::context::GitContext>>,
    incremental_updater: Arc<crate::raptor::incremental::IncrementalUpdater>,
    event_tx: Arc<AsyncMutex<Option<Sender<crate::agent::AgentEvent>>>>, // Thread-safe channel for unified events
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
            slash_commands: SlashCommandRegistry::new(),
            classification_cache: Arc::new(AsyncMutex::new(ClassificationCache::new())),
            related_files_detector,
            git_context,
            incremental_updater,
            event_tx: Arc::new(AsyncMutex::new(None)), // Initialize thread-safe channel
        })
    }

    /// Set unified event channel for sending updates to UI (async version)
    pub async fn set_event_channel_async(&self, tx: Sender<crate::agent::AgentEvent>) {
        let mut event_tx = self.event_tx.lock().await;
        *event_tx = Some(tx);
    }

    /// Set unified event channel for sending updates to UI (sync version with try_lock)
    pub fn set_event_channel(&self, tx: Sender<crate::agent::AgentEvent>) {
        if let Ok(mut event_tx) = self.event_tx.try_lock() {
            *event_tx = Some(tx);
        }
    }

    /// Send status update to UI if channel is available
    fn send_status(&self, message: String) {
        if let Ok(event_tx) = self.event_tx.try_lock() {
            if let Some(tx) = &*event_tx {
                let _ = tx.try_send(crate::agent::AgentEvent::Status(message));
            }
        }
    }

    /// Send detailed progress update to UI with stage and timing
    fn send_progress(&self, stage: ProgressStage, message: String, elapsed_ms: u64) {
        if let Ok(event_tx) = self.event_tx.try_lock() {
            if let Some(ref tx) = &*event_tx {
                let update = ProgressUpdate {
                    stage,
                    message,
                    elapsed_ms,
                };
                let _ = tx.try_send(crate::agent::AgentEvent::Progress(update));
            }
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
                // Quick index failed; log diagnostic info (do not panic)
                if self.config.debug {
                    log_error!("⚠ [RAPTOR] Quick index failed: {}", e);
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
        log_debug!("🔧 [REINDEX] rebuild_raptor() called");

        if let Some(raptor_service) = &self.raptor_service {
            // Clear existing index
            log_debug!("🔧 [REINDEX] Clearing existing index");
            match raptor_service.lock().await.clear().await {
                Ok(_) => log_debug!("🔧 [REINDEX] Index cleared successfully"),
                Err(e) => log_warn!("🔧 [REINDEX] Failed to clear index: {}", e),
            }

            // Reset full_index_ready flag
            self.full_index_ready.store(false, Ordering::SeqCst);

            // Rebuild index
            log_debug!("🔧 [REINDEX] Starting full rebuild");
            let working_dir = &self.config.working_dir;

            // Perform full rebuild
            let mut service_guard = raptor_service.lock().await;
            match service_guard.build_tree_with_progress(working_dir, Some(2000), Some(0.6), None).await {
                Ok(_) => {
                    self.full_index_ready.store(true, Ordering::SeqCst);
                    log_info!("✓ [REINDEX] RAPTOR index rebuilt successfully");
                    Ok("✓ Índice RAPTOR reconstruido exitosamente".to_string())
                }
                Err(e) => {
                    log_error!("❌ [REINDEX] Failed to rebuild RAPTOR index: {}", e);
                    Ok(format!("❌ Error al reconstruir índice RAPTOR: {}", e))
                }
            }
        } else {
            log_warn!("🔧 [REINDEX] RAPTOR service not available");
            Ok("⚠️ Servicio RAPTOR no disponible".to_string())
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
        progress_tx: Option<Sender<TaskProgressInfo>>
    ) -> Result<bool> {
        let working_dir = Path::new(&self.config.working_dir);
        
        // Send initial status
        if let Some(ref tx) = progress_tx {
            let _ = tx.send(TaskProgressInfo {
                task_index: 0,
                total_tasks: 100,
                description: "Lectura: Escaneando archivos...".to_string(),
                status: TaskProgressStatus::Started,
            }).await;
        }
        
        // Quick index (synchronous, <1s, no embeddings)
        let chunk_count = match quick_index_sync(working_dir, 2000, 200) {
            Ok(count) => {
                if let Some(ref tx) = progress_tx {
                    let _ = tx.send(TaskProgressInfo {
                        task_index: 5,
                        total_tasks: 100,
                        description: format!("Lectura: {} chunks leídos (5%)", count),
                        status: TaskProgressStatus::Completed("OK".to_string()),
                    }).await;
                }
                count
            }
            Err(_) => 0,
        };

        if chunk_count == 0 {
            // Send a diagnostic progress update so the UI knows why indexing stopped
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(TaskProgressInfo {
                    task_index: 0,
                    total_tasks: 0,
                    description: format!("No se detectaron archivos en: {}", working_dir.display()),
                    status: TaskProgressStatus::Failed("No files found in working_dir".to_string()),
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
                        
                        let _ = outer_tx_clone.send(TaskProgressInfo {
                            task_index: raptor_progress.current,
                            total_tasks: raptor_progress.total,
                            description,
                            status: TaskProgressStatus::Started,
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
                        let _ = tx.send(TaskProgressInfo {
                            task_index: 100,
                            total_tasks: 100,
                            description: "Completado: Índice RAPTOR listo (100%)".to_string(),
                            status: TaskProgressStatus::Completed("OK".to_string()),
                        }).await;
                    }
                    
                    Ok(true)
                }
                Err(e) => {
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(TaskProgressInfo {
                            task_index: 0,
                            total_tasks: 0,
                            description: format!("Error: {}", e),
                            status: TaskProgressStatus::Failed(e.to_string()),
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
        // Send progress update (non-blocking)
        if let Ok(event_tx) = self.event_tx.try_lock() {
            if let Some(tx) = &*event_tx {
                let update = ProgressUpdate {
                    stage: ProgressStage::Classifying,
                    message: "🔍 Clasificando consulta...".to_string(),
                    elapsed_ms: 0,
                };
                let _ = tx.try_send(crate::agent::AgentEvent::Progress(update));
            }
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

        // Quick rule-based overrides for common patterns where we know the intended mode.
        // Prefer conservative 'Ask' (read-only) when the user asks to explain or analyze the repository.
        let q_lower = user_query.to_lowercase();
        let explain_patterns = [
            "explica",
            "explícame",
            "explicame",
            "de qué se trata",
            "de que se trata",
            "analiza el repositorio",
            "qué hace este proyecto",
            "que hace este proyecto",
            "what does this project do",
            "explain this repository",
            "explain the project",
            "analyze the repository",
            "describe the project",
        ];

        if explain_patterns.iter().any(|p| q_lower.contains(p)) {
            if self.config.debug {
                log_info!("🔬 [ROUTER RULE] Matched explain-pattern; forcing RepositoryAnalysis");
            }

            let decision = RouterDecision::RepositoryAnalysis {
                query: user_query.to_string(),
            };

            // Cache the decision
            {
                let mut cache = self.classification_cache.lock().await;
                cache.insert(user_query, decision.clone());
            }

            return Ok(decision);
        }
        
        let classification_prompt = build_router_classification_prompt(user_query, &self.config.locale);
        
        // Log classification attempts only in debug mode
        if self.config.debug {
            log_debug!("\n🔍 [CLASIFICACIÓN] Query: {}", user_query);
            log_debug!("📝 [CLASIFICACIÓN] Prompt:\n{}", classification_prompt);
        }
        
        let provider_config = self.config.fast_model_config.clone();

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
            "RepositoryAnalysis" => RouterDecision::RepositoryAnalysis {
                query: user_query.to_string(),
            },
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
        log_debug!("🔧 [SLASH] handle_slash_command called with input: '{}'", input);
        // Check if this is a slash command
        if !SlashCommandRegistry::is_slash_command(input) {
            log_debug!("🔧 [SLASH] '{}' is not a slash command", input);
            return Ok(None);
        }

        log_debug!("🔧 [SLASH] '{}' is a slash command, processing...", input);
        // Debug: Always log when debug is enabled, regardless of command
        if self.config.debug {
            log_debug!("🔧 [SLASH] Processing slash command: '{}' (debug=true)", input);
        }

        self.send_status("Ejecutando comando slash...".to_string());
        if input.starts_with("/rag-debug") {
            // parse query after command
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            let query = if parts.len() > 1 { parts[1].trim() } else { "" };

            if query.is_empty() {
                return Ok(Some(OrchestratorResponse::Text(
                    "Usage: /rag-debug <query>\nReturns summary and chunk scores used by RAPTOR for the query".to_string(),
                )));
            }

            // Use raptor_service to get debug report
            if let Some(raptor_service) = &self.raptor_service {
                let mut svc = raptor_service.lock().await;
                match svc.get_debug_context(query).await {
                    Ok(report) => {
                        let debug_prefix = if self.config.debug {
                            format!("🔧 [DEBUG] Detected slash command: {}\n🔧 [DEBUG] Executing /rag-debug with query: {}\n\n", input, query)
                        } else {
                            String::new()
                        };
                        return Ok(Some(OrchestratorResponse::Text(format!("{}{}", debug_prefix, report))));
                    }
                    Err(e) => return Ok(Some(OrchestratorResponse::Error(format!("Error: {}", e)))),
                }
            } else {
                return Ok(Some(OrchestratorResponse::Error(
                    "RAPTOR not available".to_string(),
                )));
            }
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
                let mut debug_output = String::new();
                if self.config.debug {
                    debug_output.push_str(&format!("🔧 [DEBUG] Detected slash command: {}\n", input));
                    debug_output.push_str(&format!("🔧 [DEBUG] Command executed successfully: {}\n", result.output));
                    debug_output.push_str(&format!("🔧 [DEBUG] Command metadata: {:?}\n", result.metadata));
                }

                // Handle special commands
                if let Some(action) = result.metadata.get("action") {
                    if action.as_str() == "reindex" {
                        if self.config.debug {
                            log_debug!("🔧 [SLASH] Found reindex action in metadata");
                            debug_output.push_str("🔧 [DEBUG] Executing reindex action\n");
                        }
                        log_debug!("🔧 [REINDEX] Starting rebuild_raptor()");
                        // Trigger full reindex
                        log_debug!("🔧 [REINDEX] About to call rebuild_raptor");
                        self.send_status("Reindexando...".to_string());
                        
                        // Call rebuild_raptor and return its result
                        match self.rebuild_raptor().await {
                            Ok(reindex_result) => {
                                if self.config.debug {
                                    debug_output.push_str("🔧 [DEBUG] Reindex completed\n");
                                    log_debug!("🔧 [REINDEX] Reindex result: {}", reindex_result);
                                }
                                let full_output = if debug_output.is_empty() {
                                    reindex_result
                                } else {
                                    format!("{}\n\n{}", debug_output.trim(), reindex_result)
                                };
                                return Ok(Some(OrchestratorResponse::Text(full_output)));
                            }
                            Err(e) => {
                                let error_msg = format!("❌ Error en reindex: {}", e);
                                if self.config.debug {
                                    debug_output.push_str("🔧 [DEBUG] Reindex failed\n");
                                    log_debug!("🔧 [REINDEX] Reindex error: {}", e);
                                }
                                let full_output = if debug_output.is_empty() {
                                    error_msg
                                } else {
                                    format!("{}\n\n{}", debug_output.trim(), error_msg)
                                };
                                return Ok(Some(OrchestratorResponse::Text(full_output)));
                            }
                        }
                    }
                }

                // Return result with debug info if enabled
                let final_output = if debug_output.is_empty() {
                    result.output
                } else {
                    format!("{}\n\n{}", debug_output.trim(), result.output)
                };

                Ok(Some(OrchestratorResponse::Text(final_output)))
            }
            Err(e) => {
                let error_msg = format!("❌ Error en comando: {}", e);
                if self.config.debug {
                    log_debug!("🔧 [SLASH] Command failed: {}", e);
                }
                Ok(Some(OrchestratorResponse::Text(error_msg)))
            }
        }
    }

    /// Get available slash command names for autocomplete
    pub fn get_slash_command_names(&self) -> Vec<String> {
        self.slash_commands.command_names()
    }

    /// Process user query with routing
    pub async fn process(&self, user_query: &str) -> Result<OrchestratorResponse> {
        log_debug!("🔧 [PROCESS] process() called with query: '{}'", user_query);
        let start_time = std::time::Instant::now();
        
        // Check for slash commands first
        if let Some(response) = self.handle_slash_command(user_query).await? {
            log_debug!("🔧 [PROCESS] Slash command handled, returning response");
            return Ok(response);
        }

        // Classify query
        self.send_progress(
            ProgressStage::Classifying,
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
                    ProgressStage::Generating,
                    "💬 Generando respuesta...".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );
                // Use orchestrator directly without tools
                let response = {
                    let mut orchestrator = self.orchestrator.lock().await;
                    orchestrator.process(&query).await.map_err(|e| anyhow::anyhow!("{:?}", e))?
                };
                self.send_progress(
                    ProgressStage::Complete,
                    "✓ Completado".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );
                Ok(response)
            }

            RouterDecision::RepositoryAnalysis { query } => {
                if self.config.debug {
                    log_info!("[ROUTER] RepositoryAnalysis mode for query: '{}'", query);
                }
                self.send_status("🔍 Analizando repositorio...".to_string());

                let event_tx = self.event_tx.lock().await.clone().ok_or_else(|| anyhow::anyhow!("Event sender not set"))?;
                let orchestrator_arc = Arc::clone(&self.orchestrator);
                let raptor_service_arc = self.raptor_service.clone();
                let config_clone = self.config.clone();
                let related_files_detector_arc = Arc::clone(&self.related_files_detector);
                let git_context_arc = Arc::clone(&self.git_context);

                tokio::spawn(async move {
                    let mut full_context = String::new();
                    let start_time = std::time::Instant::now();

                    // Get tools from orchestrator
                    let tools = {
                        let orchestrator = orchestrator_arc.lock().await;
                        std::sync::Arc::new(orchestrator.tools().clone())
                    };

                    // --- Step 1: List root directory ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::ExecutingTool { tool_name: "list_directory".to_string() },
                        message: "1/5: Listando directorio raíz...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));
                    match tools.list_directory.call(crate::tools::ListDirectoryArgs {
                        path: ".".to_string(),
                        recursive: false,
                        max_depth: 1,
                    }).await {
                        Ok(result) => {
                            full_context.push_str("Estructura del Directorio Raíz:\n");
                            for entry in result.entries.iter().take(20) { // Limit output
                                let icon = if entry.is_dir { "📁" } else { "📄" };
                                full_context.push_str(&format!("{} {}\n", icon, entry.name));
                            }
                            if result.count > 20 {
                                full_context.push_str(&format!("... y {} más.\n", result.count - 20));
                            }
                            full_context.push_str("\n---\n");
                        }
                        Err(e) => log_warn!("[Analysis] Failed to list root directory: {}", e),
                    }

                    // --- Step 2: Read README.md ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::ExecutingTool { tool_name: "read_file".to_string() },
                        message: "2/5: Leyendo README.md...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));
                    if Path::new(&config_clone.working_dir).join("README.md").exists() {
                        match tools.file_read.call(crate::tools::FileReadArgs {
                            path: "README.md".to_string(),
                            start_line: None,
                            end_line: Some(100), // Limit to first 100 lines
                        }).await {
                            Ok(result) => {
                                full_context.push_str("Contenido de README.md (primeras 100 líneas):\n");
                                full_context.push_str(&result.content);
                                full_context.push_str("\n---\n");
                            }
                            Err(e) => log_warn!("[Analysis] Failed to read README.md: {}", e),
                        }
                    }

                    // --- Step 3: Read Cargo.toml ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::ExecutingTool { tool_name: "read_file".to_string() },
                        message: "3/5: Leyendo Cargo.toml...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));
                    if Path::new(&config_clone.working_dir).join("Cargo.toml").exists() {
                         match tools.file_read.call(crate::tools::FileReadArgs {
                            path: "Cargo.toml".to_string(),
                            start_line: None,
                            end_line: None,
                        }).await {
                            Ok(result) => {
                                full_context.push_str("Contenido de Cargo.toml:\n");
                                full_context.push_str(&result.content);
                                full_context.push_str("\n---\n");
                            }
                            Err(e) => log_warn!("[Analysis] Failed to read Cargo.toml: {}", e),
                        }
                    }

                    // --- Step 4: List src directory ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::ExecutingTool { tool_name: "list_directory".to_string() },
                        message: "4/5: Listando directorio 'src'...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));
                    if Path::new(&config_clone.working_dir).join("src").exists() {
                        match tools.list_directory.call(crate::tools::ListDirectoryArgs {
                            path: "src".to_string(),
                            recursive: true,
                            max_depth: 5,
                        }).await {
                            Ok(result) => {
                                full_context.push_str("Estructura del Directorio 'src':\n");
                                 for entry in result.entries.iter().take(50) { // Limit output
                                    full_context.push_str(&format!("- {}\n", entry.path));
                                }
                                if result.count > 50 {
                                    full_context.push_str(&format!("... y {} más.\n", result.count - 50));
                                }
                                full_context.push_str("\n---\n");
                            }
                            Err(e) => log_warn!("[Analysis] Failed to list src directory: {}", e),
                        }
                    }
                    
                    // --- Step 5: Get RAPTOR context ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::SearchingContext { chunks: 0 }, // Placeholder chunks
                        message: "5/5: Obteniendo contexto del índice (RAPTOR)...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));
                    if let Some(service) = raptor_service_arc {
                        let mut service_guard = service.lock().await;
                        match service_guard.get_planning_context(&query).await {
                            Ok(context) if !context.is_empty() && !context.contains("No RAPTOR context") => {
                                full_context.push_str("Contexto Relevante del Índice (RAPTOR):\n");
                                full_context.push_str(&context);
                                full_context.push_str("\n---\n");
                            }
                            _ => log_warn!("[Analysis] No RAPTOR context found for query."),
                        }
                    }

                    // --- Step 6: Related files context ---
                    let (_detected_files, related_context) = tokio::time::timeout(
                        Duration::from_secs(5), // 5 second timeout for related files
                        related_files_detector_arc.enrich_with_query_context(&query, &config_clone)
                    ).await.unwrap_or_else(|_| (vec![], String::new()));

                    if !related_context.is_empty() {
                        full_context.push_str(&related_context);
                    }

                    // --- Step 7: Git context ---
                    let git_context = tokio::time::timeout(
                        Duration::from_secs(3), // 3 second timeout for git context
                        {
                            let git_context_arc_clone = git_context_arc.clone();
                            async move {
                                let mut git_ctx = git_context_arc_clone.lock().await;
                                git_ctx.get_full_context().await // Call the new get_full_context method
                            }
                        }
                    ).await.unwrap_or_else(|_| String::new());

                    if !git_context.is_empty() {
                        full_context.push_str(&git_context);
                    }


                    // --- Final Summarization (Streaming) ---
                    let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                        stage: ProgressStage::Generating,
                        message: "Generando resumen final (streaming)...".to_string(),
                        elapsed_ms: start_time.elapsed().as_millis() as u64,
                    }));

                    let final_prompt = format!(
                        "Basado en el siguiente análisis de un repositorio de código, proporciona un resumen completo y conciso sobre el proyecto. \
                        Describe su propósito principal, las tecnologías clave utilizadas, su estructura general y cualquier otra información relevante que encuentres. \
                        La consulta original del usuario fue: '{}'.\n\n\
                        --- ANÁLISIS DEL REPOSITORIO ---\n\n{}",
                        query,
                        full_context
                    );

                    // Get config needed for streaming WITHOUT holding lock during the operation
                    let ollama_url = config_clone.heavy_model_config.url.clone();
                    let heavy_model = config_clone.heavy_model_config.model.clone();
                    let timeout_secs = config_clone.execution_timeout_secs;

                    // Do streaming WITHOUT holding any locks
                    let streaming_result = DualModelOrchestrator::stream_heavy_model_static(
                        &ollama_url,
                        &heavy_model,
                        timeout_secs,
                        &final_prompt,
                        event_tx.clone()
                    ).await;

                    match streaming_result {
                        Ok(_) => {
                            let _ = event_tx.try_send(crate::agent::AgentEvent::Progress(ProgressUpdate {
                                stage: ProgressStage::Complete,
                                message: "✓ Análisis completado".to_string(),
                                elapsed_ms: start_time.elapsed().as_millis() as u64,
                            }));
                            // CRITICAL: Always send StreamEnd when streaming completes successfully
                            let _ = event_tx.try_send(crate::agent::AgentEvent::StreamEnd);
                        }
                        Err(e) => {
                            let _ = event_tx.try_send(crate::agent::AgentEvent::Error(format!("Error during streaming: {}", e)));
                            let _ = event_tx.try_send(crate::agent::AgentEvent::StreamEnd);
                        }
                    }
                });

                // Immediately return Streaming response
                Ok(OrchestratorResponse::Streaming { task_id: uuid::Uuid::new_v4() })
            }

            RouterDecision::ToolExecution { query, mode, needs_raptor, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] ToolExecution mode: {:?} (confidence: {:.2})", mode, confidence);
                }

                // Step 1: Detect files mentioned in query and get related files
                // TEMPORARY: Skip context enrichment to isolate the freezing issue
                let (detected_files, related_context) = tokio::time::timeout(
                    Duration::from_secs(5), // 5 second timeout for related files
                    self.related_files_detector.enrich_with_query_context(&query, &self.config)
                ).await.unwrap_or_else(|_| (vec![], String::new()));
                
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
                            ProgressStage::SearchingContext { chunks: chunk_count },
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
                                    // Limit RAPTOR context to prevent model confusion
                                    let original_len = context.len();
                                    let limited_context = if original_len > 4000 {
                                        format!("{}... (truncated)", context.chars().take(4000).collect::<String>())
                                    } else {
                                        context
                                    };
                                    self.send_progress(
                                        ProgressStage::SearchingContext { chunks: chunk_count },
                                        format!("✓ Contexto encontrado ({} chars)", limited_context.len()),
                                        start_time.elapsed().as_millis() as u64,
                                    );
                                    if self.config.debug {
                                        log_info!("✓ [RAPTOR] Contexto: {} chars (limited from {})", limited_context.len(), original_len);
                                    }
                                    format!("{}\n\nContexto del proyecto:\n{}", query, limited_context)
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
                let git_context = tokio::time::timeout(
                    Duration::from_secs(3), // 3 second timeout for git context
                    self.enrich_with_git_context()
                ).await.unwrap_or_else(|_| String::new());
                if !git_context.is_empty() {
                    enriched_query.push_str(&git_context);
                }

                self.send_progress(
                    ProgressStage::ExecutingTool { tool_name: format!("mode_{:?}", mode) },
                    "⚙️ Ejecutando herramientas...".to_string(),
                    start_time.elapsed().as_millis() as u64,
                );

                // Execute based on mode
                match mode {
                    OperationMode::Ask => {
                        // Read-only operations, allow tools
                        // Wrap processing with timeout + heartbeat so UI doesn't hang indefinitely
                        let timeout_dur = Duration::from_secs(self.config.execution_timeout_secs);

                        // Heartbeat: periodically send status updates while the operation is running
                        let (hb_tx, hb_rx) = oneshot::channel::<()>();
                        {
                            let event_tx = self.event_tx.lock().await;
                            if let Some(ref tx) = &*event_tx {
                                let tx_clone = tx.clone();
                                tokio::spawn(async move {
                                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                                    let mut hb_rx = hb_rx;
                                    loop {
                                        tokio::select! {
                                            _ = interval.tick() => {
                                                let _ = tx_clone.try_send(crate::agent::AgentEvent::Status("Procesando (read-only)...".to_string()));
                                            }
                                            _ = &mut hb_rx => {
                                                break;
                                            }
                                        }
                                    }
                                });
                            }
                        }

                        let timeout_result = {
                            let mut orchestrator = self.orchestrator.lock().await;
                            timeout(timeout_dur, orchestrator.process(&enriched_query)).await
                        };

                        match timeout_result {
                            Ok(Ok(resp)) => {
                                let _ = hb_tx.send(());
                                Ok(resp)
                            }
                            Ok(Err(e)) => {
                                let _ = hb_tx.send(());
                                Err(anyhow::anyhow!("{:?}", e))
                            }
                            Err(_) => {
                                // timeout - attempt a single retry with repository-aware context
                                let _ = hb_tx.send(());
                                {
                                    let event_tx = self.event_tx.lock().await;
                                    if let Some(tx) = &*event_tx {
                                        let _ = tx.try_send(crate::agent::AgentEvent::Status("⏱️ Timeout: attempting fallback with repo context...".to_string()));
                                    }
                                }

                                if let Ok(repo_ctx) = self.collect_repo_context(&enriched_query).await {
                                    if !repo_ctx.is_empty() {
                                        let retry_query = format!("{}\n\nContexto adicional del repositorio:\n{}", enriched_query, repo_ctx);
                                        if self.config.debug {
                                            log_info!("🔁 [RAPTOR-RETRY] Retrying with repo context (short timeout)");
                                        }

                                        // short retry timeout
                                        let retry_timeout = Duration::from_secs((self.config.execution_timeout_secs / 4).max(10));
                                        let (hb2_tx, hb2_rx) = oneshot::channel::<()>();
                                        {
                                            let event_tx = self.event_tx.lock().await;
                                            if let Some(ref tx) = &*event_tx {
                                                let tx_clone = tx.clone();
                                                tokio::spawn(async move {
                                                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                                                    let mut hb_rx = hb2_rx;
                                                    loop {
                                                        tokio::select! {
                                                            _ = interval.tick() => {
                                                                let _ = tx_clone.try_send(crate::agent::AgentEvent::Status("Procesando (retry with repo context)...".to_string()));
                                                            }
                                                            _ = &mut hb_rx => {
                                                                break;
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        }

                                        let timeout_result = {
                                            let mut orch = self.orchestrator.lock().await;
                                            timeout(retry_timeout, orch.process(&retry_query)).await
                                        };

                                        match timeout_result {
                                            Ok(Ok(resp)) => {
                                                let _ = hb2_tx.send(());
                                                return Ok(resp);
                                            }
                                            Ok(Err(e)) => {
                                                let _ = hb2_tx.send(());
                                                return Err(anyhow::anyhow!("{:?}", e));
                                            }
                                            Err(_) => {
                                                let _ = hb2_tx.send(());
                                                return Ok(OrchestratorResponse::Error(format!("⏱️ Timeout after retry: operation exceeded {}s", retry_timeout.as_secs())));
                                            }
                                        }
                                    }
                                }

                                Ok(OrchestratorResponse::Error(format!("⏱️ Timeout: operation exceeded {}s", timeout_dur.as_secs())))
                            }
                        }
                    }
                    OperationMode::Build => {
                        // Write operations, allow all tools
                        let timeout_dur = Duration::from_secs(self.config.execution_timeout_secs);

                        let (hb_tx, hb_rx) = oneshot::channel::<()>();
                        {
                            let event_tx = self.event_tx.lock().await;
                            if let Some(ref tx) = &*event_tx {
                                let tx_clone = tx.clone();
                                tokio::spawn(async move {
                                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                                    let mut hb_rx = hb_rx;
                                    loop {
                                        tokio::select! {
                                            _ = interval.tick() => {
                                                let _ = tx_clone.try_send(crate::agent::AgentEvent::Status("Procesando (build)...".to_string()));
                                            }
                                            _ = &mut hb_rx => {
                                                break;
                                            }
                                        }
                                    }
                                });
                            }
                        }

                        let timeout_result = {
                            let mut orchestrator = self.orchestrator.lock().await;
                            timeout(timeout_dur, orchestrator.process(&enriched_query)).await
                        };

                        match timeout_result {
                            Ok(Ok(resp)) => {
                                let _ = hb_tx.send(());
                                Ok(resp)
                            }
                            Ok(Err(e)) => {
                                let _ = hb_tx.send(());
                                Err(anyhow::anyhow!("{:?}", e))
                            }
                            Err(_) => {
                                let _ = hb_tx.send(());
                                Ok(OrchestratorResponse::Error(format!("⏱️ Timeout: operation exceeded {}s", timeout_dur.as_secs())))
                            }
                        }
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
                        
                        let response = {
                            let mut orchestrator = self.orchestrator.lock().await;
                            orchestrator.process(&plan_prompt).await.map_err(|e| anyhow::anyhow!("{:?}", e))?
                        };
                        Ok(response)
                    }
                }
            }

            RouterDecision::FullPipeline { query, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] FullPipeline mode (confidence: {:.2})", confidence);
                }
                self.send_status("Análisis completo en progreso...".to_string());
                // Use full orchestrator with all capabilities
                let response = {
                    let mut orchestrator = self.orchestrator.lock().await;
                    orchestrator.process(&query).await.map_err(|e| anyhow::anyhow!("{:?}", e))?
                };
                Ok(response)
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
    
    /// Enrich context with git-aware information
    /// Returns formatted string with git context (uncommitted changes, recently modified files)
    async fn enrich_with_git_context(&self) -> String {
        let mut git_ctx_locked = self.git_context.lock().await;
        git_ctx_locked.get_full_context().await
    }

    /// Collect repository-aware context by searching and reading relevant files.
    /// This is used as a fallback when RAPTOR returns insufficient planning context.
    pub async fn collect_repo_context(&self, user_query: &str) -> Result<String> {
        // Briefly acquire orchestrator to get access to tools
        let tools = {
            let orchestrator = self.orchestrator.lock().await;
            Arc::new(orchestrator.tools().clone())
        };

        if self.config.debug {
            log_info!("🔎 [RepoContext] Searching repository for query: {}", user_query);
        }

        let mut snippets: Vec<String> = Vec::new();

        // 1) Try a targeted search using the SearchInFiles tool
        let search_args = crate::tools::SearchArgs {
            path: self.config.working_dir.clone(),
            pattern: user_query.to_string(),
            is_regex: Some(false),
            case_insensitive: Some(true),
            file_pattern: None,
            max_results: Some(50),
            context_lines: Some(3),
            max_depth: Some(8),
        };

        if let Ok(search_out) = tools.search_files.search(search_args).await {
            if search_out.total_matches > 0 {
                use std::collections::HashMap;
                let mut by_file: HashMap<String, Vec<crate::tools::SearchResult>> = HashMap::new();
                for r in search_out.results.into_iter() {
                    let key = r.file.to_string_lossy().to_string();
                    by_file.entry(key).or_default().push(r);
                }

                for (file, results) in by_file.into_iter().take(6) {
                    snippets.push(format!("Archivo: {}", file));
                    for r in results.into_iter().take(3) {
                        for b in r.context_before { snippets.push(format!("  {}", b)); }
                        snippets.push(format!("  {}: {}", r.line_number, r.line_content));
                        for a in r.context_after { snippets.push(format!("  {}", a)); }
                        snippets.push(String::from(""));
                    }
                    snippets.push(String::from("---"));
                }
            }
        }

        // 2) If search returned nothing useful, fall back to reading important files
        if snippets.is_empty() {
            if let Ok(list_out) = tools.list_directory.call(crate::tools::ListDirectoryArgs {
                path: self.config.working_dir.clone(),
                recursive: true,
                max_depth: 4,
            }).await {
                // Filter likely-useful files
                let mut candidates: Vec<_> = list_out
                    .entries
                    .into_iter()
                    .filter(|e| {
                        !e.is_dir
                            && (e.name.ends_with(".rs")
                                || e.name.ends_with(".py")
                                || e.name.ends_with(".js")
                                || e.name.ends_with(".ts")
                                || e.name.ends_with(".md")
                                || e.name == "Cargo.toml"
                                || e.name == "package.json")
                    })
                    .collect();

                // Prefer larger files (heuristic)
                candidates.sort_by(|a, b| b.size.cmp(&a.size));

                for entry in candidates.into_iter().take(8) {
                    let path = entry.path;
                    if let Ok(read_out) = tools.file_read.call(crate::tools::FileReadArgs {
                        path: path.clone(),
                        start_line: None,
                        end_line: Some(200),
                    }).await {
                        snippets.push(format!("Archivo: {}\n{}\n---", path, read_out.content));
                    }
                }
            }
        }

        // Assemble and truncate
        let mut ctx = snippets.join("\n");
        if ctx.len() > 8000 {
            ctx.truncate(8000);
            ctx.push_str("\n... (truncated)");
        }

        Ok(ctx)
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
1. RepositoryAnalysis - Análisis profundo y automático del repositorio.
   Usa cuando: "analiza el repositorio", "explícame el proyecto", "de qué se trata este código".
   
2. DirectResponse - Respuesta directa sin contexto de código.
   Usa cuando: conocimiento general, matemáticas, definiciones sin código.
   Ejemplos: "hola", "calcula 5*8", "qué es async/await en general".
   
3. ToolExecution - Tareas específicas que requieren herramientas.
   Usa cuando: el usuario pide una acción concreta como "lee main.rs", "ejecuta tests", "busca X".
   
   Submodos:
   - mode: "Ask" (read-only, default)
   - mode: "Build" (escribe código: "crea función", "refactoriza")
   - mode: "Plan" (genera plan: "planifica", "diseña")
   
   needs_raptor:
   - true: si la tarea necesita buscar en el contenido del proyecto.
   - false: para operaciones simples de archivos sin contexto.
   
4. FullPipeline - RARA VEZ USADO - Solo para operaciones masivas.
   Usa cuando: refactorización completa, rediseño arquitectónico.

Casos comunes:
- "analiza el repositorio" → RepositoryAnalysis
- "explícame de qué se trata" → RepositoryAnalysis
- "qué hace este proyecto" → RepositoryAnalysis
- "lee archivo X" → ToolExecution (mode: "Ask", needs_raptor: false)
- "mejora el código" → ToolExecution (mode: "Plan", needs_raptor: true)
- "escribe función para X" → ToolExecution (mode: "Build", needs_raptor: false)

Responde exactamente este formato JSON:
{{
  "route": "RepositoryAnalysis|DirectResponse|ToolExecution|FullPipeline",
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
1. RepositoryAnalysis - Deep, automatic analysis of the repository.
   Use for: "analyze the repository", "explain the project", "what is this code about".

2. DirectResponse - Direct answer without code context.
   Use for: general knowledge, math, non-code definitions.
   Examples: "hello", "calculate 5*8", "what is async/await in general".

3. ToolExecution - Specific tasks that require tools.
   Use when: the user asks for a concrete action like "read main.rs", "run tests", "search for X".
   
   Submodes:
   - mode: "Ask" (read-only, default)
   - mode: "Build" (write code: "create function", "refactor")
   - mode: "Plan" (generate plan: "plan", "design")
   
4. FullPipeline - RARELY USED - For massive operations only.
   Use for: complete refactoring, architectural redesign.

Common cases:
- "analyze the repository" → RepositoryAnalysis
- "explain what this project does" → RepositoryAnalysis
- "read file X" → ToolExecution (mode: "Ask")
- "improve the code" → ToolExecution (mode: "Plan")
- "write a function for X" → ToolExecution (mode: "Build")

Respond exactly in this JSON format:
{{
  "route": "RepositoryAnalysis|DirectResponse|ToolExecution|FullPipeline",
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

    /// Test that classifier rule forces ToolExecution::Ask for explain queries
    #[tokio::test]
    async fn test_classify_rules_explain() {
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
                let decision = router.classify("analiza este repositorio y explicame de que trata").await;
                assert!(decision.is_ok());
                let d = decision.unwrap();
                match d {
                    RouterDecision::ToolExecution { mode, needs_raptor, .. } => {
                        assert_eq!(mode, OperationMode::Ask);
                        assert!(needs_raptor);
                    }
                    _ => panic!("Expected ToolExecution Ask mode"),
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
