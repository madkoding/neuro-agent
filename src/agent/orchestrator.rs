#![allow(dead_code)]

///! Dual-model orchestrator for routing between fast and heavy models

use super::classifier::{TaskClassifier, TaskType};
use super::state::{create_shared_state, Message, PendingTask, SharedState};
use crate::tools::ToolRegistry;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Orchestrator errors
#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("Failed to connect to Ollama: {0}")]
    ConnectionError(String),
    #[error("Model error: {0}")]
    ModelError(String),
    #[error("Task cancelled")]
    Cancelled,
    #[error("Task timed out after {0} seconds")]
    Timeout(u64),
    #[error("Tool error: {0}")]
    ToolError(String),
    #[error("Classification error: {0}")]
    ClassificationError(String),
}

/// Response from the orchestrator
#[derive(Debug, Clone)]
pub enum OrchestratorResponse {
    /// Immediate response (from fast model or simple command)
    Immediate { content: String, model: String },
    /// Simple text response
    Text(String),
    /// Task delegated to heavy model
    Delegated {
        task_id: Uuid,
        description: String,
        estimated_secs: u64,
    },
    /// Streaming response in progress
    Streaming { task_id: Uuid },
    /// Tool execution result
    ToolResult {
        tool_name: String,
        result: String,
        success: bool,
    },
    /// Error response
    Error(String),
    /// Needs user confirmation (for dangerous commands)
    NeedsConfirmation { command: String, risk_level: String },
    /// Task started notification
    TaskStarted { task_id: Uuid, description: String },
}

/// Result from a heavy task
#[derive(Debug, Clone)]
pub struct HeavyTaskResult {
    pub task_id: Uuid,
    pub content: String,
    pub success: bool,
    pub model: String,
}

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Ollama API base URL
    pub ollama_url: String,
    /// Fast model name
    pub fast_model: String,
    /// Heavy model name
    pub heavy_model: String,
    /// Timeout for heavy tasks in seconds
    pub heavy_timeout_secs: u64,
    /// Maximum concurrent heavy tasks
    pub max_concurrent_heavy: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            fast_model: "qwen3:0.6b".to_string(),
            heavy_model: "qwen3:8b".to_string(),
            heavy_timeout_secs: 1200,
            max_concurrent_heavy: 2,
        }
    }
}

/// Dual-model orchestrator
pub struct DualModelOrchestrator {
    config: OrchestratorConfig,
    classifier: TaskClassifier,
    tools: ToolRegistry,
    state: SharedState,
    /// Channel for receiving results from heavy tasks
    result_rx: mpsc::Receiver<HeavyTaskResult>,
    /// Sender for heavy task results (cloned for each task)
    result_tx: mpsc::Sender<HeavyTaskResult>,
    /// Global cancellation token
    global_cancel: CancellationToken,
    /// Per-task cancellation tokens
    task_cancels: std::collections::HashMap<Uuid, CancellationToken>,
}

impl DualModelOrchestrator {
    /// Create a new orchestrator with default configuration
    pub async fn new() -> Result<Self, OrchestratorError> {
        Self::with_config(OrchestratorConfig::default()).await
    }

