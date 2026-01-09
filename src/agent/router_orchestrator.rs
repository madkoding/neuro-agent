//! Router Orchestrator - Simplified routing system for efficient model usage
//!
//! This module implements a lightweight router that classifies user requests BEFORE
//! executing any heavy operations. Optimized for small context window models.

#![allow(deprecated)]

use super::orchestrator::{DualModelOrchestrator, OrchestratorResponse};
use super::slash_commands::{SlashCommandRegistry, CommandContext};
use super::state::SharedState;
use crate::agent::provider::OllamaProvider;
use crate::config::{ModelConfig, ModelProvider as ProviderType};
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
    slash_commands: SlashCommandRegistry,
}

impl RouterOrchestrator {
    /// Create new router orchestrator with configuration
    pub async fn new(
        config: RouterConfig,
        orchestrator: DualModelOrchestrator,
    ) -> Result<Self> {
        let state = orchestrator.state();
        let orchestrator_arc = Arc::new(AsyncMutex::new(orchestrator));
        
        Ok(Self {
            config,
            orchestrator: orchestrator_arc.clone(),
            raptor_service: Some(Arc::new(AsyncMutex::new(
                RaptorContextService::new(orchestrator_arc),
            ))),
            full_index_ready: Arc::new(AtomicBool::new(false)),
            state,
            status_tx: None,
            slash_commands: SlashCommandRegistry::new(),
        })
    }

    /// Set status channel for sending progress updates to UI
    pub fn set_status_channel(&mut self, tx: Sender<String>) {
        self.status_tx = Some(tx);
    }

    /// Send status update to UI if channel is available
    fn send_status(&self, message: String) {
        if let Some(tx) = &self.status_tx {
            let _ = tx.try_send(message);
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

    /// Classify user query using fast model
    pub async fn classify(&self, user_query: &str) -> Result<RouterDecision> {
        self.send_status("Clasificando consulta...".to_string());
        
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
            return Ok(RouterDecision::ToolExecution {
                query: user_query.to_string(),
                mode: OperationMode::Ask,
                needs_raptor: true,
                confidence: classification.confidence,
            });
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
                    match action.as_str() {
                        "reindex" => {
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
                        _ => {}
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
        // Check for slash commands first
        if let Some(response) = self.handle_slash_command(user_query).await? {
            return Ok(response);
        }

        // Classify query
        let decision = self.classify(user_query).await?;

        match decision {
            RouterDecision::DirectResponse { query, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] DirectResponse mode (confidence: {:.2})", confidence);
                }
                self.send_status("Generando respuesta directa...".to_string());
                // Use orchestrator directly without tools
                let mut orchestrator = self.orchestrator.lock().await;
                orchestrator.process(&query).await.map_err(|e| anyhow::anyhow!("{:?}", e))
            }

            RouterDecision::ToolExecution { query, mode, needs_raptor, confidence } => {
                if self.config.debug {
                    log_info!("[ROUTER] ToolExecution mode: {:?} (confidence: {:.2})", mode, confidence);
                }

                // Enrich with RAPTOR context if needed
                let enriched_query = if needs_raptor && self.raptor_service.is_some() {
                    if has_quick_index() || has_full_index() {
                        let chunk_count = {
                            let store = GLOBAL_STORE.lock().unwrap();
                            store.chunk_map.len()
                        };
                        
                        self.send_status(format!("Buscando contexto ({} chunks)...", chunk_count));
                        
                        if self.config.debug {
                            log_debug!("🔍 [RAPTOR] Buscando contexto...");
                        }
                        if let Some(service) = &self.raptor_service {
                            let mut service_guard = service.lock().await;
                            match service_guard.get_planning_context(&query).await {
                                Ok(context) if !context.is_empty() => {
                                    self.send_status(format!("Contexto encontrado ({} chars)", context.len()));
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

                self.send_status("Procesando con herramientas...".to_string());

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
    pub fn is_full_index_ready(&self) -> bool {
        self.full_index_ready.load(Ordering::SeqCst)
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
