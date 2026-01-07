//! Dual-model orchestrator for routing between fast and heavy models

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
        // LAYER 0: Try direct pattern matching first (fastest, most reliable for Spanish)
        if let Ok(result) = self.extract_tool_from_natural_language(message).await {
            // Pattern matched and tool executed
            let mut state = self.state.lock().await;
            state.add_message(Message::assistant(&result, "tool"));
            return Ok(OrchestratorResponse::Immediate {
                content: result,
                model: "direct".to_string(),
            });
        }

        // LAYER 1: Use prompt-based tools with LLM
        let response = self
            .call_ollama_with_prompt_tools(&self.config.fast_model.clone(), message)
            .await?;

        // Add to state
        {
            let mut state = self.state.lock().await;
            state.add_message(Message::assistant(&response, &self.config.fast_model));
        }

        Ok(OrchestratorResponse::Immediate {
            content: response,
            model: self.config.fast_model.clone(),
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

    /// Build system prompt for prompt-based tools
    fn build_prompt_tools_system_prompt(&self, working_dir: &str) -> String {
        format!(
            r#"You are Neuro, a programming assistant. Working directory: {}

IMPORTANT: You MUST use tools for ANY request about files, code, or the system.

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
            working_dir
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

    /// Robust tool calling with multi-layer fallback system
    #[allow(dead_code)]
    async fn call_ollama_with_robust_tools(
        &self,
        model: &str,
        user_message: &str,
    ) -> Result<String, OrchestratorError> {
        // LAYER 1: JSON Schema mode (primary) - 95% confidence
        match self.try_json_schema_mode(model, user_message).await {
            Ok(response) => return Ok(response),
            Err(e) => eprintln!("JSON schema failed: {}, trying XML...", e),
        }

        // LAYER 2: XML prompt-based (current) - 75% confidence
        match self
            .call_ollama_with_prompt_tools(model, user_message)
            .await
        {
            Ok(response) => return Ok(response),
            Err(e) => eprintln!("XML parsing failed: {}, trying patterns...", e),
        }

        // LAYER 3: Pattern recognition fallback - 60% confidence
        match self.extract_tool_from_natural_language(user_message).await {
            Ok(response) => return Ok(response),
            Err(e) => eprintln!("Pattern matching failed: {}, asking clarification...", e),
        }

        // LAYER 4: Self-healing - pedir clarificación al modelo
        self.request_clarification(model, user_message).await
    }

    /// LAYER 1: Try JSON Schema mode for structured output
    async fn try_json_schema_mode(
        &self,
        model: &str,
        msg: &str,
    ) -> Result<String, OrchestratorError> {
        use crate::agent::confidence::StructuredResponse;

        let client = reqwest::Client::new();
        let working_dir = {
            let state = self.state.lock().await;
            state.working_dir.clone()
        };

        let system_prompt = self.build_enhanced_system_prompt(&working_dir);

        let conversation = vec![
            serde_json::json!({
                "role": "system",
                "content": system_prompt
            }),
            serde_json::json!({
                "role": "user",
                "content": msg
            }),
        ];

        let request = serde_json::json!({
            "model": model,
            "messages": conversation,
            "format": "json",
            "options": {
                "temperature": 0.3,
                "num_predict": 2048
            }
        });

        let response = client
            .post(format!("{}/api/chat", self.config.ollama_url))
            .json(&request)
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
            .ok_or_else(|| OrchestratorError::ModelError("No content in response".to_string()))?;

        // Parse structured response
        let structured: StructuredResponse = serde_json::from_str(content)
            .map_err(|e| OrchestratorError::ModelError(format!("JSON parse error: {}", e)))?;

        if structured.action == "call_tool" {
            let tool_name = structured
                .tool_name
                .ok_or_else(|| OrchestratorError::ModelError("Missing tool_name".to_string()))?;
            let tool_args = structured
                .tool_args
                .ok_or_else(|| OrchestratorError::ModelError("Missing tool_args".to_string()))?;

            let result = self.execute_tool(&tool_name, &tool_args).await;
            return Ok(result);
        }

        Ok(structured.response_text.unwrap_or_default())
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
        let tool_names = vec![
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
        format!(
            r#"Eres Neuro, un asistente de código inteligente. Para usar herramientas, responde con JSON.

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