    /// Create a new orchestrator with custom configuration
    pub async fn with_config(config: OrchestratorConfig) -> Result<Self, OrchestratorError> {
        // Test connection to Ollama
        let client = reqwest::Client::new();
        client
            .get(format!("{}/api/tags", config.ollama_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| OrchestratorError::ConnectionError(e.to_string()))?;

        let (result_tx, result_rx) = mpsc::channel(32);

        Ok(Self {
            config,
            classifier: TaskClassifier::new(),
            tools: ToolRegistry::new(),
            state: create_shared_state(),
            result_rx,
            result_tx,
            global_cancel: CancellationToken::new(),
            task_cancels: std::collections::HashMap::new(),
        })
    }

    /// Get shared state
    pub fn state(&self) -> SharedState {
        self.state.clone()
    }

    /// Get tool registry
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Process user input
    pub async fn process(
        &mut self,
        input: &str,
    ) -> Result<OrchestratorResponse, OrchestratorError> {
        // First, try fast classification
        let task_type = self
            .classifier
            .classify_fast(input)
            .unwrap_or(TaskType::SimpleChat {
                message: input.to_string(),
            });

        // Add user message to state
        {
            let mut state = self.state.lock().await;
            state.add_message(Message::user(input));
        }

        match task_type {
            TaskType::SimpleCommand { action } => self.handle_simple_command(action).await,

            TaskType::SimpleChat { message } => self.handle_simple_chat(&message).await,

            TaskType::CodeReview {
                file_paths,
                review_type,
            } => {
                self.delegate_heavy_task(
                    format!("Code review ({:?}): {:?}", review_type, file_paths),
                    self.build_code_review_prompt(&file_paths, &review_type),
                    60,
                )
                .await
            }

            TaskType::CodeGeneration {
                description,
                language,
                ..
            } => {
                self.delegate_heavy_task(
                    format!("Generating {} code", language),
                    self.build_code_gen_prompt(&description, &language),
                    90,
                )
                .await
            }

            TaskType::ComplexReasoning { query, .. } => {
                self.delegate_heavy_task(
                    "Complex analysis".to_string(),
                    self.build_reasoning_prompt(&query),
                    60,
                )
                .await
            }

            TaskType::ToolExecution { tool_name: _, .. } => {
                // Tool execution is handled by the model
                self.handle_simple_chat(input).await
            }
        }
    }

    /// Handle simple commands directly
    async fn handle_simple_command(
        &mut self,
        action: super::classifier::SimpleAction,
    ) -> Result<OrchestratorResponse, OrchestratorError> {
        use super::classifier::SimpleAction;

        let response = match action {
            SimpleAction::Exit => "Goodbye! 👋".to_string(),
            SimpleAction::Help => self.get_help_text(),
            SimpleAction::ClearHistory => {
                let mut state = self.state.lock().await;
                state.clear_history();
                "History cleared.".to_string()
            }
            SimpleAction::ShowHistory => {
                let state = self.state.lock().await;
                format!("{} messages in history", state.messages.len())
            }
            SimpleAction::ShowStatus => self.get_status_text().await,
            SimpleAction::ListFiles => {
                // Use the list directory tool
                match self.list_current_directory().await {
                    Ok(listing) => listing,
                    Err(e) => format!("Error listing directory: {}", e),
                }
            }
        };

        // Add to state
        {
            let mut state = self.state.lock().await;
            state.add_message(Message::assistant(&response, "system"));
        }

        Ok(OrchestratorResponse::Immediate {
            content: response,
            model: "system".to_string(),
        })
    }

    /// Handle simple chat with fast model - uses multi-layer tool detection
    async fn handle_simple_chat(
        &mut self,
        message: &str,
    ) -> Result<OrchestratorResponse, OrchestratorError> {
        // LAYER 0: Proactive tool execution (pre-fetch obvious context)
        let effective_message = if let Some(proactive_results) = self.proactive_tool_execution(message).await {
            tracing::info!(
                "Proactive execution completed: {} tool(s) executed",
                proactive_results.len()
            );
            
            // Build context message from proactive results
            let context = proactive_results
                .iter()
                .map(|(tool_name, result)| format!("**{}**:\n{}", tool_name, result))
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            
            // Enhance user message with proactive context
            format!(
                "Context gathered:\n{}\n\n---\n\nUser query: {}",
                context, message
            )
        } else {
            message.to_string()
        };

        // LAYER 1: Native function calling (95% confidence)
        match self
            .call_ollama_with_native_tools(&self.config.fast_model.clone(), &effective_message)
            .await
        {
            Ok(response) => {
                tracing::info!("Layer 1 (native tools) succeeded");
                
                // Add to state
                {
                    let mut state = self.state.lock().await;
                    state.add_message(Message::assistant(&response, &self.config.fast_model));
                }
                
                return Ok(OrchestratorResponse::Immediate {
                    content: response,
                    model: self.config.fast_model.clone(),
                });
            }
            Err(e) => {
                tracing::warn!("Layer 1 (native tools) failed: {}, falling back to Layer 2", e);
            }
        }

        // LAYER 2: XML-based prompt tools (75% confidence)
        match self
            .call_ollama_with_prompt_tools(&self.config.fast_model.clone(), &effective_message)
            .await
        {
            Ok(response) => {
                tracing::info!("Layer 2 (XML tools) succeeded");
                
                // Check for vague response
                if self.detect_vague_response(&response) {
                    tracing::warn!("Detected vague response, attempting recovery");
                    
                    // Try pattern matching as recovery
                    if let Ok(result) = self.extract_tool_from_natural_language(message).await {
                        let mut state = self.state.lock().await;
                        state.add_message(Message::assistant(&result, "tool"));
                        return Ok(OrchestratorResponse::Immediate {
                            content: result,
                            model: "recovery".to_string(),
                        });
                    }
                }
                
                // Add to state
                {
                    let mut state = self.state.lock().await;
                    state.add_message(Message::assistant(&response, &self.config.fast_model));
                }
                
                return Ok(OrchestratorResponse::Immediate {
                    content: response,
                    model: self.config.fast_model.clone(),
                });
            }
            Err(e) => {
                tracing::warn!("Layer 2 (XML tools) failed: {}, falling back to Layer 3", e);
            }
        }

        // LAYER 3: Pattern matching (60% confidence)
        if let Ok(result) = self.extract_tool_from_natural_language(message).await {
            tracing::info!("Layer 3 (pattern matching) succeeded");
            
            let mut state = self.state.lock().await;
            state.add_message(Message::assistant(&result, "tool"));
            return Ok(OrchestratorResponse::Immediate {
                content: result,
                model: "direct".to_string(),
            });
        }

        // LAYER 4: Self-healing - ask for clarification
        tracing::warn!("All layers failed, requesting clarification");
        
        let clarification = match crate::i18n::current_locale() {
            crate::i18n::Locale::Spanish => {
                "No pude determinar qué herramienta usar para tu solicitud. ¿Podrías ser más específico? Por ejemplo:\n\
                - Para leer un archivo: 'lee src/main.rs'\n\
                - Para listar archivos: 'lista los archivos'\n\
                - Para ejecutar un comando: 'ejecuta cargo build'"
            }
            crate::i18n::Locale::English => {
                "I couldn't determine which tool to use for your request. Could you be more specific? For example:\n\
                - To read a file: 'read src/main.rs'\n\
                - To list files: 'list files'\n\
                - To run a command: 'run cargo build'"
            }
        };
        
        Ok(OrchestratorResponse::Immediate {
            content: clarification.to_string(),
            model: "fallback".to_string(),
        })
    }

    /// Delegate a task to the heavy model
    async fn delegate_heavy_task(
        &mut self,
        description: String,
        prompt: String,
        estimated_secs: u64,
    ) -> Result<OrchestratorResponse, OrchestratorError> {
        let task_id = Uuid::new_v4();
        let cancel_token = CancellationToken::new();
        self.task_cancels.insert(task_id, cancel_token.clone());

        // Add pending task to state
        {
            let mut state = self.state.lock().await;
            state.add_pending_task(PendingTask::new(&description, estimated_secs));
        }

        // Clone what we need for the spawned task
        let result_tx = self.result_tx.clone();
        let model = self.config.heavy_model.clone();
        let ollama_url = self.config.ollama_url.clone();
        let timeout_secs = self.config.heavy_timeout_secs;

        // Spawn the heavy task
        tokio::spawn(async move {
            let result = Self::run_heavy_task(
                task_id,
                &ollama_url,
                &model,
                &prompt,
                timeout_secs,
                cancel_token,
            )
            .await;

            let _ = result_tx.send(result).await;
        });

        Ok(OrchestratorResponse::Delegated {
            task_id,
            description,
            estimated_secs,
        })
    }

    /// Run a heavy task with timeout and cancellation support
    async fn run_heavy_task(
        task_id: Uuid,
        ollama_url: &str,
        model: &str,
        prompt: &str,
        timeout_secs: u64,
        cancel_token: CancellationToken,
    ) -> HeavyTaskResult {
        let task = async { Self::call_ollama_static(ollama_url, model, prompt).await };

        tokio::select! {
            _ = cancel_token.cancelled() => {
                HeavyTaskResult {
                    task_id,
                    content: "Task cancelled by user".to_string(),
                    success: false,
                    model: model.to_string(),
                }
            }
            result = timeout(Duration::from_secs(timeout_secs), task) => {
                match result {
                    Ok(Ok(content)) => HeavyTaskResult {
                        task_id,
                        content,
                        success: true,
                        model: model.to_string(),
                    },
                    Ok(Err(e)) => HeavyTaskResult {
                        task_id,
                        content: format!("Error: {}", e),
                        success: false,
                        model: model.to_string(),
                    },
                    Err(_) => HeavyTaskResult {
                        task_id,
                        content: format!("Task timed out after {} seconds", timeout_secs),
                        success: false,
                        model: model.to_string(),
                    },
                }
            }
        }
    }

    /// Cancel a specific task
    pub fn cancel_task(&mut self, task_id: Uuid) -> bool {
        if let Some(token) = self.task_cancels.remove(&task_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Cancel all pending tasks
    pub fn cancel_all_tasks(&mut self) {
        for (_, token) in self.task_cancels.drain() {
            token.cancel();
        }
        self.global_cancel.cancel();
    }

    /// Try to receive a completed heavy task result (non-blocking)
    pub fn try_recv_result(&mut self) -> Option<HeavyTaskResult> {
        match self.result_rx.try_recv() {
            Ok(result) => {
                self.task_cancels.remove(&result.task_id);
                Some(result)
            }
            Err(_) => None,
        }
    }

    /// Call heavy model directly with a prompt (public for PlanningOrchestrator)
    pub async fn call_heavy_model_direct(&self, prompt: &str) -> Result<String, OrchestratorError> {
        let client = reqwest::Client::new();

        let request_body = serde_json::json!({
            "model": self.config.heavy_model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 4096
            }
        });

        let response = client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request_body)
            .timeout(Duration::from_secs(self.config.heavy_timeout_secs))
            .send()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let content = response_json["response"].as_str().unwrap_or("").to_string();

        Ok(content)
    }

    /// Call fast model directly with a prompt (for quick summaries)
    pub async fn call_fast_model_direct(&self, prompt: &str) -> Result<String, OrchestratorError> {
        let client = reqwest::Client::new();

        let request_body = serde_json::json!({
            "model": self.config.fast_model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.3,
                "num_predict": 256
            }
        });

        let response = client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request_body)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let content = response_json["response"].as_str().unwrap_or("").to_string();

