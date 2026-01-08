//! Planning orchestrator - Multi-step task execution with context accumulation
//! 
//! DEPRECATED: Use RouterOrchestrator instead. This module will be removed in v2.0 (target: Feb 2026)

use super::classifier::TaskClassifier;
use super::orchestrator::{DualModelOrchestrator, OrchestratorResponse};
use super::router::{ExecutionPlan, ExecutionStep, IntelligentRouter};
use super::state::SharedState;
use crate::context::cache::ProjectContextCacheManager;
use crate::raptor::builder::RaptorBuildProgress;
use crate::raptor::integration::RaptorContextService;
use crate::tools::{PlanStatus, Task, TaskPlan, TaskStatus, ToolRegistry};
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Progress callback for task execution
pub type ProgressCallback = Box<dyn Fn(TaskProgressInfo) + Send + Sync>;

/// Task progress information
#[derive(Debug, Clone)]
pub struct TaskProgressInfo {
    pub task_index: usize,
    pub total_tasks: usize,
    pub description: String,
    pub status: TaskProgressStatus,
}

/// Task progress status
#[derive(Debug, Clone)]
pub enum TaskProgressStatus {
    Started,
    Completed(String),
    Failed(String),
}

/// Planning orchestrator that wraps DualModelOrchestrator with multi-step planning
pub struct PlanningOrchestrator {
    /// Underlying orchestrator
    orchestrator: Arc<Mutex<DualModelOrchestrator>>,
    /// Intelligent router for execution plans
    router: IntelligentRouter,
    /// Project context cache
    context_cache: ProjectContextCacheManager,
    /// Task classifier for routing
    classifier: TaskClassifier,
    /// Tool registry
    tools: Arc<ToolRegistry>,
    // REMOVED: self_correction field (module deleted)
    // /// Self-correction loop
    // self_correction: Arc<SelfCorrectionLoop>,
    /// Shared state
    state: SharedState,
    /// RAPTOR context service for semantic search
    raptor_service: Option<RaptorContextService>,
    /// Working directory
    working_dir: PathBuf,
    /// Whether RAPTOR is initialized
    raptor_initialized: bool,
}

/// Response from planning orchestrator
#[derive(Debug, Clone)]
pub enum PlanningResponse {
    /// Simple response (no planning needed)
    Simple(OrchestratorResponse),
    /// Plan created and being executed
    PlanStarted {
        plan_id: Uuid,
        goal: String,
        total_tasks: usize,
    },
    /// Plan completed
    PlanCompleted {
        plan_id: Uuid,
        result: String,
        tasks_completed: usize,
    },
    /// Plan failed
    PlanFailed {
        plan_id: Uuid,
        error: String,
        tasks_completed: usize,
    },
    /// Task in plan completed
    TaskCompleted {
        plan_id: Uuid,
        task_index: usize,
        total_tasks: usize,
    },
}

impl PlanningOrchestrator {
    pub fn new(
        orchestrator: Arc<Mutex<DualModelOrchestrator>>,
        tools: Arc<ToolRegistry>,
        state: SharedState,
        working_dir: PathBuf,
    ) -> Self {
        // Crear servicio RAPTOR
        let raptor_service = Some(RaptorContextService::new(orchestrator.clone()));

        Self {
            orchestrator: orchestrator.clone(),
            router: IntelligentRouter::new(),
            context_cache: ProjectContextCacheManager::new(working_dir.clone()),
            classifier: TaskClassifier::new(),
            tools,
            // REMOVED: self_correction initialization (module deleted)
            // self_correction: Arc::new(SelfCorrectionLoop::new()),
            state,
            raptor_service,
            working_dir,
            raptor_initialized: false,
        }
    }

    /// Initialize RAPTOR index for the working directory
    pub async fn initialize_raptor(&mut self) -> Result<bool> {
        self.initialize_raptor_with_progress(None).await
    }