        Ok(content)
    }

    /// Call Ollama API with prompt-based tools (for models without native tool support)
    async fn call_ollama_with_prompt_tools(
        &self,
        model: &str,
        user_message: &str,
    ) -> Result<String, OrchestratorError> {
        let client = reqwest::Client::new();
        let working_dir = {
            let state = self.state.lock().await;
            state.working_dir.clone()
        };

        let system_prompt = self.build_prompt_tools_system_prompt(&working_dir);

        let mut conversation = vec![
            serde_json::json!({
                "role": "system",
                "content": system_prompt
            }),
            serde_json::json!({
                "role": "user",
                "content": user_message
            }),
        ];

        let mut final_response = String::new();
        let max_iterations = 10;

        for _iteration in 0..max_iterations {
            let request_body = serde_json::json!({
                "model": model,
                "messages": conversation,
                "stream": false,
                "options": {
                    "temperature": 0.7,
                    "num_predict": 4096
                }
            });

            let response = client
                .post(format!("{}/api/chat", self.config.ollama_url))
                .json(&request_body)
                .timeout(Duration::from_secs(300))
                .send()
                .await
                .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

            let response_json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

            let content = response_json["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            // Check if the model wants to use a tool
            if let Some((tool_name, tool_args)) = self.parse_tool_call_from_response(&content) {
                // Execute the tool
                let tool_result = self.execute_tool(&tool_name, &tool_args).await;

                // Add to conversation
                conversation.push(serde_json::json!({
                    "role": "assistant",
                    "content": content
                }));
                conversation.push(serde_json::json!({
                    "role": "user",
                    "content": format!("Tool result:\n```\n{}\n```\n\nPlease continue with your response based on this result.", tool_result)
                }));

                continue;
            }

            // No tool call detected, this is the final response
            final_response = content;
            break;
        }

        Ok(final_response)
    }

    /// Call Ollama with native function calling support (Ollama 0.3+)
    ///
    /// This is the PRIMARY method (Layer 1, 95% confidence) for tool usage.
    /// Uses Ollama's native `/api/chat` endpoint with tools array for robust
    /// function calling without XML parsing.
    async fn call_ollama_with_native_tools(
        &self,
        model: &str,
        user_message: &str,
    ) -> Result<String, OrchestratorError> {
        use crate::agent::provider::OllamaProvider;
        use crate::agent::{build_minimal_system_prompt, PromptConfig};
        use crate::config::{ModelConfig, ModelProvider as ProviderType};
        use crate::i18n::current_locale;

        let working_dir = {
            let state = self.state.lock().await;
            state.working_dir.clone()
        };

        // Create provider
        let provider_config = ModelConfig {
            provider: ProviderType::Ollama,
            url: self.config.ollama_url.clone(),
            model: model.to_string(),
            api_key: None,
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: Some(4096),
        };
        let provider = OllamaProvider::new(provider_config);

        // Get tools schema
        let tools_schema = self.tools.get_ollama_tools_schema().await;

        // Build minimal system prompt
        let prompt_config = PromptConfig::new(working_dir, current_locale());
        let system_prompt = build_minimal_system_prompt(&prompt_config);

        // Initialize conversation
        let mut conversation = vec![
            serde_json::json!({
                "role": "system",
                "content": system_prompt
            }),
            serde_json::json!({
                "role": "user",
                "content": user_message
            }),
        ];

        let max_iterations = 10;

        for iteration in 0..max_iterations {
            tracing::debug!(
                "Native function calling iteration {}/{} for model: {}",
                iteration + 1,
                max_iterations,
                model
            );

            // Call model with tools
            let message = provider
                .generate_with_tools(conversation.clone(), Some(tools_schema.clone()))
                .await
                .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

            // Check for tool calls
            if let Some(tool_calls) = &message.tool_calls {
                if !tool_calls.is_empty() {
                    tracing::info!("Model requested {} tool call(s)", tool_calls.len());

                    // Add assistant message to conversation
                    conversation.push(serde_json::json!({
                        "role": "assistant",
                        "content": message.content.clone().unwrap_or_default(),
                        "tool_calls": tool_calls
                    }));

                    // Execute tool calls SEQUENTIALLY (respecting dependencies)
                    for tool_call in tool_calls {
                        let tool_name = &tool_call.function.name;
                        let tool_args = &tool_call.function.arguments;

                        tracing::info!("Executing tool: {} with args: {:?}", tool_name, tool_args);

                        // Execute the tool
                        let tool_result = self.execute_tool(tool_name, tool_args).await;

                        // Add tool result as a tool message
                        conversation.push(serde_json::json!({
                            "role": "tool",
                            "content": tool_result
                        }));
                    }

                    // Continue loop to get model's response with tool results
                    continue;
                }
            }

            // No tool calls, this is the final response
            if let Some(content) = message.content {
                return Ok(content);
            }

            // Edge case: no content and no tool calls
            tracing::warn!("Model returned no content and no tool calls on iteration {}", iteration);
            break;
        }

        Err(OrchestratorError::ModelError(
            "Max iterations reached or no valid response".to_string(),
        ))
    }

    /// Proactive tool execution - pre-execute obvious tools before LLM call
    ///
    /// This method analyzes the user query and determines if it clearly requires
    /// specific tools (confidence > 0.85). If so, it pre-executes those tools
    /// and adds their results to the context, reducing roundtrips and improving
    /// response quality.
    /// Proactive tool execution has been REMOVED
    /// 
    /// DESIGN DECISION: We no longer use pattern matching (contains, regex) to decide
    /// which tools to execute. This makes the system "dumb" and inflexible.
    /// 
    /// Instead, we trust the LLM with native function calling to intelligently decide
    /// which tools it needs based on the user's query context. The LLM has access to:
    ///   - All 25+ tools via Ollama native function calling
    ///   - Full tool descriptions and parameters
    ///   - Conversation history and working directory
    /// 
    /// Example: "analiza este repositorio"
    ///   - OLD: Pattern matching checks for "analiza" + "repositorio" → executes hardcoded tools
    ///   - NEW: LLM sees query → decides to call semantic_search, list_directory, read_file as needed
    /// 
    /// This is more flexible, context-aware, and aligned with the project goal:
    /// "Make small models work as well as GitHub Copilot by compensating with robust application"
    async fn proactive_tool_execution(&self, _user_message: &str) -> Option<Vec<(String, String)>> {
        // DISABLED: Let the LLM decide everything
        None
    }

    /// Detect vague or unhelpful responses from the model
    ///
    /// This method checks if the model's response indicates it couldn't help,
    /// lacks information, or is asking for clarification instead of using tools.
    fn detect_vague_response(&self, response: &str) -> bool {
        let lower = response.to_lowercase();

        // Spanish patterns
        let vague_patterns_es = [
            "no puedo",
            "no sé",
            "no tengo",
            "necesito más",
            "especifica",
            "podrías ser más específico",
            "no está claro",
            "no entiendo",
            "sin más contexto",
            "sin información",
        ];

        // English patterns
        let vague_patterns_en = [
            "i cannot",
            "i can't",
            "i don't know",
            "i don't have",
            "i need more",
            "please specify",
            "could you be more specific",
            "not clear",
            "unclear",
            "i don't understand",
            "without more context",
            "without information",
            "i'm unable",
            "cannot perform",
            "let me know if",
        ];

        // Check if response contains vague patterns
        vague_patterns_es.iter().any(|p| lower.contains(p))
            || vague_patterns_en.iter().any(|p| lower.contains(p))
    }

    /// Build system prompt for prompt-based tools
    fn build_prompt_tools_system_prompt(&self, working_dir: &str) -> String {
        let lang_instruction = crate::i18n::llm_language_instruction();
        format!(
            r#"You are Neuro, a programming assistant. Working directory: {}

IMPORTANT: You MUST use tools for ANY request about files, code, or the system.

LANGUAGE: {}

## Available Tools:

### read_file - Read a file
{{"name": "read_file", "arguments": {{"path": "filepath"}}}}

### list_directory - List files
{{"name": "list_directory", "arguments": {{"path": "."}}}}

### execute_shell - Run commands
{{"name": "execute_shell", "arguments": {{"command": "cargo build"}}}}

### write_file - Write to file
{{"name": "write_file", "arguments": {{"path": "file", "content": "text"}}}}

## HOW TO USE TOOLS:
Wrap tool calls in <tool_call> tags:

<tool_call>
{{"name": "tool_name", "arguments": {{"param": "value"}}}}
</tool_call>

## EXAMPLES:

User: "lee main.rs" or "read main.rs" or "muéstrame main.rs"
<tool_call>
{{"name": "read_file", "arguments": {{"path": "src/main.rs"}}}}
</tool_call>

User: "lista archivos" or "list files" or "ls"
<tool_call>
{{"name": "list_directory", "arguments": {{"path": "."}}}}
</tool_call>

User: "compila" or "build" or "cargo build"
<tool_call>
{{"name": "execute_shell", "arguments": {{"command": "cargo build"}}}}
</tool_call>

User: "qué archivos hay en src" or "what's in src"
<tool_call>
{{"name": "list_directory", "arguments": {{"path": "src"}}}}
</tool_call>

## RULES:
1. ALWAYS use a tool for file/code/system requests
2. Use ONE tool at a time
3. After getting results, explain them to the user
4. Respond in the SAME language the user used"#,
            working_dir,
            lang_instruction
        )
    }

    /// Parse tool call from model response (for prompt-based tools)
    fn parse_tool_call_from_response(&self, response: &str) -> Option<(String, serde_json::Value)> {
        // Look for <tool_call>...</tool_call> pattern
        let start_tag = "<tool_call>";
        let end_tag = "</tool_call>";

        if let Some(start_idx) = response.find(start_tag) {
            if let Some(end_idx) = response.find(end_tag) {
                let json_str = &response[start_idx + start_tag.len()..end_idx].trim();

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let name = parsed["name"].as_str()?.to_string();
                    let arguments = parsed["arguments"].clone();
                    return Some((name, arguments));
                }
            }
        }

        None
    }



    /// LAYER 3: Pattern recognition fallback
    async fn extract_tool_from_natural_language(
        &self,
        msg: &str,
    ) -> Result<String, OrchestratorError> {
        let lower = msg.to_lowercase();

        // Direct pattern matching for common requests (fastest, most reliable)

        // Read file patterns (Spanish + English)
        let read_patterns = [
            "lee ",
            "leer ",
            "read ",
            "muestra ",
            "mostrar ",
            "show ",
            "abre ",
            "abrir ",
            "open ",
            "ver ",
            "cat ",
            "contenido ",
            "dame ",
            "muestrame ",
            "muéstrame ",
            "enseña ",
            "enseñame ",
            "visualiza ",
            "imprime ",
            "print ",
        ];
        if read_patterns.iter().any(|p| lower.contains(p))
            || lower.starts_with("lee")
            || lower.starts_with("ver")
        {
            if let Some(path) = self.extract_path_from_message(msg) {
                let args = serde_json::json!({"path": path});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            }
        }

        // List directory patterns (Spanish + English)
        let list_patterns = [
            "lista",
            "listar",
            "list",
            "ls",
            "archivos",
            "files",
            "directorio",
            "directory",
            "carpeta",
            "folder",
            "qué hay",
            "que hay",
            "what's in",
            "whats in",
            "what is in",
            "muestra la estructura",
            "estructura del proyecto",
            "tree",
            "contenido de la carpeta",
            "files in",
        ];
        if list_patterns.iter().any(|p| lower.contains(p)) {
            let path = self
                .extract_path_from_message(msg)
                .unwrap_or(".".to_string());
            let args = serde_json::json!({"path": path, "recursive": false});
            let result = self.execute_tool("list_directory", &args).await;
            return Ok(result);
        }

        // Execute/build patterns (Spanish + English)
        let exec_patterns = [
            "ejecuta",
            "ejecutar",
            "run ",
            "exec",
            "corre ",
            "correr ",
            "compila",
            "compilar",
            "build",
            "construye",
            "construir",
            "cargo",
            "npm",
            "make",
            "yarn",
            "pnpm",
        ];
        if exec_patterns.iter().any(|p| lower.contains(p)) {
            let cmd = self.extract_command_from_message(msg);
            let args = serde_json::json!({"command": cmd});
            let result = self.execute_tool("execute_shell", &args).await;
            return Ok(result);
        }

        // Test patterns
        let test_patterns = ["test", "prueba", "pruebas", "testing", "spec"];
        if test_patterns.iter().any(|p| lower.contains(p)) {
            let args = serde_json::json!({"command": "cargo test"});
            let result = self.execute_tool("execute_shell", &args).await;
            return Ok(result);
        }

        // Git patterns
        let git_patterns = [
            "git ", "commit", "status", "diff", "branch", "push", "pull", "log",
        ];
        if git_patterns.iter().any(|p| lower.contains(p)) {
            let cmd = if lower.contains("status") || lower.contains("estado") {
                "git status"
            } else if lower.contains("diff") || lower.contains("cambios") {
                "git diff"
            } else if lower.contains("log")
                || lower.contains("historial")
                || lower.contains("commits")
            {
                "git log --oneline -10"
            } else if lower.contains("branch") || lower.contains("rama") {
                "git branch"
            } else {
                "git status"
            };
            let args = serde_json::json!({"command": cmd});
            let result = self.execute_tool("execute_shell", &args).await;
            return Ok(result);
        }

        // Search patterns
        let search_patterns = [
            "busca",
            "buscar",
            "search",
            "encuentra",
            "encontrar",
            "find",
            "grep",
            "localiza",
        ];
        if search_patterns.iter().any(|p| lower.contains(p)) {
            // Extract search term
            let search_term = msg
                .split_whitespace()
                .skip(1) // Skip the command word
                .collect::<Vec<_>>()
                .join(" ");
            if !search_term.is_empty() {
                let args = serde_json::json!({
                    "pattern": search_term,
                    "path": ".",
                    "recursive": true
                });
                let result = self.execute_tool("search_files", &args).await;
                return Ok(result);
            }
        }

        // NEW: Project structure / architecture patterns
        let structure_patterns = [
            "estructura", "structure", "arquitectura", "architecture",
            "organización", "organization", "cómo está organizado",
        ];
        if structure_patterns.iter().any(|p| lower.contains(p)) {
            let args = serde_json::json!({"path": ".", "recursive": true});
            let result = self.execute_tool("list_directory", &args).await;
            return Ok(result);
        }

        // NEW: Analyze code patterns
        let analyze_patterns = [
            "analiza", "analyze", "revisa", "review",
            "problemas", "issues", "errores en el código",
        ];
        if analyze_patterns.iter().any(|p| lower.contains(p))
            && (lower.contains("código") || lower.contains("code") || lower.contains("proyecto"))
        {
            // Try to run linter if available
            let args = serde_json::json!({"path": "."});
            let result = self.execute_tool("run_linter", &args).await;
            return Ok(result);
        }

        // NEW: Dependencies patterns
        let dep_patterns = [
            "dependencias", "dependencies", "librerías", "libraries",
            "paquetes", "packages", "crates",
        ];
        if dep_patterns.iter().any(|p| lower.contains(p)) {
            // For Rust projects, read Cargo.toml
            if std::path::Path::new("Cargo.toml").exists() {
                let args = serde_json::json!({"path": "Cargo.toml"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            }
            // For JS projects, read package.json
            if std::path::Path::new("package.json").exists() {
                let args = serde_json::json!({"path": "package.json"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            }
        }

        // NEW: Main/Entry point patterns
        let main_patterns = [
            "código principal", "main code", "punto de entrada",
            "entry point", "archivo principal", "main file",
        ];
        if main_patterns.iter().any(|p| lower.contains(p)) {
            // Detect language and read appropriate main file
            if std::path::Path::new("src/main.rs").exists() {
                let args = serde_json::json!({"path": "src/main.rs"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            } else if std::path::Path::new("main.py").exists() {
                let args = serde_json::json!({"path": "main.py"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            } else if std::path::Path::new("index.js").exists() {
                let args = serde_json::json!({"path": "index.js"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            }
        }

        // NEW: Documentation patterns
        let doc_patterns = [
            "documentación", "documentation", "readme",
            "cómo usar", "how to use", "guía", "guide",
        ];
        if doc_patterns.iter().any(|p| lower.contains(p)) {
            if std::path::Path::new("README.md").exists() {
                let args = serde_json::json!({"path": "README.md"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            } else if std::path::Path::new("README").exists() {
                let args = serde_json::json!({"path": "README"});
                let result = self.execute_tool("read_file", &args).await;
                return Ok(result);
            }
        }

        Err(OrchestratorError::ModelError(
            "No pattern matched".to_string(),
        ))
    }

    /// Extract file path from message
    fn extract_path_from_message(&self, msg: &str) -> Option<String> {
        // Look for file paths with extensions
        let re = regex::Regex::new(r"[\w./\-]+\.[\w]+").ok()?;
        if let Some(m) = re.find(msg) {
            return Some(m.as_str().to_string());
        }

        // Look for directory paths
        let re = regex::Regex::new(
            r#"(?:en|in|from|de|la carpeta|folder|directory|directorio)\s+["']?([^\s"']+)["']?"#,
        )
        .ok()?;
        if let Some(caps) = re.captures(msg) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }

        // Look for src/, ./ etc
        let re = regex::Regex::new(r"((?:\./|\.\./)?\w+(?:/\w+)*)").ok()?;
        for m in re.find_iter(msg) {
            let path = m.as_str();
            if path.contains('/') || path == "src" || path == "." {
                return Some(path.to_string());
            }
        }

        None
    }

    /// Extract command from message
    fn extract_command_from_message(&self, msg: &str) -> String {
        let lower = msg.to_lowercase();

        // Direct command detection
        if lower.contains("cargo build") || lower.contains("compila") {
            return "cargo build".to_string();
        }
        if lower.contains("cargo run") || lower.contains("ejecuta el proyecto") {
            return "cargo run".to_string();
        }
        if lower.contains("cargo test")
            || lower.contains("corre los tests")
            || lower.contains("run tests")
        {
            return "cargo test".to_string();
        }
        if lower.contains("cargo check") {
            return "cargo check".to_string();
        }
        if lower.contains("npm install") {
            return "npm install".to_string();
        }
        if lower.contains("npm run") {
            // Extract what comes after "npm run"
            if let Some(idx) = lower.find("npm run") {
                let rest = &msg[idx + 8..].trim();
                let cmd = rest.split_whitespace().next().unwrap_or("dev");
                return format!("npm run {}", cmd);
            }
        }

        // Try to extract quoted command
        let re = regex::Regex::new(r#"["`']([^"`']+)["`']"#).unwrap();
        if let Some(caps) = re.captures(msg) {
            return caps.get(1).unwrap().as_str().to_string();
        }

        // Default to cargo build for Rust projects
        "cargo build".to_string()
    }

    /// LAYER 4: Self-healing - request clarification
    async fn request_clarification(
        &self,
        model: &str,
        original: &str,
    ) -> Result<String, OrchestratorError> {
        let tool_names = [
            "read_file",
            "list_directory",
            "execute_shell",
            "write_file",
            "run_linter",
        ]
        .join(", ");

        let clarification_prompt = format!(
            "No pude entender tu respuesta. Por favor especifica claramente:\n\
             1. Nombre de la herramienta (opciones: {})\n\
             2. Argumentos como JSON\n\n\
             Responde SOLO en este formato:\n\
             TOOL: nombre_herramienta\n\
             ARGS: {{\"arg1\": \"valor1\", \"arg2\": \"valor2\"}}\n\n\
             Tu mensaje original fue: {}",
            tool_names, original
        );

        let retry = self
            .call_ollama_simple(model, &clarification_prompt)
            .await?;

        // Parse simple format
        self.parse_simple_format(&retry).await
    }

    /// Simple Ollama call without tools
    async fn call_ollama_simple(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<String, OrchestratorError> {
        let client = reqwest::Client::new();

        let request_body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "stream": false
        });

        let response = client
            .post(format!("{}/api/chat", self.config.ollama_url))
            .json(&request_body)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let content = response_json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }

    /// Parse simple TOOL:/ARGS: format
    async fn parse_simple_format(&self, text: &str) -> Result<String, OrchestratorError> {
        let lines: Vec<&str> = text.lines().collect();

        let tool_line = lines
            .iter()
            .find(|l| l.starts_with("TOOL:"))
            .ok_or_else(|| OrchestratorError::ModelError("No TOOL line found".to_string()))?;

        let args_line = lines
            .iter()
            .find(|l| l.starts_with("ARGS:"))
            .ok_or_else(|| OrchestratorError::ModelError("No ARGS line found".to_string()))?;

        let tool_name = tool_line.replace("TOOL:", "").trim().to_string();
        let args_json = args_line.replace("ARGS:", "").trim().to_string();
        let args: serde_json::Value = serde_json::from_str(&args_json)
            .map_err(|e| OrchestratorError::ModelError(format!("Invalid JSON: {}", e)))?;

        // Execute tool based on name
        let result = self.execute_tool(&tool_name, &args).await;
        Ok(result)
    }

    /// Build enhanced system prompt with few-shot examples
    fn build_enhanced_system_prompt(&self, working_dir: &str) -> String {
        let lang_instruction = crate::i18n::llm_language_instruction();
        format!(
            r#"Eres Neuro, un asistente de código inteligente. Para usar herramientas, responde con JSON.

IDIOMA: {}

EJEMPLOS DE USO CORRECTO DE HERRAMIENTAS:

Ejemplo 1 - Leer archivo:
User: "muéstrame el archivo main.rs"
Assistant: {{"action": "call_tool", "tool_name": "read_file", "tool_args": {{"path": "src/main.rs"}}}}

Ejemplo 2 - Listar directorio:
User: "lista los archivos en src/"
Assistant: {{"action": "call_tool", "tool_name": "list_directory", "tool_args": {{"path": "src", "recursive": false}}}}

Ejemplo 3 - Ejecutar comando:
User: "compila el proyecto"
Assistant: {{"action": "call_tool", "tool_name": "execute_shell", "tool_args": {{"command": "cargo build"}}}}

Ejemplo 4 - Responder sin herramienta:
User: "explícame qué es Rust"
Assistant: {{"action": "respond", "response_text": "Rust es un lenguaje..."}}

HERRAMIENTAS DISPONIBLES:
- read_file: Lee archivos (path, start_line, end_line)
- write_file: Escribe archivos (path, content, append, create_dirs)
- list_directory: Lista directorios (path, recursive, max_depth)
- execute_shell: Ejecuta comandos (command, working_dir, timeout_secs)
- run_linter: Ejecuta linter (path, check_only)

REGLAS CRÍTICAS:
1. SIEMPRE responde con JSON válido
2. Si necesitas información de archivos, usa las herramientas
3. Para tareas complejas, descompón en múltiples llamadas

Directorio de trabajo actual: {}
"#,
            lang_instruction,
            working_dir
        )
    }

    /// Execute a tool by name (public for PlanningOrchestrator)
    pub async fn execute_tool(&self, tool_name: &str, args: &serde_json::Value) -> String {
        use crate::tools::{
            FileReadArgs, FileWriteArgs, LinterArgs, ListDirectoryArgs, ShellExecuteArgs,
        };
        use rig::tool::Tool;

        let working_dir = {
            let state = self.state.lock().await;
            state.working_dir.clone()
        };

        match tool_name {
            "read_file" => {
                let path = args["path"].as_str().unwrap_or("");
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("{}/{}", working_dir, path)
                };

                let tool_args = FileReadArgs {
                    path: full_path.clone(),
                    start_line: args["start_line"].as_u64().map(|n| n as usize),
                    end_line: args["end_line"].as_u64().map(|n| n as usize),
                };

                match self.tools.file_read.call(tool_args).await {
                    Ok(result) => {
                        if result.total_lines > 100 {
                            format!(
                                "File: {} ({} lines, showing {})\n\n{}",
                                full_path, result.total_lines, result.lines_read, result.content
                            )
                        } else {
                            format!("File: {}\n\n{}", full_path, result.content)
                        }
                    }
                    Err(e) => format!("Error reading file: {}", e),
                }
            }

            "write_file" => {
                let path = args["path"].as_str().unwrap_or("");
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("{}/{}", working_dir, path)
                };

                let tool_args = FileWriteArgs {
                    path: full_path,
                    content: args["content"].as_str().unwrap_or("").to_string(),
                    append: args["append"].as_bool().unwrap_or(false),
                    create_dirs: args["create_dirs"].as_bool().unwrap_or(true),
                };

                match self.tools.file_write.call(tool_args).await {
                    Ok(result) => {
                        format!(
                            "✅ File written: {} ({} bytes)",
                            result.path, result.bytes_written
                        )
                    }
                    Err(e) => format!("Error writing file: {}", e),
                }
            }

            "list_directory" => {
                let path = args["path"].as_str().unwrap_or(".");
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else if path == "." {
                    working_dir.clone()
                } else {
                    format!("{}/{}", working_dir, path)
                };

                let tool_args = ListDirectoryArgs {
                    path: full_path,
                    recursive: args["recursive"].as_bool().unwrap_or(false),
                    max_depth: args["max_depth"].as_u64().unwrap_or(3) as usize,
                };

                match self.tools.list_directory.call(tool_args).await {
                    Ok(result) => {
                        let mut output =
                            format!("Directory listing ({} entries):\n\n", result.count);
                        for entry in result.entries {
                            let icon = if entry.is_dir { "📁" } else { "📄" };
                            let size = entry
                                .size
                                .map(|s| format!(" ({} bytes)", s))
                                .unwrap_or_default();
                            output.push_str(&format!("{} {}{}\n", icon, entry.name, size));
                        }
                        output
                    }
                    Err(e) => format!("Error listing directory: {}", e),
                }
            }

            "execute_shell" => {
                let command = args["command"].as_str().unwrap_or("");
                let cmd_working_dir = args["working_dir"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| working_dir.clone());

                let tool_args = ShellExecuteArgs {
                    command: command.to_string(),
                    working_dir: Some(cmd_working_dir),
                    timeout_secs: args["timeout_secs"].as_u64().unwrap_or(60),
                };

                match self.tools.shell_execute.call(tool_args).await {
                    Ok(result) => {
                        let status = if result.exit_code == 0 { "✅" } else { "❌" };
                        let mut output =
                            format!("{} Command exited with code {}\n", status, result.exit_code);
                        if !result.stdout.is_empty() {
                            output.push_str(&format!("\nstdout:\n{}", result.stdout));
                        }
                        if !result.stderr.is_empty() {
                            output.push_str(&format!("\nstderr:\n{}", result.stderr));
                        }
                        output
                    }
                    Err(e) => format!("Error executing command: {}", e),
                }
            }

            "run_linter" => {
                let path = args["path"].as_str().unwrap_or(".");
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else if path == "." {
                    working_dir.clone()
                } else {
                    format!("{}/{}", working_dir, path)
                };

                let tool_args = LinterArgs {
                    project_path: full_path,
                    mode: crate::tools::LinterMode::Clippy,
                    extra_args: vec![],
                    auto_fix: args["auto_fix"].as_bool().unwrap_or(false),
                };

                match self.tools.linter.call(tool_args).await {
                    Ok(result) => {
                        let mut output = format!(
                            "Linter results: {} errors, {} warnings\n",
                            result.error_count, result.warning_count
                        );
                        if !result.diagnostics.is_empty() {
                            output.push_str("\nDiagnostics:\n");
                            for diag in result.diagnostics.iter().take(20) {
                                let file = diag.file.as_deref().unwrap_or("unknown");
                                let line = diag.line.unwrap_or(0);
                                output.push_str(&format!(
                                    "  [{}] {}:{}: {}\n",
                                    diag.level, file, line, diag.message
                                ));
                            }
                            if result.diagnostics.len() > 20 {
                                output.push_str(&format!(
                                    "  ... and {} more\n",
                                    result.diagnostics.len() - 20
                                ));
                            }
                        }
                        output
                    }
                    Err(e) => format!("Error running linter: {}", e),
                }
            }

            "build_raptor_tree" => {
                let path = args["path"].as_str().unwrap_or(".");
                let full_path = if path.starts_with('/') {
                    path.to_string()
                } else if path == "." {
                    working_dir.clone()
                } else {
                    format!("{}/{}", working_dir, path)
                };
                
                let max_chars = args["max_chars"].as_u64().unwrap_or(2500) as usize;
                let threshold = args["threshold"].as_f64().unwrap_or(0.5) as f32;
                
                tracing::info!("� RAPTOR build requested for: {} (max_chars: {}, threshold: {})", full_path, max_chars, threshold);
                
                // For now, RAPTOR requires PlanningOrchestrator context
                // Return informative message and suggest alternatives
                format!(
                    "📊 RAPTOR hierarchical indexing requested for '{}'\n\n\
                    ⚠️ Full RAPTOR indexing requires heavy model context.\n\
                    Available alternatives:\n\
                    - Use list_directory to explore structure\n\
                    - Use read_file for specific files (README.md, Cargo.toml)\n\
                    - Use search_files to find code patterns\n\n\
                    For complete project analysis, please use the planning mode.",
                    path
                )
            }

            "query_raptor_tree" => {
                let query = args["query"].as_str().unwrap_or("");
                let top_k = args["top_k"].as_u64().unwrap_or(5) as usize;
                
                tracing::info!("🔍 RAPTOR query requested: {} (top_k: {})", query, top_k);
                
                format!(
                    "🔍 RAPTOR query for: '{}'\n\n\
                    ⚠️ RAPTOR tree not initialized in this context.\n\
                    Available alternatives:\n\
                    - Use search_files to search code\n\
                    - Use read_file to inspect specific files\n\
                    - Use list_directory to explore structure\n\n\
                    For hierarchical project understanding, please use planning mode.",
                    query
                )
            }

            "semantic_search" => {
                let query = args["query"].as_str().unwrap_or("");
                let _limit = args["limit"].as_u64().unwrap_or(10) as usize;
                
                tracing::info!("🔎 Semantic search requested: {}", query);
                
                // Semantic search not yet in registry - suggest alternatives
                format!(
                    "🔎 Semantic search for: '{}'\n\n\
                    ⚠️ Semantic search requires embedding engine.\n\
                    Try using:\n\
                    - search_files: grep-style text search across files\n\
                    - list_directory: explore project structure\n\
                    - read_file: read specific files\n\n\
                    Example: Use search_files to find where '{}' appears in code.",
                    query, query
                )
            }

            _ => format!("Unknown tool: {}", tool_name),
        }
    }

    /// Call Ollama API (static method for use in spawned tasks - without tools)
    async fn call_ollama_static(
        ollama_url: &str,
        model: &str,
        prompt: &str,
    ) -> Result<String, OrchestratorError> {
        let client = reqwest::Client::new();

        let request_body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 4096
            }
        });

        let response = client
            .post(format!("{}/api/chat", ollama_url))
            .json(&request_body)
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| OrchestratorError::ModelError(e.to_string()))?;

        response_json["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| OrchestratorError::ModelError("Invalid response format".to_string()))
    }

    /// Build prompt for code review
    fn build_code_review_prompt(
        &self,
        file_paths: &[String],
        review_type: &super::classifier::ReviewType,
    ) -> String {
        format!(
            "Please perform a {:?} code review on the following files: {:?}\n\n\
             Focus on:\n\
             - Identifying potential issues\n\
             - Suggesting improvements\n\
             - Highlighting best practices violations\n\n\
             Use the available tools to read the file contents first.",
            review_type, file_paths
        )
    }

    /// Build prompt for code generation
    fn build_code_gen_prompt(&self, description: &str, language: &str) -> String {
        format!(
            "Generate {} code for the following requirement:\n\n{}\n\n\
             Requirements:\n\
             - Write clean, idiomatic code\n\
             - Include appropriate error handling\n\
             - Add helpful comments\n\
             - Follow best practices for {}",
            language, description, language
        )
    }

    /// Build prompt for complex reasoning
    fn build_reasoning_prompt(&self, query: &str) -> String {
        format!(
            "Please analyze the following question thoroughly:\n\n{}\n\n\
             Provide a detailed, well-reasoned response. \
             Use tools if needed to gather additional context.",
            query
        )
    }

    /// Get help text
    fn get_help_text(&self) -> String {
        format!(
            r#"🧠 Neuro - AI Programming Assistant

Commands:
  help     - Show this help message
  exit     - Exit the application
  clear    - Clear conversation history
  history  - Show message count
  status   - Show system status
  ls       - List files in current directory

Models:
  Fast ({}): Quick responses, command parsing
  Heavy ({}): Code review, generation, complex analysis

{}

Tips:
  - Ask to "review" code for analysis
  - Ask to "generate" or "create" for new code
  - Use Ctrl+C to cancel a running task
"#,
            self.config.fast_model,
            self.config.heavy_model,
            self.tools.tool_descriptions()
        )
    }

    /// Get status text
    async fn get_status_text(&self) -> String {
        let state = self.state.lock().await;

        format!(
            r#"System Status:
  Session ID: {}
  Messages: {}
  Pending tasks: {}
  Working dir: {}
  Total tokens: {}
  
Models:
  Fast: {}
  Heavy: {}
  Ollama: {}"#,
            state.session_id,
            state.messages.len(),
            state.pending_tasks.len(),
            state.working_dir,
            state.total_tokens,
            self.config.fast_model,
            self.config.heavy_model,
            self.config.ollama_url
        )
    }

    /// List current directory
    async fn list_current_directory(&self) -> Result<String, OrchestratorError> {
        let state = self.state.lock().await;
        let dir = &state.working_dir;

        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| OrchestratorError::ToolError(e.to_string()))?;

        let mut listing = format!("Contents of {}:\n\n", dir);

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| OrchestratorError::ToolError(e.to_string()))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| OrchestratorError::ToolError(e.to_string()))?;

            let type_indicator = if metadata.is_dir() { "📁" } else { "📄" };
            let name = entry.file_name().to_string_lossy().to_string();

            listing.push_str(&format!("  {} {}\n", type_indicator, name));
        }

        Ok(listing)
    }

    /// Process user input with a filtered list of enabled tools
    /// This is used by the modern UI to allow tool configuration
    pub async fn process_with_tools(
        &mut self,
        input: &str,
        _enabled_tool_ids: &[String],
    ) -> Result<OrchestratorResponse, OrchestratorError> {
        // For now, just use the regular process method
        // In the future, we can filter which tools are available based on enabled_tool_ids

        // Add user message to state
        {
            let mut state = self.state.lock().await;
            state.add_message(Message::user(input));
        }

        // Use prompt-based tools for better compatibility
        let response = self
            .call_ollama_with_prompt_tools(&self.config.fast_model.clone(), input)
            .await?;

        // Check if the response contains tool usage info
        let tool_name = if response.contains("[read_file]") || response.contains("read_file") {
            Some("read_file".to_string())
        } else if response.contains("[list_directory]") || response.contains("list_directory") {
            Some("list_directory".to_string())
        } else if response.contains("[execute_shell]") || response.contains("execute_shell") {
            Some("execute_shell".to_string())
        } else if response.contains("[write_file]") || response.contains("write_file") {
            Some("write_file".to_string())
        } else if response.contains("[run_linter]") || response.contains("run_linter") {
            Some("run_linter".to_string())
        } else {
            None
        };

        // Add to state
        {
            let mut state = self.state.lock().await;
            state.add_message(Message::assistant(&response, &self.config.fast_model));
        }

        if let Some(tool) = tool_name {
            Ok(OrchestratorResponse::ToolResult {
                tool_name: tool,
                result: response,
                success: true,
            })
        } else {
            Ok(OrchestratorResponse::Text(response))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.fast_model, "qwen3:0.6b");
        assert_eq!(config.heavy_model, "qwen3:8b");
        assert_eq!(config.heavy_timeout_secs, 1200);
    }
}