    /// Initialize RAPTOR index for the working directory with progress updates
    pub async fn initialize_raptor_with_progress(
        &mut self,
        progress_tx: Option<Sender<TaskProgressInfo>>,
    ) -> Result<bool> {
        if self.raptor_initialized {
            return Ok(true);
        }

        if let Some(ref mut raptor) = self.raptor_service {
            // Check if there's already an index
            if raptor.has_context() {
                self.raptor_initialized = true;
                return Ok(true);
            }

            // Build index for working directory
            let path = self.working_dir.to_string_lossy().to_string();
            tracing::info!("Building RAPTOR index for: {}", path);

            // Create a channel to convert RaptorBuildProgress to TaskProgressInfo
            let (raptor_tx, mut raptor_rx) = tokio::sync::mpsc::channel::<RaptorBuildProgress>(100);

            // Clone progress_tx for the forwarding task
            let progress_tx_clone = progress_tx.clone();

            // Spawn a task to forward RAPTOR progress to TaskProgressInfo
            let forward_handle = tokio::spawn(async move {
                while let Some(raptor_progress) = raptor_rx.recv().await {
                    if let Some(ref tx) = progress_tx_clone {
                        let progress_msg = format!(
                            "{}: {} ({}/{})",
                            raptor_progress.stage,
                            raptor_progress.detail,
                            raptor_progress.current,
                            raptor_progress.total
                        );
                        let _ = tx
                            .send(TaskProgressInfo {
                                task_index: 0,
                                total_tasks: 1,
                                description: progress_msg,
                                status: if raptor_progress.stage == "Completado" {
                                    TaskProgressStatus::Completed(String::new())
                                } else {
                                    TaskProgressStatus::Started
                                },
                            })
                            .await;
                    }
                }
            });

            // Larger chunks (2000 chars) and higher threshold (0.5) for faster indexing
            let result = raptor
                .build_tree_with_progress(&path, Some(2500), Some(0.5), Some(raptor_tx))
                .await;

            // Wait for forwarding task to finish
            let _ = forward_handle.await;

            match result {
                Ok(root_id) => {
                    tracing::info!("RAPTOR index built successfully, root: {}", root_id);
                    self.raptor_initialized = true;
                    Ok(true)
                }
                Err(e) => {
                    tracing::warn!("Failed to build RAPTOR index: {}", e);
                    if let Some(ref tx) = progress_tx {
                        let _ = tx
                            .send(TaskProgressInfo {
                                task_index: 0,
                                total_tasks: 1,
                                description: format!("Error RAPTOR: {}", e),
                                status: TaskProgressStatus::Failed(e.to_string()),
                            })
                            .await;
                    }
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }

    /// Get context from RAPTOR for a query
    async fn get_raptor_context(&mut self, query: &str) -> Option<String> {
        if let Some(ref mut raptor) = self.raptor_service {
            if raptor.has_context() {
                match raptor.get_planning_context(query).await {
                    Ok(ctx) if !ctx.is_empty() => return Some(ctx),
                    _ => {}
                }
            }
        }
        None
    }

    /// Process user input with planning capability
    pub async fn process_with_planning(&mut self, input: &str) -> Result<PlanningResponse> {
        self.process_with_planning_and_progress(input, None).await
    }

    /// Process user input with planning capability and progress updates
    pub async fn process_with_planning_and_progress(
        &mut self,
        input: &str,
        progress_tx: Option<Sender<TaskProgressInfo>>,
    ) -> Result<PlanningResponse> {
        // Classify query complexity first
        let complexity = self.classifier.classify_complexity(input);

        tracing::info!("Query classified as: {:?}", complexity);

        match complexity {
            // General queries: Direct LLM response without any indexing
            super::classifier::QueryComplexity::General => {
                tracing::info!("Processing as General query (no code context)");
                let response = self
                    .orchestrator
                    .lock()
                    .await
                    .process(input)
                    .await?;
                return Ok(PlanningResponse::Simple(response));
            }

            // CodeContext: ALL code queries use RAPTOR for full project context
            super::classifier::QueryComplexity::CodeContext => {
                tracing::info!("Processing as CodeContext query (with RAPTOR)");
                
                // Initialize RAPTOR if not done yet
                if !self.raptor_initialized {
                    // Send initial progress about RAPTOR initialization
                    if let Some(ref tx) = progress_tx {
                        let _ = tx
                            .send(TaskProgressInfo {
                                task_index: 0,
                                total_tasks: 1,
                                description: "📊 Inicializando índice RAPTOR...".to_string(),
                                status: TaskProgressStatus::Started,
                            })
                            .await;
                    }

                    let _ = self
                        .initialize_raptor_with_progress(progress_tx.clone())
                        .await;

                    // Small delay to let UI update
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                // Continue with planning below
            }
        }

        // For CodeContext: Check if planning is needed
        if !self.should_plan(input).await {
            // Simple processing without planning - enrich with RAPTOR context
            let raptor_context = self.get_raptor_context(input).await;
            let enriched_input = if let Some(ctx) = raptor_context {
                format!("{}\n\nContexto del proyecto:\n{}", input, ctx)
            } else {
                input.to_string()
            };

            let response = self
                .orchestrator
                .lock()
                .await
                .process(&enriched_input)
                .await?;
            return Ok(PlanningResponse::Simple(response));
        }

        // Generate plan
        let plan = self.generate_plan(input).await?;
        let plan_id = Uuid::parse_str(&plan.id).unwrap_or_else(|_| Uuid::new_v4());
        let total_tasks = plan.tasks.len();
        let _goal = plan.goal.clone();

        // Store plan in state
        {
            let mut state = self.state.lock().await;
            state.store_plan(plan.clone());
        }

        // Execute plan with progress updates
        let result = self.execute_plan_with_progress(plan_id, progress_tx).await;

        match result {
            Ok(final_result) => Ok(PlanningResponse::PlanCompleted {
                plan_id,
                result: final_result,
                tasks_completed: total_tasks,
            }),
            Err(e) => Ok(PlanningResponse::PlanFailed {
                plan_id,
                error: e.to_string(),
                tasks_completed: 0,
            }),
        }
    }

    /// Execute plan with optional progress updates
    async fn execute_plan_with_progress(
        &mut self,
        plan_id: Uuid,
        progress_tx: Option<Sender<TaskProgressInfo>>,
    ) -> Result<String> {
        let mut accumulated_context = HashMap::new();
        let mut results = Vec::new();

        // Guardar el goal para la síntesis final
        let plan_goal = {
            let state = self.state.lock().await;
            state
                .get_plan(&plan_id)
                .map(|p| p.goal.clone())
                .unwrap_or_default()
        };

        loop {
            // Get next task to execute
            let task_info = {
                let mut state = self.state.lock().await;
                let plan = state
                    .get_plan_mut(&plan_id)
                    .ok_or_else(|| anyhow::anyhow!("Plan not found"))?;

                // Check if all tasks are done
                let all_done = plan.tasks.iter().all(|t| {
                    matches!(
                        t.status,
                        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped
                    )
                });

                if all_done {
                    plan.status = PlanStatus::Completed;
                    drop(state);
                    if !accumulated_context.is_empty() {
                        match self
                            .generate_final_synthesis(&plan_goal, &accumulated_context)
                            .await
                        {
                            Ok(synthesis) => {
                                return Ok(format!(
                                    "{}\n\n---\n\n{}",
                                    self.summarize_results(&results),
                                    synthesis
                                ))
                            }
                            Err(_) => return Ok(self.summarize_results(&results)),
                        }
                    }
                    return Ok(self.summarize_results(&results));
                }

                // Get next pending task
                let task_index = plan.tasks.iter().position(|t| {
                    t.status == TaskStatus::Pending && self.are_dependencies_met(plan, t)
                });

                match task_index {
                    Some(idx) => {
                        let task = plan.tasks[idx].clone();
                        plan.tasks[idx].status = TaskStatus::InProgress;
                        plan.current_task_index = idx;
                        Some((task, idx, plan.tasks.len()))
                    }
                    None => {
                        plan.status = PlanStatus::Completed;
                        let goal = plan.goal.clone();
                        drop(state);
                        if !accumulated_context.is_empty() {
                            match self
                                .generate_final_synthesis(&goal, &accumulated_context)
                                .await
                            {
                                Ok(synthesis) => {
                                    return Ok(format!(
                                        "{}\n\n---\n\n{}",
                                        self.summarize_results(&results),
                                        synthesis
                                    ))
                                }
                                Err(_) => return Ok(self.summarize_results(&results)),
                            }
                        }
                        return Ok(self.summarize_results(&results));
                    }
                }
            };

            let (task, task_index, total_tasks) = match task_info {
                Some(info) => info,
                None => break,
            };

            // Send progress: task started
            if let Some(ref tx) = progress_tx {
                let _ = tx
                    .send(TaskProgressInfo {
                        task_index,
                        total_tasks,
                        description: task.description.clone(),
                        status: TaskProgressStatus::Started,
                    })
                    .await;
                // Sleep breve para forzar que el runtime procese el mensaje en el UI
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            // Execute the task
            let result = self.execute_task(&task, &accumulated_context).await;

            // Update task status and store result
            {
                let mut state = self.state.lock().await;
                let plan = state
                    .get_plan_mut(&plan_id)
                    .ok_or_else(|| anyhow::anyhow!("Plan not found"))?;

                match &result {
                    Ok(output) => {
                        plan.tasks[task_index].status = TaskStatus::Completed;
                        plan.tasks[task_index].result = Some(output.clone());
                        accumulated_context.insert(task.id.clone(), output.clone());
                        // Solo guardar descripción en results, no el contenido
                        results.push(format!("✅ {}", task.description));

                        // Send progress: task completed (sin preview)
                        if let Some(ref tx) = progress_tx {
                            let _ = tx
                                .send(TaskProgressInfo {
                                    task_index,
                                    total_tasks,
                                    description: task.description.clone(),
                                    status: TaskProgressStatus::Completed(String::new()),
                                })
                                .await;
                            // Sleep breve para forzar que el runtime procese el mensaje en el UI
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }
                    Err(e) => {
                        plan.tasks[task_index].status = TaskStatus::Failed;
                        plan.tasks[task_index].result = Some(e.to_string());
                        results.push(format!("❌ {}: {}", task.description, e));

                        // Send progress: task failed
                        if let Some(ref tx) = progress_tx {
                            let _ = tx
                                .send(TaskProgressInfo {
                                    task_index,
                                    total_tasks,
                                    description: task.description.clone(),
                                    status: TaskProgressStatus::Failed(e.to_string()),
                                })
                                .await;
                            // Sleep breve para forzar que el runtime procese el mensaje en el UI
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }

                        // Marcar tareas dependientes como saltadas
                        let failed_task_id = task.id.clone();
                        for t in plan.tasks.iter_mut() {
                            if t.dependencies.contains(&failed_task_id)
                                && t.status == TaskStatus::Pending
                            {
                                t.status = TaskStatus::Skipped;
                                t.error = Some(format!(
                                    "Saltada: dependencia '{}' falló",
                                    failed_task_id
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Síntesis final al terminar el loop - solo mostrar síntesis del LLM
        if !accumulated_context.is_empty() {
            match self
                .generate_final_synthesis(&plan_goal, &accumulated_context)
                .await
            {
                Ok(synthesis) => return Ok(synthesis),
                Err(_) => return Ok("Análisis completado.".to_string()),
            }
        }
        Ok("Tareas completadas.".to_string())
    }

    /// Execute plan synchronously with progress updates to state (legacy)
    #[allow(dead_code)]
    async fn execute_plan_sync(&mut self, plan_id: Uuid) -> Result<String> {
        self.execute_plan_with_progress(plan_id, None).await
    }

    fn summarize_results(&self, results: &[String]) -> String {
        if results.is_empty() {
            return "No se ejecutaron tareas.".to_string();
        }

        let mut summary = String::new();
        for result in results {
            summary.push_str(&format!("{}\n", result));
        }
        summary
    }

    /// Generate a final synthesis using the LLM based on accumulated context
    async fn generate_final_synthesis(
        &self,
        goal: &str,
        accumulated_context: &HashMap<String, String>,
    ) -> Result<String> {
        // Si no hay contexto real, no generar síntesis
        if accumulated_context.is_empty() {
            return Ok("No se pudo obtener información del repositorio.".to_string());
        }

        // Build context from all task results
        let mut context_text = String::new();
        for (task_id, result) in accumulated_context.iter() {
            // Limitar cada resultado a 3000 caracteres para dar más contexto al LLM
            let truncated = if result.len() > 3000 {
                format!("{}... [truncado]", &result[..3000])
            } else {
                result.clone()
            };
            context_text.push_str(&format!(
                "### Resultado de tarea {}:\n{}\n\n",
                task_id, truncated
            ));
        }

        let prompt = format!(
            r#"Eres un asistente de programación que analiza repositorios de código.

IDIOMA: {}

SOLICITUD DEL USUARIO: "{}"

INFORMACIÓN OBTENIDA (no incluir en la respuesta):
{}

INSTRUCCIONES IMPORTANTES:
1. SOLO usa la información de arriba para tu análisis
2. NO copies ni incluyas el contenido de los archivos en tu respuesta
3. NO inventes información que no esté en los datos
4. Haz un RESUMEN CONCISO de lo que encontraste
5. Responde según el idioma configurado
6. Si encontraste archivos, menciona sus nombres pero NO su contenido

Tu análisis resumido:"#,
            crate::i18n::llm_language_instruction(),
            goal,
            context_text,
        );

        let orchestrator = self.orchestrator.lock().await;
        orchestrator
            .call_heavy_model_direct(&prompt)
            .await
            .context("Failed to generate final synthesis")
    }

    /// Determine if input requires planning
    /// ALWAYS plan unless it's a trivial command - the model is not smart enough
    /// to handle complex tasks without breaking them down into tool-based steps
    async fn should_plan(&self, input: &str) -> bool {
        // Use classifier to determine query complexity
        let complexity = self.classifier.classify_complexity(input);

        match complexity {
            // General queries: no planning (no code context)
            super::classifier::QueryComplexity::General => false,
            
            // CodeContext: check if full planning is needed or just RAPTOR context
            // For complex tasks (refactoring, architecture), use planning
            // For simple lookups, just RAPTOR context enrichment
            super::classifier::QueryComplexity::CodeContext => {
                // Heuristic: if query mentions planning keywords, use planning
                let planning_keywords = [
                    "refactor", "reestructura", "restructure",
                    "arquitectura", "architecture",
                    "plan", "strategy", "estrategia",
                    "optimize", "optimiza", "improve", "mejora",
                ];
                planning_keywords.iter().any(|kw| input.to_lowercase().contains(kw))
            }
        }
    }

    /// Generate a task plan using LLM
    async fn generate_plan(&self, goal: &str) -> Result<TaskPlan> {
        let goal_lower = goal.to_lowercase();

        // Para comandos comunes, usar directamente el plan default
        // Esto es más confiable que depender del LLM para generar el plan
        let use_default = goal_lower.contains("analiz")
            || goal_lower.contains("analyz")
            || goal_lower.contains("resum")
            || goal_lower.contains("qué es")
            || goal_lower.contains("que es")
            || goal_lower.contains("qué hace")
            || goal_lower.contains("que hace")
            || goal_lower.contains("explain")
            || goal_lower.contains("lee ")
            || goal_lower.contains("read ")
            || goal_lower.contains("muestr")
            || goal_lower.contains("compil")
            || goal_lower.contains("build")
            || goal_lower.contains("test");

        if use_default {
            tracing::info!("Usando plan por defecto para: {}", goal);
            return Ok(self.create_default_plan(goal));
        }

        // Para otros casos, intentar generar con el LLM
        // First, try to use the router's execution plan
        let intent = self.router.detect_intent(goal);
        let execution_plan = self.router.build_plan(intent, goal);

        // Check if we have cached context we can use
        let context_info = self.gather_cached_context().await;

        // Build a prompt for the heavy model to generate a detailed plan
        let lang_instruction = crate::i18n::llm_language_instruction();
        let prompt = format!(
            r#"Eres un planificador de tareas. Divide el objetivo en pasos ejecutables con herramientas.

IDIOMA: {}

OBJETIVO: {}

CONTEXTO DEL PROYECTO:
{}

PLAN INICIAL SUGERIDO:
{}

IMPORTANTE: El modelo que ejecutará las tareas es limitado y NECESITA usar herramientas para todo.
NO puede responder preguntas complejas sin primero leer archivos, analizar código, o ejecutar comandos.

HERRAMIENTAS DISPONIBLES:
- read_file: Leer contenido de archivos (SIEMPRE usar primero para entender el código)
- list_directory: Ver estructura del proyecto (usar para orientarse)
- execute_shell: Ejecutar comandos (cargo build, cargo test, npm, etc)
- write_file: Escribir/modificar archivos
- search_in_files: Buscar texto en archivos
- analyze_code: Analizar estructura de código
- git_status/git_diff: Ver estado de git

REGLAS:
1. SIEMPRE empezar con list_directory o read_file para obtener contexto
2. Cada tarea DEBE especificar qué herramienta usar
3. Mínimo 2 tareas, máximo 8 tareas
4. Las tareas deben ser concretas y ejecutables

Responde con este formato XML:

<plan>
  <task id="1" tool="list_directory">
    <description>Ver estructura del proyecto para entender la organización</description>
  </task>
  <task id="2" tool="read_file" depends="1">
    <description>Leer el archivo principal para entender el código</description>
  </task>
  <task id="3" tool="execute_shell" depends="2">
    <description>Ejecutar comando específico</description>
  </task>
</plan>

SOLO responde con el XML del plan, nada más."#,
            lang_instruction,
            goal,
            context_info,
            self.format_execution_plan(&execution_plan)
        );

        // Call heavy model
        let orchestrator = self.orchestrator.lock().await;
        let response = orchestrator
            .call_heavy_model_direct(&prompt)
            .await
            .context("Failed to generate plan")?;

        // Parse the plan from response
        self.parse_plan_from_llm_response(&response, goal)
    }

    /// Gather cached context information
    async fn gather_cached_context(&self) -> String {
        let mut context_parts = vec![];

        if let Some(structure) = self.context_cache.get_structure().await {
            context_parts.push(format!(
                "Project: {} ({})",
                structure.language,
                structure.framework.as_deref().unwrap_or("no framework")
            ));
            context_parts.push(format!(
                "Files: {}, Lines: {}",
                structure.total_files, structure.total_lines
            ));
        }

        if let Some(deps) = self.context_cache.get_dependencies().await {
            context_parts.push(format!("Dependencies: {} total", deps.total_count));
        }

        if context_parts.is_empty() {
            "No cached context available. You may need to gather context first.".to_string()
        } else {
            context_parts.join("\n")
        }
    }

    /// Format execution plan for display
    fn format_execution_plan(&self, plan: &ExecutionPlan) -> String {
        let mut result = String::new();
        result.push_str(&format!("Confidence: {}\n", plan.confidence));
        for (i, step) in plan.steps.iter().enumerate() {
            match step {
                ExecutionStep::ToolCall { tool_name, args } => {
                    result.push_str(&format!(
                        "{}. Call tool: {} with args: {:?}\n",
                        i + 1,
                        tool_name,
                        args
                    ));
                }
                ExecutionStep::Reasoning { prompt } => {
                    result.push_str(&format!("{}. Reasoning: {}\n", i + 1, prompt));
                }
            }
        }
        result
    }

    /// Parse plan from LLM XML response with fallback to default plan
    fn parse_plan_from_llm_response(&self, response: &str, goal: &str) -> Result<TaskPlan> {
        use crate::tools::TaskPlannerTool;

        let planner = TaskPlannerTool::new();

        // Try to parse LLM response
        match planner.parse_plan(goal, response) {
            Ok(plan) if self.is_valid_plan(&plan) => Ok(plan),
            _ => {
                // Fallback: create a default plan based on the goal
                tracing::info!("Plan del LLM inválido o incompleto, usando plan por defecto");
                Ok(self.create_default_plan(goal))
            }
        }
    }

    /// Check if a plan is valid (has multiple specific tasks with tools)
    fn is_valid_plan(&self, plan: &TaskPlan) -> bool {
        // Un plan válido debe tener:
        // 1. Más de 1 tarea
        // 2. Las tareas deben tener herramientas asignadas
        // 3. Las descripciones no deben ser muy largas (indicaría que es texto sin parsear)
        if plan.tasks.len() < 2 {
            return false;
        }

        // Al menos 50% de las tareas deben tener herramienta
        let tasks_with_tools = plan
            .tasks
            .iter()
            .filter(|t| t.tool_to_use.is_some())
            .count();
        if tasks_with_tools < plan.tasks.len() / 2 {
            return false;
        }

        // Las descripciones no deben ser muy largas
        for task in &plan.tasks {
            if task.description.len() > 500 {
                return false;
            }
        }

        true
    }

    /// Create a default plan when LLM fails to generate one
    fn create_default_plan(&self, goal: &str) -> TaskPlan {
        use crate::tools::{PlanStatus, Task, TaskEffort, TaskStatus};

        let goal_lower = goal.to_lowercase();
        let mut tasks = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Always start with listing directory to understand project structure
        tasks.push(Task {
            id: "1".to_string(),
            title: "Ver estructura".to_string(),
            description: "Ver la estructura del proyecto para entender la organización".to_string(),
            task_type: crate::tools::planner::TaskType::Research,
            status: TaskStatus::Pending,
            priority: 10,
            estimated_effort: TaskEffort::Trivial,
            dependencies: vec![],
            tool_to_use: Some("list_directory".to_string()),
            tool_args: Some(serde_json::json!({"path": ".", "recursive": false})),
            result: None,
            error: None,
        });

        // Add task based on intent
        if goal_lower.contains("lee")
            || goal_lower.contains("read")
            || goal_lower.contains("muestr")
            || goal_lower.contains("ver")
        {
            // Extract file path if present
            let path = self
                .extract_file_path(goal)
                .unwrap_or_else(|| "src/main.rs".to_string());
            tasks.push(Task {
                id: "2".to_string(),
                title: "Leer archivo".to_string(),
                description: format!("Leer el archivo {}", path),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec!["1".to_string()],
                tool_to_use: Some("read_file".to_string()),
                tool_args: Some(serde_json::json!({"path": path})),
                result: None,
                error: None,
            });
        } else if goal_lower.contains("compil") || goal_lower.contains("build") {
            tasks.push(Task {
                id: "2".to_string(),
                title: "Compilar proyecto".to_string(),
                description: "Compilar el proyecto para verificar errores".to_string(),
                task_type: crate::tools::planner::TaskType::Execution,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Small,
                dependencies: vec!["1".to_string()],
                tool_to_use: Some("execute_shell".to_string()),
                tool_args: Some(serde_json::json!({"command": "cargo build"})),
                result: None,
                error: None,
            });
        } else if goal_lower.contains("test") || goal_lower.contains("prueba") {
            tasks.push(Task {
                id: "2".to_string(),
                title: "Ejecutar tests".to_string(),
                description: "Ejecutar los tests del proyecto".to_string(),
                task_type: crate::tools::planner::TaskType::Testing,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Small,
                dependencies: vec!["1".to_string()],
                tool_to_use: Some("execute_shell".to_string()),
                tool_args: Some(serde_json::json!({"command": "cargo test"})),
                result: None,
                error: None,
            });
        } else if goal_lower.contains("busca")
            || goal_lower.contains("search")
            || goal_lower.contains("find")
        {
            let pattern = self
                .extract_search_pattern(goal)
                .unwrap_or_else(|| "TODO".to_string());
            tasks.push(Task {
                id: "2".to_string(),
                title: "Buscar en código".to_string(),
                description: format!("Buscar '{}' en el código", pattern),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec!["1".to_string()],
                tool_to_use: Some("search_files".to_string()),
                tool_args: Some(
                    serde_json::json!({"pattern": pattern, "path": ".", "recursive": true}),
                ),
                result: None,
                error: None,
            });
        } else if goal_lower.contains("analiz")
            || goal_lower.contains("analyz")
            || goal_lower.contains("resum")
            || goal_lower.contains("explain")
            || goal_lower.contains("qué es")
            || goal_lower.contains("que es")
            || goal_lower.contains("que hace")
            || goal_lower.contains("qué hace")
        {
            // Análisis completo del repositorio - tareas independientes
            tasks.push(Task {
                id: "2".to_string(),
                title: "Leer Cargo.toml".to_string(),
                description: "Leer Cargo.toml para entender dependencias y configuración"
                    .to_string(),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec![], // Sin dependencia para que se ejecute aunque falle otra
                tool_to_use: Some("read_file".to_string()),
                tool_args: Some(serde_json::json!({"path": "Cargo.toml"})),
                result: None,
                error: None,
            });
            tasks.push(Task {
                id: "3".to_string(),
                title: "Leer main.rs".to_string(),
                description: "Leer src/main.rs para entender el punto de entrada".to_string(),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 8,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec![], // Sin dependencia
                tool_to_use: Some("read_file".to_string()),
                tool_args: Some(serde_json::json!({"path": "src/main.rs"})),
                result: None,
                error: None,
            });
            tasks.push(Task {
                id: "4".to_string(),
                title: "Leer lib.rs".to_string(),
                description: "Leer src/lib.rs para entender los módulos exportados".to_string(),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 7,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec![], // Sin dependencia
                tool_to_use: Some("read_file".to_string()),
                tool_args: Some(serde_json::json!({"path": "src/lib.rs"})),
                result: None,
                error: None,
            });
        } else {
            // Default: read main file
            tasks.push(Task {
                id: "2".to_string(),
                title: "Leer código principal".to_string(),
                description: "Leer el archivo principal para entender el proyecto".to_string(),
                task_type: crate::tools::planner::TaskType::Research,
                status: TaskStatus::Pending,
                priority: 9,
                estimated_effort: TaskEffort::Trivial,
                dependencies: vec!["1".to_string()],
                tool_to_use: Some("read_file".to_string()),
                tool_args: Some(serde_json::json!({"path": "src/main.rs"})),
                result: None,
                error: None,
            });
        }

        TaskPlan {
            id: uuid::Uuid::new_v4().to_string(),
            goal: goal.to_string(),
            tasks,
            current_task_index: 0,
            status: PlanStatus::Created,
            context: HashMap::new(),
            created_at: now,
        }
    }

    /// Execute a plan in the background
    #[allow(dead_code)]
    async fn execute_plan_background(&mut self, plan_id: Uuid) -> Result<PlanningResponse> {
        let mut accumulated_context = HashMap::new();
        let mut tasks_completed = 0;

        loop {
            // Get next task to execute
            let (task, task_index, _total_tasks) = {
                let mut state = self.state.lock().await;
                let plan = state
                    .get_plan_mut(&plan_id)
                    .ok_or_else(|| anyhow::anyhow!("Plan not found"))?;

                // Check if plan is complete
                if plan.status == PlanStatus::Completed {
                    let result = plan
                        .context
                        .get("final_result")
                        .cloned()
                        .unwrap_or_else(|| "Plan completed successfully".to_string());
                    return Ok(PlanningResponse::PlanCompleted {
                        plan_id,
                        result,
                        tasks_completed,
                    });
                }

                if plan.status == PlanStatus::Failed {
                    let error = plan
                        .context
                        .get("error")
                        .cloned()
                        .unwrap_or_else(|| "Plan failed".to_string());
                    return Ok(PlanningResponse::PlanFailed {
                        plan_id,
                        error,
                        tasks_completed,
                    });
                }

                // Get next pending task
                let task_index = plan
                    .tasks
                    .iter()
                    .position(|t| {
                        t.status == TaskStatus::Pending && self.are_dependencies_met(plan, t)
                    })
                    .ok_or_else(|| anyhow::anyhow!("No executable tasks found"))?;

                let task = plan.tasks[task_index].clone();
                let total = plan.tasks.len();

                // Mark as in progress
                plan.tasks[task_index].status = TaskStatus::InProgress;

                (task, task_index, total)
            };

            // Execute the task
            let result = self.execute_task(&task, &accumulated_context).await;

            // Update plan with result
            {
                let mut state = self.state.lock().await;
                let plan = state
                    .get_plan_mut(&plan_id)
                    .ok_or_else(|| anyhow::anyhow!("Plan not found"))?;

                match result {
                    Ok(task_result) => {
                        plan.tasks[task_index].status = TaskStatus::Completed;
                        plan.tasks[task_index].result = Some(task_result.clone());
                        accumulated_context.insert(task.id.clone(), task_result);
                        tasks_completed += 1;

                        // Check if all tasks are done
                        if plan.tasks.iter().all(|t| {
                            t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped
                        }) {
                            plan.status = PlanStatus::Completed;
                            plan.context.insert(
                                "final_result".to_string(),
                                self.summarize_plan_results(plan, &accumulated_context),
                            );
                        }
                    }
                    Err(e) => {
                        plan.tasks[task_index].status = TaskStatus::Failed;
                        plan.tasks[task_index].error = Some(e.to_string());

                        // Try to replan if possible
                        if self.can_replan(plan, &task) {
                            match self
                                .adaptive_replan(&mut plan.clone(), &task, &e.to_string())
                                .await
                            {
                                Ok(new_tasks) => {
                                    // Insert new tasks after the failed one
                                    for (i, new_task) in new_tasks.into_iter().enumerate() {
                                        plan.tasks.insert(task_index + 1 + i, new_task);
                                    }
                                    plan.tasks[task_index].status = TaskStatus::Skipped;
                                }
                                Err(_) => {
                                    // Cannot recover, mark plan as failed
                                    plan.status = PlanStatus::Failed;
                                    plan.context.insert("error".to_string(), e.to_string());
                                }
                            }
                        } else {
                            // Cannot recover
                            plan.status = PlanStatus::Failed;
                            plan.context.insert("error".to_string(), e.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Check if task dependencies are met
    fn are_dependencies_met(&self, plan: &TaskPlan, task: &Task) -> bool {
        task.dependencies.iter().all(|dep_id| {
            plan.tasks
                .iter()
                .find(|t| &t.id == dep_id)
                .map(|t| t.status == TaskStatus::Completed)
                .unwrap_or(false)
        })
    }

    /// Execute a single task with parallel support for independent tasks
    async fn execute_task(&self, task: &Task, context: &HashMap<String, String>) -> Result<String> {
        // Build context summary for this task
        let context_summary = self.build_context_summary(task, context);

        // Determine tool and arguments
        let (tool_name, tool_args) = if let Some(ref tool) = task.tool_to_use {
            let args = task
                .tool_args
                .clone()
                .unwrap_or_else(|| self.infer_tool_args(task, tool, &context_summary));
            (tool.clone(), args)
        } else {
            // No specific tool, use reasoning
            return self.execute_reasoning_task(task, &context_summary).await;
        };

        // Execute the tool
        let result = {
            let orchestrator = self.orchestrator.lock().await;
            orchestrator.execute_tool(&tool_name, &tool_args).await
        };

        // Check if it's a real error (not just a word in the output)
        // Only fail if the result starts with "Error" or is very short error message
        let is_error = result.starts_with("Error")
            || result.starts_with("❌")
            || (result.len() < 100 && result.to_lowercase().contains("error:"))
            || result.contains("No such file or directory")
            || result.contains("Permission denied");

        if is_error {
            Err(anyhow::anyhow!("Tool execution failed: {}", result))
        } else {
            Ok(result)
        }
    }

    /// Build context summary for a task
    fn build_context_summary(&self, task: &Task, context: &HashMap<String, String>) -> String {
        let mut summary = format!("Task: {}\n\n", task.description);

        // Add dependency results
        for dep_id in &task.dependencies {
            if let Some(dep_result) = context.get(dep_id) {
                summary.push_str(&format!("Result from task {}:\n{}\n\n", dep_id, dep_result));
            }
        }

        summary
    }

    /// Infer tool arguments from task description
    fn infer_tool_args(&self, task: &Task, tool_name: &str, _context: &str) -> serde_json::Value {
        let desc_lower = task.description.to_lowercase();

        match tool_name {
            "read_file" => {
                let path = self
                    .extract_file_path(&task.description)
                    .unwrap_or_else(|| {
                        // Common file patterns
                        if desc_lower.contains("main") {
                            "src/main.rs".to_string()
                        } else if desc_lower.contains("cargo") || desc_lower.contains("dependenc") {
                            "Cargo.toml".to_string()
                        } else if desc_lower.contains("readme") {
                            "README.md".to_string()
                        } else if desc_lower.contains("config") {
                            "config/".to_string()
                        } else if desc_lower.contains("lib") {
                            "src/lib.rs".to_string()
                        } else {
                            "src/main.rs".to_string()
                        }
                    });
                json!({ "path": path })
            }
            "list_directory" => {
                let path = self
                    .extract_file_path(&task.description)
                    .unwrap_or_else(|| {
                        if desc_lower.contains("src") {
                            "src".to_string()
                        } else if desc_lower.contains("test") {
                            "tests".to_string()
                        } else {
                            ".".to_string()
                        }
                    });
                json!({ "path": path, "recursive": false })
            }
            "search_in_files" | "search_files" => {
                let pattern = self
                    .extract_search_pattern(&task.description)
                    .unwrap_or_else(|| "TODO".to_string());
                json!({ "pattern": pattern, "path": ".", "recursive": true })
            }
            "execute_shell" => {
                let cmd = if desc_lower.contains("build") || desc_lower.contains("compil") {
                    "cargo build"
                } else if desc_lower.contains("test") || desc_lower.contains("prueba") {
                    "cargo test"
                } else if desc_lower.contains("check") {
                    "cargo check"
                } else if desc_lower.contains("format") {
                    "cargo fmt"
                } else if desc_lower.contains("lint") || desc_lower.contains("clippy") {
                    "cargo clippy"
                } else if desc_lower.contains("run") || desc_lower.contains("ejecut") {
                    "cargo run"
                } else {
                    "cargo check"
                };
                json!({ "command": cmd })
            }
            "write_file" => {
                let path = self
                    .extract_file_path(&task.description)
                    .unwrap_or_else(|| "output.txt".to_string());
                json!({ "path": path, "content": "", "create_dirs": true })
            }
            "git_status" | "git" => {
                json!({ "command": "git status" })
            }
            "git_diff" => {
                json!({ "command": "git diff" })
            }
            "analyze_code" | "code_analyzer" => {
                let path = self
                    .extract_file_path(&task.description)
                    .unwrap_or_else(|| ".".to_string());
                json!({ "path": path })
            }
            "run_linter" | "lint_code" => {
                json!({ "path": "." })
            }
            _ => {
                // Default: try to extract any path
                if let Some(path) = self.extract_file_path(&task.description) {
                    json!({ "path": path })
                } else {
                    json!({})
                }
            }
        }
    }

    /// Extract file path from task description
    fn extract_file_path(&self, description: &str) -> Option<String> {
        // Simple pattern matching for file paths
        let words: Vec<&str> = description.split_whitespace().collect();
        for word in words {
            if word.contains('/') || word.contains('.') && word.len() > 3 {
                return Some(
                    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.')
                        .to_string(),
                );
            }
        }
        None
    }

    /// Extract search pattern from task description
    fn extract_search_pattern(&self, description: &str) -> Option<String> {
        // Look for quoted strings or keywords after "search for", "find", etc.
        if let Some(idx) = description.to_lowercase().find("search for") {
            return Some(
                description[idx + 10..]
                    .split_whitespace()
                    .next()?
                    .to_string(),
            );
        }
        if let Some(idx) = description.to_lowercase().find("find") {
            return Some(
                description[idx + 4..]
                    .split_whitespace()
                    .next()?
                    .to_string(),
            );
        }
        None
    }

    /// Execute a reasoning task (no specific tool)
    async fn execute_reasoning_task(&self, task: &Task, context: &str) -> Result<String> {
        let prompt = format!(
            r#"Task: {}

Context:
{}

Please analyze and provide your findings."#,
            task.description, context
        );

        let orchestrator = self.orchestrator.lock().await;
        orchestrator
            .call_heavy_model_direct(&prompt)
            .await
            .context("Failed to execute reasoning task")
    }

    /// Check if plan can be replanned after failure
    fn can_replan(&self, plan: &TaskPlan, failed_task: &Task) -> bool {
        // Can replan if:
        // 1. Not too many tasks have failed already
        let failed_count = plan
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();
        if failed_count > 3 {
            return false;
        }

        // 2. Task is not critical (has no dependents or few dependents)
        let dependent_count = plan
            .tasks
            .iter()
            .filter(|t| t.dependencies.contains(&failed_task.id))
            .count();

        dependent_count <= 2
    }

    /// Adaptively replan after task failure
    async fn adaptive_replan(
        &self,
        plan: &mut TaskPlan,
        failed_task: &Task,
        error: &str,
    ) -> Result<Vec<Task>> {
        let prompt = format!(
            r#"A task in our plan has failed. Please suggest alternative tasks to achieve the goal.

ORIGINAL GOAL: {}

FAILED TASK: {}
ERROR: {}

COMPLETED TASKS:
{}

Suggest 1-3 alternative tasks that could help us recover and continue toward the goal.
Use the same XML format as before for task definitions."#,
            plan.goal,
            failed_task.description,
            error,
            plan.tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .map(|t| format!("- {}", t.description))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let orchestrator = self.orchestrator.lock().await;
        let response = orchestrator
            .call_heavy_model_direct(&prompt)
            .await
            .context("Failed to generate replan")?;

        // Parse new tasks from response
        self.parse_tasks_from_response(&response)
    }

    /// Parse tasks from LLM XML response
    fn parse_tasks_from_response(&self, response: &str) -> Result<Vec<Task>> {
        use crate::tools::TaskPlannerTool;

        // Create a temporary plan to parse tasks
        let temp_plan = TaskPlannerTool::new()
            .parse_plan("temp", response)
            .map_err(|e| anyhow::anyhow!("Failed to parse plan: {}", e))?;
        Ok(temp_plan.tasks)
    }

    /// Summarize plan results
    fn summarize_plan_results(
        &self,
        plan: &TaskPlan,
        _context: &HashMap<String, String>,
    ) -> String {
        let mut summary = format!("Goal: {}\n\n", plan.goal);
        summary.push_str("Completed tasks:\n");

        for task in &plan.tasks {
            if task.status == TaskStatus::Completed {
                summary.push_str(&format!("✓ {}\n", task.description));
                if let Some(ref result) = task.result {
                    let preview = if result.len() > 100 {
                        format!("{}...", &result[..100])
                    } else {
                        result.clone()
                    };
                    summary.push_str(&format!("  Result: {}\n", preview));
                }
            }
        }

        summary
    }

    /// Execute independent tasks in parallel
    #[allow(dead_code)]
    async fn execute_parallel_tasks(
        &self,
        plan_id: Uuid,
        task_indices: Vec<usize>,
        context: &HashMap<String, String>,
    ) -> Vec<(usize, Result<String>)> {
        let mut handles: Vec<JoinHandle<(usize, Result<String>)>> = vec![];

        for idx in task_indices {
            let task = {
                let state = self.state.lock().await;
                let plan = state.get_plan(&plan_id).unwrap();
                plan.tasks[idx].clone()
            };

            let context_clone = context.clone();
            let self_clone = self.clone();

            let handle = tokio::spawn(async move {
                let result = self_clone.execute_task(&task, &context_clone).await;
                (idx, result)
            });

            handles.push(handle);
        }

        let mut results = vec![];
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        results
    }

    /// Find tasks that can be executed in parallel
    #[allow(dead_code)]
    fn find_parallel_tasks(&self, plan: &TaskPlan) -> Vec<Vec<usize>> {
        let mut parallel_groups = vec![];
        let mut current_group = vec![];

        for (idx, task) in plan.tasks.iter().enumerate() {
            if task.status != TaskStatus::Pending {
                continue;
            }

            // Check if dependencies are met
            if !self.are_dependencies_met(plan, task) {
                continue;
            }

            // Check if task has no conflicts with current group
            let has_conflict = current_group.iter().any(|&other_idx| {
                let other_task: &Task = &plan.tasks[other_idx];
                // Tasks conflict if they share dependencies or one depends on the other
                task.dependencies
                    .iter()
                    .any(|d| other_task.dependencies.contains(d))
                    || other_task.id == task.id
            });

            if has_conflict && !current_group.is_empty() {
                parallel_groups.push(current_group);
                current_group = vec![idx];
            } else {
                current_group.push(idx);
            }
        }

        if !current_group.is_empty() {
            parallel_groups.push(current_group);
        }

        parallel_groups
    }
}

impl Clone for PlanningOrchestrator {
    fn clone(&self) -> Self {
        Self {
            orchestrator: Arc::clone(&self.orchestrator),
            router: self.router.clone(),
            context_cache: self.context_cache.clone(),
            classifier: TaskClassifier::new(),
            tools: Arc::clone(&self.tools),
            // REMOVED: self_correction field (module deleted)
            state: Arc::clone(&self.state),
            raptor_service: None, // RAPTOR service is not cloneable, will be re-initialized if needed
            working_dir: self.working_dir.clone(),
            raptor_initialized: false,
        }
    }
}
