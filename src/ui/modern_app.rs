//! Modern TUI Application with async processing

#![allow(deprecated)]

use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use tokio::sync::{mpsc, Mutex};

use crate::agent::{
    OrchestratorResponse, PlanningOrchestrator, PlanningResponse, RouterOrchestrator,
    TaskProgressInfo, TaskProgressStatus,
};
use crate::i18n::{current_locale, init_locale, t, Locale, Text};
use crate::{log_error, log_debug};

/// Enum que envuelve ambos tipos de orquestadores
pub enum OrchestratorWrapper {
    Planning(PlanningOrchestrator),
    Router(RouterOrchestrator),
}
use crate::tools::TaskPlan;

use super::animations::{Spinner, StatusIndicator, StatusState};
use super::layout::centered_rect;
use super::model_config_panel::{ButtonAction, ModelConfigPanel};
use super::settings::{SettingsPanel, ToolConfig};
use super::theme::{Icons, Theme};
// Plan widgets available but not used in modern_app directly
// use super::widgets::{PlanViewer, PlanSummary};

/// Application mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Chat,
    Settings,
    ModelConfig,
    IndexingPrompt,
    Confirmation,
    Password,
}

/// Indexing options for the prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexingOption {
    /// RAG rápido (ahora) + RAPTOR background
    RagNow,
    /// Solo RAPTOR completo en background
    RaptorOnly,
    /// Más tarde (no indexar ahora)
    Later,
}

impl IndexingOption {
    pub fn next(self) -> Self {
        match self {
            IndexingOption::RagNow => IndexingOption::RaptorOnly,
            IndexingOption::RaptorOnly => IndexingOption::Later,
            IndexingOption::Later => IndexingOption::RagNow,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            IndexingOption::RagNow => IndexingOption::Later,
            IndexingOption::RaptorOnly => IndexingOption::RagNow,
            IndexingOption::Later => IndexingOption::RaptorOnly,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            IndexingOption::RagNow => "RAG Rápido (ahora) + RAPTOR (background)",
            IndexingOption::RaptorOnly => "Solo RAPTOR completo (background)",
            IndexingOption::Later => "Más tarde",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            IndexingOption::RagNow => "Indexación rápida en 2-5s, RAPTOR completo después",
            IndexingOption::RaptorOnly => "Indexación completa en background (~30-60s)",
            IndexingOption::Later => "No indexar ahora (funcionalidad limitada)",
        }
    }
}

#[cfg(test)]
mod tests_prefs {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_read_project_preferences_for_path() {
        let dir = tempdir().unwrap();
        let prefs_dir = dir.path().join(".neuro-agent");
        std::fs::create_dir_all(&prefs_dir).unwrap();

        let prefs_file = prefs_dir.join("preferences.json");
        let prefs = serde_json::json!({
            "skip_indexing_prompt": true,
            "default_indexing_option": "later"
        });
        std::fs::write(&prefs_file, serde_json::to_string(&prefs).unwrap()).unwrap();

        let res = ModernApp::read_project_preferences_for_path_testable(dir.path());
        assert!(res.is_some());
        let (skip, opt) = res.unwrap();
        assert!(skip);
        assert_eq!(opt, "later");
    }
}

#[cfg(test)]
mod tests_auto_index {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_should_auto_start_indexing_default() {
        let dir = tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        // No prefs and no index cache => should auto start
        assert!(ModernApp::should_auto_start_indexing(false));

        // Create prefs to skip auto index
        let prefs_dir = dir.path().join(".neuro-agent");
        std::fs::create_dir_all(&prefs_dir).unwrap();
        let prefs = serde_json::json!({
            "skip_indexing_prompt": true,
            "default_indexing_option": "later"
        });
        std::fs::write(prefs_dir.join("preferences.json"), serde_json::to_string(&prefs).unwrap()).unwrap();

        // With skip preference => should not auto start
        assert!(!ModernApp::should_auto_start_indexing(false));

        // With index cache present => should not auto start
        std::fs::create_dir_all(dir.path().join(".neuro-agent").join("raptor")).unwrap();
        assert!(!ModernApp::should_auto_start_indexing(false));

        // Restore cwd
        std::env::set_current_dir(orig).unwrap();
    }
}

/// Input mode for the chat
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal question mode
    Question,
    /// Build/execute mode
    Build,
    /// Planning mode
    Plan,
}

impl InputMode {
    pub fn next(self) -> Self {
        match self {
            InputMode::Question => InputMode::Build,
            InputMode::Build => InputMode::Plan,
            InputMode::Plan => InputMode::Question,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            InputMode::Question => "Pregunta",
            InputMode::Build => "Build",
            InputMode::Plan => "Plan",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            InputMode::Question => "❓",
            InputMode::Build => "🔨",
            InputMode::Plan => "📋",
        }
    }
}

/// Chat message for display
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub sender: MessageSender,
    pub content: String,
    pub timestamp: Instant,
    pub is_streaming: bool,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSender {
    User,
    Assistant,
    System,
    Tool,
}

use crate::agent::AgentEvent;

/// Main application state
pub struct ModernApp {
    // Core
    terminal: Terminal<CrosstermBackend<Stdout>>,
    orchestrator: Arc<Mutex<OrchestratorWrapper>>,
    should_quit: bool,

    // UI State
    screen: AppScreen,
    theme: Theme,

    // Chat
    messages: Vec<DisplayMessage>,
    input_buffer: String,
    cursor_position: usize,

    // Planning
    active_plan: Option<TaskPlan>,
    show_plan_panel: bool,

    // Scroll state
    scroll_offset: usize,
    auto_scroll: bool,

    // Status & Animations
    status: StatusIndicator,
    spinner: Spinner,
    status_message: String,

    // Processing state
    is_processing: bool,
    processing_start: Option<Instant>,
    last_event_time: Option<Instant>,  // Track time of last event for inactivity timeout
    current_thinking: Option<String>,

    // Streaming optimization: accumulate chunks without rendering
    streaming_buffer: Option<String>,
    streaming_chunks_count: usize,

    // Background task communication
    response_rx: Option<mpsc::Receiver<AgentEvent>>,
    background_task_handle: Option<tokio::task::JoinHandle<()>>,

    // Settings
    settings_panel: SettingsPanel,
    model_config_panel: ModelConfigPanel,

    // Confirmation
    pending_command: Option<String>,
    password_input: String,
    password_error: Option<String>,

    // Background RAPTOR indexing
    raptor_indexing: bool,
    raptor_status: Option<String>,
    raptor_progress: Option<(usize, usize)>, // (current, total)
    raptor_stage: Option<String>,
    raptor_rx: Option<mpsc::Receiver<AgentEvent>>,
    raptor_start_time: Option<Instant>,
    raptor_eta: Option<Duration>,

    // Indexing prompt state
    indexing_prompt_dont_ask: bool,
    indexing_prompt_selected: IndexingOption,

    // Input mode
    input_mode: InputMode,

    // Ctrl+C counter for exit
    ctrl_c_count: u8,
    last_ctrl_c: Option<Instant>,

    // Tick counter for animations
    tick_counter: u64,

    // Command autocomplete
    show_autocomplete: bool,
    autocomplete_selected: usize,
}

impl ModernApp {
    /// Read project preferences from `.neuro-agent/preferences.json` under `path`.
    fn read_project_preferences_for_path(path: &std::path::Path) -> Option<(bool, String)> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct PrefsFile {
            skip_indexing_prompt: Option<bool>,
            default_indexing_option: Option<String>,
        }

        let prefs_dir = path.join(".neuro-agent");
        let prefs_file = prefs_dir.join("preferences.json");

        if !prefs_file.exists() {
            return None;
        }

        match std::fs::read_to_string(&prefs_file) {
            Ok(content) => match serde_json::from_str::<PrefsFile>(&content) {
                Ok(p) => Some((
                    p.skip_indexing_prompt.unwrap_or(false),
                    p.default_indexing_option.unwrap_or_else(|| "later".to_string()),
                )),
                Err(_) => None,
            },
            Err(_) => None,
        }
    }

    /// Decide whether to auto-start RAPTOR indexing for the current project.
    /// This is `pub(crate)` so tests can validate the decision logic without
    /// starting the full TUI.
    #[allow(dead_code)]
    pub(crate) fn should_auto_start_indexing(raptor_indexing: bool) -> bool {
        let project_path = std::env::current_dir().unwrap_or_default();
        let prefs = Self::read_project_preferences_for_path(&project_path);
        let skip_auto_index = prefs
            .as_ref()
            .map(|(skip, opt)| *skip && opt == "later")
            .unwrap_or(false);

        let cache_path = project_path.join(".neuro-agent").join("raptor");
        let has_indexed = cache_path.exists() && cache_path.is_dir();

        !has_indexed && !raptor_indexing && !skip_auto_index
    }
    /// Clean XML tags and formatting artifacts from a response
    fn clean_xml_from_response(text: &str) -> String {
        let mut result = text.to_string();

        // Remover bloques de plan XML completos
        if let (Some(start), Some(end)) = (result.find("<plan>"), result.find("</plan>")) {
            if end > start {
                result = format!("{}{}", &result[..start], &result[end + 7..]);
            }
        }

        // Remover tags individuales comunes
        let tags_to_remove = [
            "<task",
            "</task>",
            "<description>",
            "</description>",
            "<dependencies>",
            "</dependencies>",
            "depends=",
            "tool=",
            "id=",
        ];
        for tag in tags_to_remove {
            result = result.replace(tag, "");
        }

        // Limpiar líneas vacías múltiples
        while result.contains("\n\n\n") {
            result = result.replace("\n\n\n", "\n\n");
        }

        result.trim().to_string()
    }

    pub async fn new(orchestrator: PlanningOrchestrator) -> io::Result<Self> {
        Self::new_internal(OrchestratorWrapper::Planning(orchestrator)).await
    }

    /// Create a new ModernApp with a RouterOrchestrator
    pub async fn new_with_router(orchestrator: RouterOrchestrator) -> io::Result<Self> {
        Self::new_internal(OrchestratorWrapper::Router(orchestrator)).await
    }

    /// Internal constructor that accepts wrapped orchestrator
    async fn new_internal(orchestrator: OrchestratorWrapper) -> io::Result<Self> {
        // Initialize locale
        let locale = init_locale();

        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let theme = Theme::dark();

        Ok(Self {
            terminal,
            orchestrator: Arc::new(Mutex::new(orchestrator)),
            should_quit: false,

            screen: AppScreen::Chat,
            theme,

            messages: vec![
                DisplayMessage {
                    sender: MessageSender::System,
                    content: format!("{} ({})", t(Text::Ready), locale.display_name()),
                    timestamp: Instant::now(),
                    is_streaming: false,
                    tool_name: None,
                },
            ],
            input_buffer: String::new(),
            cursor_position: 0,

            active_plan: None,
            show_plan_panel: false,

            scroll_offset: 0,
            auto_scroll: true,

            status: StatusIndicator::new(),
            spinner: Spinner::dots(),
            status_message: t(Text::Ready).to_string(),

            is_processing: false,
            processing_start: None,
            last_event_time: None,
            current_thinking: None,

            streaming_buffer: None,
            streaming_chunks_count: 0,

            response_rx: None,
            background_task_handle: None,

            settings_panel: SettingsPanel::new(),
            model_config_panel: ModelConfigPanel::new(crate::config::AppConfig::default()),

            pending_command: None,
            password_input: String::new(),
            password_error: None,

            raptor_indexing: false,
            raptor_status: None,
            raptor_progress: None,
            raptor_stage: None,
            raptor_rx: None,
            raptor_start_time: None,
            raptor_eta: None,

            indexing_prompt_dont_ask: false,
            indexing_prompt_selected: IndexingOption::RagNow,

            input_mode: InputMode::Question,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            tick_counter: 0,

            show_autocomplete: false,
            autocomplete_selected: 0,
        })
    }

    /// Check if this project has been indexed before
    fn has_indexed_this_project(&self) -> bool {
        // Check if RAPTOR cache exists
        let project_path = std::env::current_dir().unwrap_or_default();
        let cache_path = project_path.join(".neuro-agent").join("raptor");
        cache_path.exists() && cache_path.is_dir()
    }

    #[cfg(test)]
    fn read_project_preferences_for_path_testable(path: &std::path::Path) -> Option<(bool, String)> {
        Self::read_project_preferences_for_path(path)
    }

    #[allow(dead_code)]
    /// Check if this is a git project (has .git directory)
    fn is_git_project(&self) -> bool {
        let project_path = std::env::current_dir().unwrap_or_default();
        project_path.join(".git").exists()
    }

    /// Start background RAPTOR indexing (two phases: quick index + full RAPTOR)
    fn start_background_raptor_indexing(&mut self) {
        if self.raptor_indexing {
            return; // Already indexing
        }

        self.raptor_indexing = true;
        self.raptor_status = Some("Iniciando indexado...".to_string());
        self.raptor_progress = Some((0, 0));
        self.raptor_stage = Some("Preparación".to_string());
        self.raptor_start_time = Some(Instant::now());
        self.raptor_eta = None;

        let orchestrator = self.orchestrator.clone();
        let (tx, rx) = mpsc::channel::<AgentEvent>(50);
        self.raptor_rx = Some(rx);

        // Spawn background task with two phases
        tokio::spawn(async move {
            use crate::raptor::builder::{has_full_index, quick_index_sync};

            // Phase 1: Quick index (very fast - just read files) - run in blocking thread
            let _ = tx
                .send(AgentEvent::RaptorProgress {
                    stage: "Lectura".to_string(),
                    current: 0,
                    total: 0,
                    detail: "Escaneando archivos...".to_string(),
                })
                .await;

            let project_path = std::env::current_dir().unwrap_or_default();
            let path_clone = project_path.clone();

            let quick_result = tokio::time::timeout(
                Duration::from_secs(30), // 30 second timeout for quick index
                tokio::task::spawn_blocking(move || quick_index_sync(&path_clone, 1500, 200))
            ).await;

            match quick_result {
                Ok(Ok(Ok(chunks))) => {
                    let _ = tx
                        .send(AgentEvent::RaptorProgress {
                            stage: "Lectura".to_string(),
                            current: chunks,
                            total: chunks,
                            detail: format!("{} archivos leídos", chunks),
                        })
                        .await;
                }
                Ok(Ok(Err(_))) | Ok(Err(_)) => {
                    let _ = tx
                        .send(AgentEvent::RaptorStatus(
                            "⚠ Error en lectura".to_string(),
                        ))
                        .await;
                }
                Err(_) => {
                    let _ = tx
                        .send(AgentEvent::RaptorStatus(
                            "⏱️ Timeout en lectura".to_string(),
                        ))
                        .await;
                }
            }

            // Phase 2: Full RAPTOR index (embeddings, clustering, summarization)
            let is_full_result = tokio::time::timeout(
                Duration::from_secs(5), // 5 second timeout for checking full index
                tokio::task::spawn_blocking(has_full_index)
            ).await;

            let is_full = match is_full_result {
                Ok(Ok(full)) => full,
                _ => false, // Assume not full if timeout or error
            };

            if !is_full {
                // Create a channel for progress updates
                let (progress_tx, mut progress_rx) =
                    tokio::sync::mpsc::channel::<crate::agent::TaskProgressInfo>(50);

                // Spawn task to forward progress updates
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    while let Some(progress) = progress_rx.recv().await {
                        // Use task_index/total_tasks as current/total for progress
                        // Description format: "Stage: detail"
                        let description = progress.description.clone();
                        let current = progress.task_index;
                        let total = progress.total_tasks;
                        
                        // Extract stage from description (before colon)
                        if let Some(colon_pos) = description.find(':') {
                            let stage = description[..colon_pos].to_string();
                            let detail = description[colon_pos + 1..].trim().to_string();
                            
                            let _ = tx_clone
                                .send(AgentEvent::RaptorProgress {
                                    stage,
                                    current,
                                    total,
                                    detail,
                                })
                                .await;
                        } else {
                            // No colon, use description as-is
                            let _ = tx_clone
                                .send(AgentEvent::RaptorProgress {
                                    stage: "RAPTOR".to_string(),
                                    current,
                                    total,
                                    detail: description,
                                })
                                .await;
                        }
                    }
                });

                let mut orch = orchestrator.lock().await;
                
                match &mut *orch {
                    OrchestratorWrapper::Planning(planning) => {
                        match planning.initialize_raptor_with_progress(Some(progress_tx)).await {
                            Ok(true) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus(
                                        "✓ RAPTOR listo".to_string(),
                                    ))
                                    .await;
                            }
                            Ok(false) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus("📄 Solo texto".to_string()))
                                    .await;
                            }
                            Err(_) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus(
                                        "⚠ Error RAPTOR".to_string(),
                                    ))
                                    .await;
                            }
                        }
                    }
                    OrchestratorWrapper::Router(router) => {
                        // RouterOrchestrator: use initialize_raptor_with_progress
                        match router.initialize_raptor_with_progress(Some(progress_tx)).await {
                            Ok(true) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus(
                                        "✓ RAPTOR listo".to_string(),
                                    ))
                                    .await;
                            }
                            Ok(false) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus("📄 Solo texto".to_string()))
                                    .await;
                            }
                            Err(_) => {
                                let _ = tx
                                    .send(AgentEvent::RaptorStatus(
                                        "⚠ Error RAPTOR".to_string(),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
            } else {
                let _ = tx
                    .send(AgentEvent::RaptorStatus(
                        "✓ RAPTOR listo".to_string(),
                    ))
                    .await;
            }

            let _ = tx.try_send(AgentEvent::RaptorComplete);
        });
    }

    /// Check for RAPTOR indexing updates
    fn check_raptor_status(&mut self) {
        if let Some(ref mut rx) = self.raptor_rx {
            loop {
                match rx.try_recv() {
                    Ok(AgentEvent::RaptorStatus(status)) => {
                        // Parsear el estado para extraer información de progreso
                        if status.contains("chunks listos") {
                            if let Some(num_str) = status.split_whitespace().nth(1) {
                                if let Ok(num) = num_str.parse::<usize>() {
                                    self.raptor_progress = Some((num, num));
                                    self.raptor_stage = Some("Lectura".to_string());
                                }
                            }
                        } else if status.contains("Indexando RAPTOR") {
                            self.raptor_stage = Some("RAPTOR".to_string());
                        } else if status.contains("Leyendo archivos") {
                            self.raptor_stage = Some("Lectura".to_string());
                        }
                        self.raptor_status = Some(status);
                    }
                    Ok(AgentEvent::RaptorProgress {
                        stage,
                        current,
                        total,
                        detail,
                    }) => {
                        self.raptor_stage = Some(stage);
                        self.raptor_progress = Some((current, total));
                        self.raptor_status = Some(detail);
                    }
                    Ok(AgentEvent::RaptorComplete) => {
                        self.raptor_indexing = false;
                        self.raptor_status = Some("Índice listo ✓".to_string());
                        self.raptor_progress = None;
                        self.raptor_stage = None;
                        self.raptor_rx = None;
                        break;
                    }
                    Ok(_) => {} // Ignore other messages
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.raptor_indexing = false;
                        self.raptor_progress = None;
                        self.raptor_stage = None;
                        self.raptor_rx = None;
                        break;
                    }
                }
            }
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        // Auto-start RAPTOR indexing if not already indexed (silent for non-git projects as well)
        // Respect project preferences if the user chose "Don't ask again" and default option is "later"
        let project_path = std::env::current_dir().unwrap_or_default();
        let prefs = Self::read_project_preferences_for_path(&project_path);
        let skip_auto_index = prefs
            .as_ref()
            .map(|(skip, opt)| *skip && opt == "later")
            .unwrap_or(false);

        if !self.has_indexed_this_project() && !self.raptor_indexing && !skip_auto_index {
            self.start_background_raptor_indexing();
        }

        let tick_rate = Duration::from_millis(80); // Faster tick for smoother animations
        let mut last_tick = Instant::now();
        let mut loop_iteration = 0u64;
        let mut last_log_iter = 0u64;

        loop {
            loop_iteration += 1;

            // Log every 100 iterations (roughly every 8 seconds) to track event loop responsiveness
            if loop_iteration - last_log_iter >= 100 {
                let elapsed = self.processing_start.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                log_debug!("🔄 [EVENT-LOOP] Iteration {}, processing_elapsed: {}s", loop_iteration, elapsed);
                last_log_iter = loop_iteration;
            }

            // Draw UI first
            self.draw()?;

            // Check for background task completion
            self.check_background_response().await;

            // Yield to runtime after processing events to keep UI responsive
            tokio::task::yield_now().await;

            // Check RAPTOR indexing status
            self.check_raptor_status();

            // Handle events with short timeout for responsive animations
            let timeout = tick_rate.saturating_sub(last_tick.elapsed());

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => self.handle_key_event(key).await,
                    Event::Mouse(mouse) => self.handle_mouse_event(mouse),
                    _ => {}
                }
            }

            // Update animations on every tick
            if last_tick.elapsed() >= tick_rate {
                self.tick();
                last_tick = Instant::now();
            }

            if self.should_quit {
                break;
            }
        }

        // Cleanup
        self.cleanup()?;
        Ok(())
    }

    async fn check_background_response(&mut self) {
        // Early exit if not processing
        if !self.is_processing {
            return;
        }

        let _processing_elapsed = self.processing_start.map(|t| t.elapsed().as_secs());
        let mut messages_to_add: Vec<(MessageSender, String, Option<String>)> = Vec::new();
        let mut final_response: Option<Result<PlanningResponse, String>> = None;
        let mut orch_response: Option<Result<OrchestratorResponse, String>> = None;
        let mut should_close = false;
        let mut new_status: Option<String> = None;

        // Detect if we haven't received events for a very long time (possible stuck/lost StreamEnd)
        // Only apply timeout after we've been processing for at least 5 seconds
        if let Some(last_event) = self.last_event_time {
            let since_last_event = last_event.elapsed().as_secs();
            let since_start = self.processing_start.map(|t| t.elapsed().as_secs()).unwrap_or(0);

            // If no events for 60 seconds AND we've been processing for at least 5 seconds,
            // assume stream ended but StreamEnd was lost or process is stuck
            if since_last_event >= 60 && since_start >= 5 {
                log_debug!("🔧 [TIMEOUT] No events for {}s, assuming stream ended or stuck", since_last_event);
                self.add_message(MessageSender::System,
                    format!("⚠️ Timeout: Sin eventos por {} segundos. El proceso puede estar bloqueado.", since_last_event),
                    None);
                self.cleanup_processing();
                return;
            }
        }

        // Early exit if no channel (nothing to process)
        if self.response_rx.is_none() {
            return;
        }

        if let Some(ref mut rx) = self.response_rx {
            // Aggressive draining: process ALL available events immediately
            // No yielding - we want to drain the entire channel buffer as fast as possible
            let mut events_count = 0;

            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        events_count += 1;

                        // Update last event time whenever we receive ANYTHING
                        self.last_event_time = Some(Instant::now());

                        // Now process the event
                        match event {
                            AgentEvent::Response(result) => {
                                orch_response = Some(result.clone());
                                // Check if this is a streaming response
                                let is_streaming = if let Ok(ref resp) = result {
                                    matches!(resp, OrchestratorResponse::Streaming { .. })
                                } else {
                                    false
                                };

                                if !is_streaming {
                                    // Non-streaming responses close immediately
                                    should_close = true;
                                    break;
                                }
                                // For streaming responses, continue processing chunks
                            }
                            AgentEvent::PlanningResponse(result) => {
                                final_response = Some(result);
                                should_close = true;
                                break;
                            }
                            AgentEvent::Status(status) => {
                                new_status = Some(status.clone());
                                // Status messages are shown in chat (System messages don't show header)
                                messages_to_add.push((MessageSender::System, status, None));
                            }
                            AgentEvent::Progress(progress) => {
                                let msg = format!("{}", progress.message);
                                new_status = Some(msg.clone());
                                // Add progress to messages (System messages don't show header, just content)
                                messages_to_add.push((MessageSender::System, msg, None));
                            }
                            AgentEvent::Chunk(content) => {
                                // PERFORMANCE FIX: Accumulate chunks in hidden buffer, don't render
                                if let Some(ref mut buffer) = self.streaming_buffer {
                                    buffer.push_str(&content);
                                } else {
                                    self.streaming_buffer = Some(content);
                                }

                                self.streaming_chunks_count += 1;

                                // Update status every 100 chunks to show progress
                                if self.streaming_chunks_count % 100 == 0 {
                                    let kb = self.streaming_buffer.as_ref().map(|b| b.len() / 1024).unwrap_or(0);
                                    self.status_message = format!("Generando respuesta... {} KB recibidos", kb);
                                }
                            }
                            AgentEvent::StreamEnd => {
                                log_debug!("🏁 [UI] StreamEnd received, creating final message");

                                // Create the complete message from the buffer
                                if let Some(buffer) = self.streaming_buffer.take() {
                                    log_debug!("🏁 [UI] Message finalized: {} chars from {} chunks", buffer.len(), self.streaming_chunks_count);

                                    let msg = DisplayMessage {
                                        sender: MessageSender::Assistant,
                                        content: buffer,
                                        timestamp: Instant::now(),
                                        is_streaming: false,
                                        tool_name: None,
                                    };
                                    self.messages.push(msg);
                                    self.auto_scroll = true;
                                }

                                // Reset streaming state
                                self.streaming_buffer = None;
                                self.streaming_chunks_count = 0;

                                // Close the channel and reset processing state
                                should_close = true;
                            }
                            AgentEvent::TaskProgress(progress) => {
                                let TaskProgressInfo {
                                    task_index,
                                    total_tasks,
                                    description,
                                    status,
                                } = progress;
                                let msg = match status {
                                    TaskProgressStatus::Started => {
                                        new_status = Some(format!(
                                            "Tarea {}/{}: {}",
                                            task_index + 1,
                                            total_tasks,
                                            description
                                        ));
                                        continue;
                                    }
                                    TaskProgressStatus::Completed(_) => {
                                        format!("✅ {}/{}: {}", task_index + 1, total_tasks, description)
                                    }
                                    TaskProgressStatus::Failed(error) => {
                                        format!(
                                            "❌ {}/{}: {} - {}",
                                            task_index + 1,
                                            total_tasks,
                                            description,
                                            error
                                        )
                                    }
                                };
                                messages_to_add.push((MessageSender::System, msg, None));
                            }
                            AgentEvent::RaptorStatus(_)
                            | AgentEvent::RaptorProgress { .. } => {
                                // Handled by check_raptor_status, ignore here
                            }
                            AgentEvent::RaptorComplete => {
                                // Handled by check_raptor_status, ignore here
                            }
                            AgentEvent::Error(err_msg) => {
                                messages_to_add.push((MessageSender::System, format!("Error: {}", err_msg), None));
                                should_close = true;
                            }
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        should_close = true;
                        self.status.set_state(StatusState::Error);
                        self.status_message = t(Text::Error).to_string();
                        break;
                    }
                }
            }

            // Only log if we processed a significant number of events or received StreamEnd
            if events_count > 100 {
                log_debug!("📥 [UI] Processed {} events this iteration", events_count);
            }
        }

        // Process collected messages (chunks are processed inline now)
        for (sender, content, tool) in messages_to_add {
            self.add_message(sender, content, tool);
        }

        if let Some(status) = new_status {
            self.status_message = status;
        }

        if let Some(result) = orch_response {
            // Check if this is a streaming response before closing the channel
            let is_streaming = if let Ok(ref resp) = result {
                matches!(resp, OrchestratorResponse::Streaming { .. })
            } else {
                false
            };

            self.handle_orchestrator_response(result);

            // Only close if NOT streaming (we need to keep receiving chunks)
            if !is_streaming {
                self.cleanup_processing();
            }
        } else if let Some(result) = final_response {
            self.handle_planning_response(result);
            self.cleanup_processing();
        } else if should_close {
            self.cleanup_processing();
        }
    }

    fn handle_orchestrator_response(&mut self, result: Result<OrchestratorResponse, String>) {
        match result {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Text(text) => {
                        self.add_message(MessageSender::Assistant, text, None);
                        self.status.set_state(StatusState::Success);
                    }
                    OrchestratorResponse::ToolResult {
                        tool_name, result, ..
                    } => {
                        self.add_message(MessageSender::Tool, result, Some(tool_name));
                        self.status.set_state(StatusState::Success);
                    }
                    OrchestratorResponse::Error(err) => {
                        self.add_message(MessageSender::System, format!("Error: {}", err), None);
                        self.status.set_state(StatusState::Error);
                    }
                    OrchestratorResponse::NeedsConfirmation { command, .. } => {
                        self.pending_command = Some(command);
                        self.screen = AppScreen::Confirmation;
                    }
                    OrchestratorResponse::Immediate { content, .. } => {
                        self.add_message(MessageSender::Assistant, content, None);
                        self.status.set_state(StatusState::Success);
                    }
                    OrchestratorResponse::Delegated { description, .. } => {
                        self.add_message(
                            MessageSender::System,
                            format!("Task delegated: {}", description),
                            None,
                        );
                    }
                    OrchestratorResponse::TaskStarted { description, .. } => {
                        self.add_message(MessageSender::System, description, None);
                    }
                    OrchestratorResponse::Streaming { .. } => {
                        // Create a streaming message that will be filled with chunks
                        let msg = DisplayMessage {
                            sender: MessageSender::Assistant,
                            content: String::new(),
                            timestamp: Instant::now(),
                            is_streaming: true,
                            tool_name: None,
                        };
                        self.messages.push(msg);
                        self.auto_scroll = true;
                    }
                }
            }
            Err(e) => {
                self.add_message(
                    MessageSender::System,
                    format!("{}: {}", t(Text::Error), e),
                    None,
                );
                self.status.set_state(StatusState::Error);
            }
        }
    }

    fn handle_planning_response(&mut self, result: Result<PlanningResponse, String>) {
        match result {
            Ok(response) => {
                match response {
                    PlanningResponse::Simple(orch_response) => {
                        // Delegate to orchestrator response handler
                        self.handle_orchestrator_response(Ok(orch_response));
                    }
                    PlanningResponse::PlanStarted {
                        goal, total_tasks, ..
                    } => {
                        self.add_message(
                            MessageSender::System,
                            format!(
                                "📋 Plan started: {}\n{} tasks to complete",
                                goal, total_tasks
                            ),
                            None,
                        );
                        self.show_plan_panel = true;
                        self.status.set_state(StatusState::Working);
                    }
                    PlanningResponse::PlanCompleted { result, .. } => {
                        // Limpiar cualquier XML residual del resultado
                        let clean_result = Self::clean_xml_from_response(&result);
                        self.add_message(
                            MessageSender::Assistant,
                            format!("{}\n", clean_result), // Añadir línea extra al final
                            None,
                        );
                        self.show_plan_panel = false;
                        self.active_plan = None;
                        self.status.set_state(StatusState::Success);
                    }
                    PlanningResponse::PlanFailed {
                        error,
                        tasks_completed,
                        ..
                    } => {
                        self.add_message(
                            MessageSender::System,
                            format!("❌ Plan failed after {} tasks: {}", tasks_completed, error),
                            None,
                        );
                        self.show_plan_panel = false;
                        self.active_plan = None;
                        self.status.set_state(StatusState::Error);
                    }
                    PlanningResponse::TaskCompleted {
                        task_index,
                        total_tasks,
                        ..
                    } => {
                        self.status_message =
                            format!("Task {}/{} completed", task_index + 1, total_tasks);
                    }
                }
            }
            Err(e) => {
                self.add_message(
                    MessageSender::System,
                    format!("{}: {}", t(Text::Error), e),
                    None,
                );
                self.status.set_state(StatusState::Error);
                self.show_plan_panel = false;
                self.active_plan = None;
            }
        }
    }

    fn cleanup(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        )?;
        Ok(())
    }

    fn tick(&mut self) {
        self.status.tick();
        self.spinner.tick();
        self.settings_panel.tick();
        self.tick_counter = self.tick_counter.wrapping_add(1);
    }

    fn draw(&mut self) -> io::Result<()> {
        // Prepare data needed for rendering (avoid cloning large vectors)
        let render_data = RenderData {
            theme: self.theme.clone(),
            screen: self.screen,
            status_render: self.status.render(),
            status_message: self.status_message.clone(),
            messages: &self.messages,
            input_buffer: self.input_buffer.clone(),
            scroll_offset: self.scroll_offset,
            is_processing: self.is_processing,
            processing_start: self.processing_start,
            spinner_frame: self.spinner.frame().to_string(),
            settings_tools: self.settings_panel.tools.clone(),
            settings_selected: self.settings_panel.selected_index,
            model_config_panel: &self.model_config_panel,
            pending_command: self.pending_command.clone(),
            password_input_len: self.password_input.len(),
            password_error: self.password_error.clone(),
            enabled_tools_count: self.settings_panel.get_enabled_tools().len(),
            raptor_indexing: self.raptor_indexing,
            raptor_status: self.raptor_status.clone(),
            raptor_progress: self.raptor_progress,
            raptor_stage: self.raptor_stage.clone(),
            raptor_start_time: self.raptor_start_time,
            input_mode: self.input_mode,
            tick_counter: self.tick_counter,
            indexing_prompt_selected: self.indexing_prompt_selected,
            indexing_prompt_dont_ask: self.indexing_prompt_dont_ask,
            show_autocomplete: self.show_autocomplete,
            autocomplete_selected: self.autocomplete_selected,
            auto_scroll: self.auto_scroll,
        };

        self.terminal.draw(|frame| {
            render_ui(frame, &render_data);
        })?;
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) {
        // Handle Ctrl+C - double press to exit
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            let now = Instant::now();
            if let Some(last) = self.last_ctrl_c {
                if now.duration_since(last) < Duration::from_millis(500) {
                    self.ctrl_c_count += 1;
                    if self.ctrl_c_count >= 2 {
                        self.should_quit = true;
                        return;
                    }
                } else {
                    self.ctrl_c_count = 1;
                }
            } else {
                self.ctrl_c_count = 1;
            }
            self.last_ctrl_c = Some(now);

            // Cancel processing on first Ctrl+C
            if self.is_processing {
                self.cancel_processing();
            }
            return;
        }

        // Handle Ctrl+N - cycle input mode (N = Next mode)
        if key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.input_mode = self.input_mode.next();
            // No actualizar status_message, el modo ya se muestra en la barra
            return;
        }

        // Global quit with Ctrl+Q
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        match self.screen {
            AppScreen::Chat => self.handle_chat_keys(key).await,
            AppScreen::Settings => self.handle_settings_keys(key),
            AppScreen::ModelConfig => self.handle_model_config_keys(key).await,
            AppScreen::IndexingPrompt => self.handle_indexing_prompt_keys(key).await,
            AppScreen::Confirmation => self.handle_confirmation_keys(key).await,
            AppScreen::Password => self.handle_password_keys(key).await,
        }
    }

    async fn handle_chat_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab if self.input_buffer.is_empty() => {
                self.screen = AppScreen::Settings;
            }
            KeyCode::Enter if !self.input_buffer.is_empty() && !self.is_processing => {
                // If autocomplete is showing, accept selected command
                if self.show_autocomplete {
                    let commands = self.get_filtered_commands();
                    if self.autocomplete_selected < commands.len() {
                        self.input_buffer = commands[self.autocomplete_selected].0.to_string();
                        self.cursor_position = self.input_buffer.len();
                        self.show_autocomplete = false;
                        return;
                    }
                }

                // Check for special commands
                let input = self.input_buffer.trim();
                if input == "/reindex" {
                    self.handle_reindex_command().await;
                } else if input == "/stats" {
                    self.handle_stats_command().await;
                } else if input == "/help" {
                    self.handle_help_command().await;
                } else {
                    self.start_processing().await;
                }
            }
            KeyCode::Up if self.show_autocomplete && !self.is_processing => {
                if self.autocomplete_selected > 0 {
                    self.autocomplete_selected -= 1;
                }
            }
            KeyCode::Down if self.show_autocomplete && !self.is_processing => {
                let commands = self.get_filtered_commands();
                if self.autocomplete_selected < commands.len().saturating_sub(1) {
                    self.autocomplete_selected += 1;
                }
            }
            KeyCode::Esc if self.show_autocomplete => {
                self.show_autocomplete = false;
                self.autocomplete_selected = 0;
            }
            KeyCode::Char(c) if !self.is_processing => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
                
                // Show autocomplete if input starts with /
                if self.input_buffer.starts_with('/') {
                    self.show_autocomplete = true;
                    self.autocomplete_selected = 0;
                } else {
                    self.show_autocomplete = false;
                }
            }
            KeyCode::Backspace if !self.is_processing => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left if self.cursor_position > 0 && !self.is_processing => {
                self.cursor_position -= 1;
            }
            KeyCode::Right
                if self.cursor_position < self.input_buffer.len() && !self.is_processing =>
            {
                self.cursor_position += 1;
            }
            KeyCode::Up => {
                // Scroll up - ensure first scroll always moves at least 1 line
                self.apply_user_scroll(-6);
            }
            KeyCode::Down => {
                // Scroll down - ensure first scroll always moves at least 1 line
                self.apply_user_scroll(6);
            }
            KeyCode::PageUp => {
                // Scroll up by page
                self.apply_user_scroll(-15);
            }
            KeyCode::PageDown => {
                // Scroll down by page
                self.apply_user_scroll(15);
            }
            KeyCode::Home if self.is_processing || self.input_buffer.is_empty() => {
                // Ir al inicio del chat
                self.apply_user_scroll_to_start();
            }
            KeyCode::End if self.is_processing || self.input_buffer.is_empty() => {
                // Ir al final del chat - reactivar auto-scroll
                self.apply_user_scroll_to_end();
            }
            KeyCode::Home if !self.is_processing => {
                self.cursor_position = 0;
            }
            KeyCode::End if !self.is_processing => {
                self.cursor_position = self.input_buffer.len();
            }
            _ => {}
        }
    }

    async fn start_processing(&mut self) {
        let user_input = std::mem::take(&mut self.input_buffer);
        self.cursor_position = 0;

        // Add user message immediately
        self.add_message(MessageSender::User, user_input.clone(), None);

        // Set processing state IMMEDIATELY - this triggers the spinner
        self.is_processing = true;
        self.processing_start = Some(Instant::now());
        self.last_event_time = Some(Instant::now());  // Initialize inactivity timeout
        self.status.set_state(StatusState::Working);
        self.status_message = t(Text::Processing).to_string();
        self.spinner = Spinner::thinking(); // Reset spinner
        self.auto_scroll = true; // Reactivar auto-scroll al empezar a procesar

        // Get enabled tools
        let _enabled_tools = self.settings_panel.get_enabled_tool_ids();

        // Create channel for background communication
        // Large buffer to handle streaming responses with many chunks (e.g., repository analysis)
        let (tx, rx) = mpsc::channel(5000);
        self.response_rx = Some(rx);

        // Create channel for progress updates
        let (progress_tx, mut progress_rx) = mpsc::channel::<TaskProgressInfo>(50);

        // Clone orchestrator for background task
        let orchestrator = Arc::clone(&self.orchestrator);
        let tx_clone = tx.clone();

        // Spawn task to forward progress updates to main channel
        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                if tx_clone
                    .send(AgentEvent::TaskProgress(progress))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Spawn background task based on orchestrator type
        // NOTE: We keep tx alive even after sending the response because the router
        // may have spawned internal tasks that will send streaming chunks/events
        let task_handle = tokio::spawn(async move {
            let bg_start = std::time::Instant::now();
            log_debug!("🔧 [BG-TASK] Starting background task for query: '{}'", user_input);

            // Determine orchestrator type without holding lock
            let is_router = {
                let orch = orchestrator.lock().await;
                matches!(&*orch, OrchestratorWrapper::Router(_))
            };

            if is_router {
                // Router orchestrator: configure channel without holding lock
                log_debug!("🔧 [BG-TASK] Using Router orchestrator");

                // Set event channel BEFORE acquiring lock (set_event_channel now takes &self)
                {
                    let orch = orchestrator.lock().await;
                    if let OrchestratorWrapper::Router(router_orch) = &*orch {
                        router_orch.set_event_channel_async(tx.clone()).await;
                        log_debug!("🔧 [BG-TASK] Event channel set at {}ms", bg_start.elapsed().as_millis());
                    }
                } // Lock released here

                // Now process WITHOUT holding the orchestrator lock
                log_debug!("🔧 [BG-TASK] Calling router_orch.process() at {}ms", bg_start.elapsed().as_millis());
                let process_start = std::time::Instant::now();

                let result = {
                    let orch = orchestrator.lock().await;
                    if let OrchestratorWrapper::Router(router_orch) = &*orch {
                        let timeout_result = tokio::time::timeout(
                            std::time::Duration::from_secs(120),
                            router_orch.process(&user_input)
                        ).await;
                        timeout_result
                    } else {
                        // Wrong orchestrator type - treat as error
                        Ok(Err(anyhow::anyhow!("Wrong orchestrator type")))
                    }
                }; // Lock released immediately after calling process

                log_debug!("🔧 [BG-TASK] router_orch.process() returned after {}ms (total: {}ms)",
                    process_start.elapsed().as_millis(),
                    bg_start.elapsed().as_millis());

                let msg = match result {
                    Ok(Ok(response)) => {
                        log_debug!("🔧 [BG-TASK] Response received successfully");
                        AgentEvent::Response(Ok(response))
                    },
                    Ok(Err(e)) => {
                        log_error!("Router orchestrator error: {}", e);
                        AgentEvent::Response(Err(e.to_string()))
                    }
                    Err(_) => {
                        let err_msg = "Timeout: El procesamiento tardó más de 120 segundos".to_string();
                        log_error!("{}", err_msg);
                        AgentEvent::Response(Err(err_msg))
                    }
                };
                // Use try_send to avoid blocking if channel is closed
                if tx.try_send(msg).is_err() {
                    log_debug!("🔧 [BG-TASK] Failed to send response (channel closed or full)");
                }
            } else {
                // Planning orchestrator: needs &mut, keep lock for entire operation
                let mut orch = orchestrator.lock().await;
                log_debug!("🔧 [BG-TASK] Acquired orchestrator lock at {}ms", bg_start.elapsed().as_millis());

                if let OrchestratorWrapper::Planning(planning_orch) = &mut *orch {
                    log_debug!("🔧 [BG-TASK] Using Planning orchestrator");
                    let result = planning_orch
                        .process_with_planning_and_progress(&user_input, Some(progress_tx))
                        .await;
                    log_debug!("🔧 [BG-TASK] Planning orchestrator completed at {}ms", bg_start.elapsed().as_millis());
                    let msg = match result {
                        Ok(response) => AgentEvent::PlanningResponse(Ok(response)),
                        Err(e) => {
                            log_error!("Planning orchestrator error: {}", e);
                            AgentEvent::PlanningResponse(Err(e.to_string()))
                        }
                    };
                    // Use try_send to avoid blocking if channel is closed
                    if tx.try_send(msg).is_err() {
                        log_debug!("🔧 [BG-TASK] Failed to send planning response (channel closed or full)");
                    }
                }
            } // Lock released here for planning

            log_debug!("🔧 [BG-TASK] Background task complete at {}ms", bg_start.elapsed().as_millis());

            // The channel naturally stays alive until the router task completes or
            // StreamEnd event is sent. No need to artificially keep it alive.
            // When both sides of the channel are done, it will close automatically.
        });

        // Store the task handle so we can cancel it later if needed
        self.background_task_handle = Some(task_handle);
    }

    /// Handle !reindex command to rebuild RAPTOR index
    async fn handle_reindex_command(&mut self) {
        let user_input = std::mem::take(&mut self.input_buffer);
        self.cursor_position = 0;

        // Add user command to messages
        self.add_message(MessageSender::User, user_input, None);

        // Check which orchestrator we're using
        let orchestrator = Arc::clone(&self.orchestrator);
        let orch = orchestrator.lock().await;
        
        match &*orch {
            OrchestratorWrapper::Router(_) => {
                // Router has built-in RAPTOR support
                drop(orch); // Release lock before async operation
                
                self.add_message(
                    MessageSender::System,
                    "🔄 Reconstruyendo índice RAPTOR...".to_string(),
                    None,
                );
                self.raptor_indexing = true;
                self.raptor_status = Some("Iniciando reindexación...".to_string());
                self.raptor_progress = Some((0, 0));
                self.raptor_stage = Some("Preparación".to_string());
                self.raptor_start_time = Some(Instant::now());
                self.raptor_eta = None;
                
                let orchestrator_clone = Arc::clone(&orchestrator);
                let (tx, rx) = mpsc::channel(100);
                self.raptor_rx = Some(rx);
                
                tokio::spawn(async move {
                    let mut orch = orchestrator_clone.lock().await;
                    if let OrchestratorWrapper::Router(router) = &mut *orch {
                        match router.rebuild_raptor().await {
                            Ok(summary) => {
                                let _ = tx.try_send(AgentEvent::RaptorStatus(summary));
                                let _ = tx.try_send(AgentEvent::RaptorComplete);
                            }
                            Err(e) => {
                                let _ = tx.try_send(AgentEvent::RaptorStatus(
                                    format!("❌ Error: {}", e)
                                ));
                                let _ = tx.try_send(AgentEvent::RaptorComplete);
                            }
                        }
                    }
                });
            }
            OrchestratorWrapper::Planning(_) => {
                drop(orch);
                self.add_message(
                    MessageSender::System,
                    "⚠️ El comando /reindex solo está disponible con RouterOrchestrator".to_string(),
                    None,
                );
            }
        }
    }

    /// Handle !stats command to show RAPTOR index statistics
    async fn handle_stats_command(&mut self) {
        let user_input = std::mem::take(&mut self.input_buffer);
        self.cursor_position = 0;

        // Add user command to messages
        self.add_message(MessageSender::User, user_input, None);

        // Get statistics from GLOBAL_STORE and current UI state
        let ui_indexing = self.raptor_indexing;
        let stats_msg = {
            let store = crate::raptor::persistence::GLOBAL_STORE.lock().unwrap();
            let chunk_count = store.chunk_map.len();
            let has_embeddings = !store.chunk_embeddings.is_empty();
            let indexed_files = store.indexed_files.len();
            let is_complete = store.indexing_complete && !ui_indexing;
            
            // Tree statistics (RAPTOR v2)
            let tree_exists = store.tree_root.is_some();
            let total_nodes = store.tree_nodes.len();
            let mut levels_map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            let mut max_depth = 0;
            
            for node in store.tree_nodes.values() {
                *levels_map.entry(node.level).or_insert(0) += 1;
                max_depth = max_depth.max(node.level);
            }
            
            // Determine actual status
            let status_text = if ui_indexing {
                "🔄 Indexando..."
            } else if is_complete && has_embeddings {
                "✅ Completo"
            } else if chunk_count > 0 && !has_embeddings {
                "⚠️ Solo lectura (sin embeddings)"
            } else if chunk_count == 0 {
                "❌ Sin indexar"
            } else {
                "🔄 En progreso"
            };
            
            let mut message = format!(
                "📊 Estadísticas del Índice RAPTOR v2\n\n\
                 📋 Archivos indexados: {}\n\
                 📝 Chunks almacenados: {}\n\
                 🧮 Embeddings: {}\n\
                 📌 Estado: {}\n\n",
                indexed_files,
                chunk_count,
                if has_embeddings { "✅ Generados" } else { "❌ No disponibles" },
                status_text,
            );
            
            // Add tree structure info if exists
            if tree_exists && total_nodes > 0 {
                message.push_str(&format!(
                    "🌲 Estructura Jerárquica:\n\
                     └─ Nodos totales: {}\n\
                     └─ Profundidad máxima: {} niveles\n",
                    total_nodes,
                    max_depth + 1
                ));
                
                // Show nodes per level
                let mut levels: Vec<_> = levels_map.into_iter().collect();
                levels.sort_by_key(|(level, _)| *level);
                for (level, count) in levels {
                    message.push_str(&format!("   • Nivel {}: {} nodos\n", level, count));
                }
                message.push('\n');
            }
            
            // Add footer message
            message.push_str(if chunk_count == 0 {
                "⚠️ No hay árbol construido. Usa /reindex para construir el índice."
            } else if !has_embeddings {
                "💡 El índice tiene texto pero aún no se han generado los embeddings.\n\
                 Espera a que termine la indexación o usa /reindex."
            } else if !tree_exists {
                "💡 Modo LITE: Embeddings sin jerarquía. Usa /reindex para construir el árbol completo."
            } else {
                "✓ Todo listo: árbol jerárquico activo para búsquedas contextuales."
            });
            
            message
        };

        self.add_message(
            MessageSender::System,
            stats_msg,
            None,
        );
    }

    /// Get available commands for autocomplete
    fn get_available_commands(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            // Code commands
            ("/code-review", "Análisis integral de código (linter + analyzer + deps)"),
            ("/analyze", "Análisis profundo de código y métricas"),
            ("/refactor", "Refactorización de código (próximamente)"),
            ("/format", "Formatear código con formatters automáticos"),
            ("/docs", "Generar documentación del proyecto"),
            
            // Testing
            ("/test", "Ejecutar tests con detección automática"),
            
            // Git
            ("/commit", "Crear commit con mensaje auto-generado"),
            ("/commit-push-pr", "Commit, push y crear PR"),
            
            // Context
            ("/deps", "Analizar dependencias del proyecto"),
            ("/search", "Buscar en código con regex"),
            ("/context", "Ver información del proyecto"),
            
            // System
            ("/plan", "Generar plan de ejecución (próximamente)"),
            ("/shell", "Ejecutar comando shell con seguridad"),
            ("/reindex", "Reconstruir índice RAPTOR"),
            ("/mode", "Cambiar modo del agente (próximamente)"),
            ("/help", "Mostrar ayuda de comandos"),
            
            // Legacy
            ("/stats", "Ver estadísticas del índice RAPTOR"),
        ]
    }

    fn get_filtered_commands(&self) -> Vec<(&'static str, &'static str)> {
        let all_commands = self.get_available_commands();
        
        // Filter commands based on input
        if self.input_buffer.len() > 1 {
            all_commands
                .into_iter()
                .filter(|(cmd, _)| cmd.starts_with(&self.input_buffer))
                .collect()
        } else {
            all_commands
        }
    }

    /// Handle !help command to show available commands
    async fn handle_help_command(&mut self) {
        let user_input = std::mem::take(&mut self.input_buffer);
        self.cursor_position = 0;

        // Add user command to messages
        self.add_message(MessageSender::User, user_input, None);

        let help_msg = "\
📚 Comandos Slash Disponibles\n\n\
📝 Código:\n\
  /code-review    - Análisis integral (linter + analyzer + deps)\n\
  /analyze <file> - Análisis profundo de código\n\
  /refactor       - Refactorización (próximamente)\n\
  /format <path>  - Formatear código\n\
  /docs [path]    - Generar documentación\n\n\
🧪 Testing:\n\
  /test [pattern] - Ejecutar tests\n\n\
🔧 Git:\n\
  /commit [msg]   - Commit con mensaje auto-generado\n\
  /commit-push-pr - Commit, push y crear PR\n\n\
🔍 Contexto:\n\
  /deps [path]    - Analizar dependencias\n\
  /search <query> - Buscar en código (--regex para regex)\n\
  /context        - Información del proyecto\n\n\
⚙️ Sistema:\n\
  /plan <task>    - Generar plan (próximamente)\n\
  /shell <cmd>    - Ejecutar comando shell\n\
  /reindex        - Reconstruir índice RAPTOR\n\
  /mode           - Cambiar modo (próximamente)\n\
  /help           - Mostrar esta ayuda\n\
  /stats          - Estadísticas del índice\n\n\
🎹 Atajos de Teclado:\n\
  Tab        - Cambiar entre Chat/Settings/ModelConfig\n\
  Esc        - Volver al chat\n\
  Ctrl+C     - Salir\n\
  ↑/↓        - Navegar autocompletado / Scroll chat\n\
  PgUp/PgDn  - Scroll página completa\n\
  Home/End   - Inicio/final del chat\n\n\
💡 Consejos:\n\
  • Escribe '/' para ver comandos disponibles\n\
  • Usa consultas naturales para análisis del proyecto\n\
  • El sistema mantiene contexto entre conversaciones";

        self.add_message(
            MessageSender::System,
            help_msg.to_string(),
            None,
        );
    }

    fn handle_settings_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                // Tab: Settings -> ModelConfig
                self.screen = AppScreen::ModelConfig;
            }
            KeyCode::Esc => {
                // Esc vuelve al chat
                self.screen = AppScreen::Chat;
            }
            KeyCode::Up => {
                self.settings_panel.move_up();
            }
            KeyCode::Down => {
                self.settings_panel.move_down();
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.settings_panel.toggle_selected();
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Toggle language
                use crate::i18n::{current_locale, set_locale, Locale};
                let new_locale = match current_locale() {
                    Locale::English => Locale::Spanish,
                    Locale::Spanish => Locale::English,
                };
                set_locale(new_locale);
                self.add_message(
                    MessageSender::System,
                    format!("Idioma cambiado a: {}", new_locale.display_name()),
                    None,
                );
            }
            _ => {}
        }
    }

    async fn handle_model_config_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                // Tab: ModelConfig -> Chat
                self.screen = AppScreen::Chat;
            }
            KeyCode::Esc => {
                if self.model_config_panel.editing {
                    self.model_config_panel.cancel_editing();
                } else {
                    // Esc vuelve al chat
                    self.screen = AppScreen::Chat;
                }
            }
            KeyCode::Up => {
                self.model_config_panel.move_up();
            }
            KeyCode::Down => {
                self.model_config_panel.move_down();
            }
            KeyCode::Left => {
                self.model_config_panel.handle_left();
            }
            KeyCode::Right => {
                self.model_config_panel.handle_right();
            }
            KeyCode::Enter => {
                if self.model_config_panel.editing {
                    self.model_config_panel.finish_editing();
                } else if let Some(action) = self.model_config_panel.activate_button() {
                    self.handle_model_config_action(action).await;
                } else {
                    self.model_config_panel.start_editing();
                }
            }
            KeyCode::Char(c) => {
                self.model_config_panel.handle_char(c);
            }
            KeyCode::Backspace => {
                self.model_config_panel.handle_backspace();
            }
            KeyCode::Delete => {
                self.model_config_panel.handle_delete();
            }
            _ => {}
        }
    }

    async fn handle_model_config_action(&mut self, action: ButtonAction) {
        match action {
            ButtonAction::Save => {
                // Validate and save configuration
                match self.model_config_panel.get_config().validate() {
                    Ok(config) => {
                        // Save configuration to file
                        let config_path = std::env::current_dir()
                            .unwrap_or_default()
                            .join("config.json");
                        
                        match serde_json::to_string_pretty(&config) {
                            Ok(json) => {
                                match std::fs::write(&config_path, json) {
                                    Ok(_) => {
                                        self.model_config_panel.set_status(
                                            format!("✓ Configuration saved to {:?}", config_path.file_name().unwrap()),
                                            false,
                                        );
                                        self.add_message(
                                            MessageSender::System,
                                            "Configuration saved. Restart to apply changes.".to_string(),
                                            None,
                                        );
                                    }
                                    Err(e) => {
                                        self.model_config_panel.set_status(
                                            format!("✗ Failed to save: {}", e),
                                            true,
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                self.model_config_panel.set_status(
                                    format!("✗ Serialization error: {}", e),
                                    true,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        self.model_config_panel.set_status(
                            format!("✗ Validation error: {}", e),
                            true,
                        );
                    }
                }
            }
            ButtonAction::TestConnection => {
                self.model_config_panel.set_status(
                    "Testing connection...".to_string(),
                    false,
                );
                
                // Test connection to Ollama
                let config = self.model_config_panel.get_config();
                let url_clone = format!("{}/api/tags", config.fast_model.url);
                let ollama_url = config.fast_model.url.clone();
                
                match reqwest::Client::new()
                    .get(&url_clone)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                {
                    Ok(response) if response.status().is_success() => {
                        self.model_config_panel.set_status(
                            "✓ Connection successful".to_string(),
                            false,
                        );
                        self.add_message(
                            MessageSender::System,
                            format!("Connected to Ollama at {}", ollama_url),
                            None,
                        );
                    }
                    Ok(response) => {
                        self.model_config_panel.set_status(
                            format!("✗ Connection failed: HTTP {}", response.status()),
                            true,
                        );
                    }
                    Err(e) => {
                        self.model_config_panel.set_status(
                            format!("✗ Connection failed: {}", e),
                            true,
                        );
                    }
                }
            }
        }
    }

    async fn handle_confirmation_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(cmd) = self.pending_command.take() {
                    self.add_message(MessageSender::System, format!("Executing: {}", cmd), None);
                }
                self.screen = AppScreen::Chat;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending_command = None;
                self.add_message(MessageSender::System, t(Text::Cancelled).to_string(), None);
                self.screen = AppScreen::Chat;
            }
            _ => {}
        }
    }

    async fn handle_password_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.password_input.clear();
                self.password_error = None;
                self.screen = AppScreen::Chat;
            }
            KeyCode::Esc => {
                self.password_input.clear();
                self.password_error = None;
                self.pending_command = None;
                self.screen = AppScreen::Chat;
            }
            KeyCode::Char(c) => {
                self.password_input.push(c);
            }
            KeyCode::Backspace => {
                self.password_input.pop();
            }
            _ => {}
        }
    }

    async fn handle_indexing_prompt_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.indexing_prompt_selected = self.indexing_prompt_selected.prev();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.indexing_prompt_selected = self.indexing_prompt_selected.next();
            }
            KeyCode::Char(' ') => {
                // Toggle checkbox
                self.indexing_prompt_dont_ask = !self.indexing_prompt_dont_ask;
            }
            KeyCode::Enter => {
                // Execute selected indexing option
                self.execute_indexing_option().await;
                self.screen = AppScreen::Chat;
            }
            KeyCode::Esc => {
                // Cancel - don't index
                self.add_message(
                    MessageSender::System,
                    "Indexing cancelled. Limited functionality available.".to_string(),
                    None,
                );
                self.screen = AppScreen::Chat;
            }
            _ => {}
        }
    }

    async fn execute_indexing_option(&mut self) {
        match self.indexing_prompt_selected {
            IndexingOption::RagNow => {
                // Execute RAG synchronously with progress
                self.add_message(
                    MessageSender::System,
                    "🔍 Starting RAG quick indexing...".to_string(),
                    None,
                );
                
                // Perform quick index synchronously
                let project_path = std::env::current_dir().unwrap_or_default();
                let path_clone = project_path.clone();
                
                match tokio::task::spawn_blocking(move || {
                    crate::raptor::builder::quick_index_sync(&path_clone, 1500, 200)
                }).await {
                    Ok(Ok(chunks)) => {
                        self.add_message(
                            MessageSender::System,
                            format!("✓ RAG quick index complete: {} chunks. Starting RAPTOR in background...", chunks),
                            None,
                        );
                    }
                    _ => {
                        self.add_message(
                            MessageSender::System,
                            "⚠ RAG indexing had issues. Starting RAPTOR anyway...".to_string(),
                            None,
                        );
                    }
                }

                // Start RAPTOR in background
                self.start_background_raptor_indexing();
            }
            IndexingOption::RaptorOnly => {
                // Only start RAPTOR in background
                self.add_message(
                    MessageSender::System,
                    "📊 Starting RAPTOR indexing in background...".to_string(),
                    None,
                );
                self.start_background_raptor_indexing();
            }
            IndexingOption::Later => {
                // Don't index now
                self.add_message(
                    MessageSender::System,
                    "Indexing postponed. Use limited functionality mode.".to_string(),
                    None,
                );
            }
        }

        // Save preference if checkbox was checked
        if self.indexing_prompt_dont_ask {
            // Save preference to .neuro-agent/preferences.json
            let prefs_dir = std::env::current_dir()
                .unwrap_or_default()
                .join(".neuro-agent");
            
            if std::fs::create_dir_all(&prefs_dir).is_ok() {
                let prefs_file = prefs_dir.join("preferences.json");
                let prefs = serde_json::json!({
                    "skip_indexing_prompt": true,
                    "default_indexing_option": match self.indexing_prompt_selected {
                        IndexingOption::RagNow => "rag_now",
                        IndexingOption::RaptorOnly => "raptor_only",
                        IndexingOption::Later => "later",
                    }
                });
                
                if let Ok(json) = serde_json::to_string_pretty(&prefs) {
                    if std::fs::write(&prefs_file, json).is_ok() {
                        self.add_message(
                            MessageSender::System,
                            "Preference saved. Won't ask again for this project.".to_string(),
                            None,
                        );
                    }
                }
            }
        }
    }

    fn cancel_processing(&mut self) {
        // Abort the background task if it's running
        if let Some(handle) = self.background_task_handle.take() {
            handle.abort();
        }

        self.is_processing = false;
        self.processing_start = None;
        self.last_event_time = None;
        self.current_thinking = None;
        self.response_rx = None;
        self.status.set_state(StatusState::Warning);
        self.status_message = t(Text::Cancelled).to_string();
        self.add_message(MessageSender::System, t(Text::Cancelled).to_string(), None);
    }

    fn cleanup_processing(&mut self) {
        // Clean up background task and processing state
        self.background_task_handle = None;
        self.is_processing = false;
        self.processing_start = None;
        self.last_event_time = None;
        self.current_thinking = None;
        self.status_message = t(Text::Ready).to_string();
        self.status.set_state(StatusState::Idle);
        self.response_rx = None;

        // Clean up streaming buffer
        self.streaming_buffer = None;
        self.streaming_chunks_count = 0;
    }

    fn add_message(&mut self, sender: MessageSender, content: String, tool_name: Option<String>) {
        self.messages.push(DisplayMessage {
            sender,
            content,
            timestamp: Instant::now(),
            is_streaming: false,
            tool_name,
        });
        // Note: auto_scroll is handled dynamically in render_chat_output
        // When auto_scroll=true, it always scrolls to the bottom regardless of scroll_offset
    }

    /// Apply a user-initiated scroll. This always disables auto-scroll and makes
    /// sure the view moves at least one line so the first scroll isn't ignored.
    fn apply_user_scroll(&mut self, delta: isize) {
        self.auto_scroll = false;

        if delta < 0 {
            // Scroll up
            let move_by = (-delta) as usize;
            let new_offset = if move_by == 0 { 1 } else { move_by };
            self.scroll_offset = self.scroll_offset.saturating_sub(new_offset);
        } else if delta > 0 {
            let move_by = delta as usize;
            let new_offset = if move_by == 0 { 1 } else { move_by };
            self.scroll_offset = self.scroll_offset.saturating_add(new_offset);
        }
    }

    fn apply_user_scroll_to_start(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = 0;
    }

    fn apply_user_scroll_to_end(&mut self) {
        // Enable auto_scroll to always show the bottom
        // The scroll_offset value is ignored when auto_scroll=true
        self.auto_scroll = true;
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                // Scroll hacia arriba - 6 líneas por evento (más perceptible)
                self.apply_user_scroll(-6);
            }
            MouseEventKind::ScrollDown => {
                // Scroll hacia abajo - 6 líneas por evento (más perceptible)
                self.apply_user_scroll(6);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Click izquierdo - desactiva auto-scroll
                self.apply_user_scroll(0);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Drag para selección - el terminal maneja esto nativo con Shift
            }
            _ => {}
        }
    }
}

impl Drop for ModernApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

// ============================================================================
// Render Data & Static Rendering Functions
// ============================================================================

struct RenderData<'a> {
    theme: Theme,
    screen: AppScreen,
    status_render: (&'static str, (u8, u8, u8)),
    status_message: String,
    messages: &'a [DisplayMessage],
    input_buffer: String,
    scroll_offset: usize,
    is_processing: bool,
    processing_start: Option<Instant>,
    spinner_frame: String,
    settings_tools: Vec<ToolConfig>,
    settings_selected: usize,
    model_config_panel: &'a ModelConfigPanel,
    pending_command: Option<String>,
    password_input_len: usize,
    password_error: Option<String>,
    enabled_tools_count: usize,
    raptor_indexing: bool,
    raptor_status: Option<String>,
    raptor_progress: Option<(usize, usize)>,
    raptor_stage: Option<String>,
    raptor_start_time: Option<Instant>,
    input_mode: InputMode,
    tick_counter: u64,
    indexing_prompt_selected: IndexingOption,
    indexing_prompt_dont_ask: bool,
    show_autocomplete: bool,
    autocomplete_selected: usize,
    auto_scroll: bool,
}

fn render_ui(frame: &mut Frame, data: &RenderData) {
    let area = frame.area();

    frame.render_widget(Block::default().style(data.theme.base_style()), area);

    match data.screen {
        AppScreen::Chat => {
            // Two-column layout: main chat + history sidebar
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(75), // Main area
                    Constraint::Percentage(25), // History sidebar
                ])
                .split(area);

            // Left column: output (top) + input (bottom) + status
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(10),   // Output (scrollable)
                    Constraint::Length(5), // Input (3 lines + borders)
                    Constraint::Length(1), // Status bar
                ])
                .split(columns[0]);

            render_chat_output(frame, left_chunks[0], data);
            render_input(frame, left_chunks[1], data);
            render_status_bar(frame, left_chunks[2], data);

            // Right column: task history
            render_history_sidebar(frame, columns[1], data);
        }
        AppScreen::Settings => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);

            render_header(frame, chunks[0], data);
            render_settings(frame, chunks[1], data);
            render_settings_footer(frame, chunks[2], data);
            render_status_bar(frame, chunks[3], data);
        }
        AppScreen::ModelConfig => {
            // Render model configuration panel
            data.model_config_panel.render(area, frame.buffer_mut());
        }
        AppScreen::IndexingPrompt => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(10),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
                .split(area);

            render_chat_output(frame, chunks[0], data);
            render_input(frame, chunks[1], data);
            render_status_bar(frame, chunks[2], data);
            render_indexing_prompt_modal(frame, area, data);
        }
        AppScreen::Confirmation => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(10),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
                .split(area);

            render_chat_output(frame, chunks[0], data);
            render_input(frame, chunks[1], data);
            render_status_bar(frame, chunks[2], data);
            render_confirmation_modal(frame, area, data);
        }
        AppScreen::Password => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(10),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
                .split(area);

            render_chat_output(frame, chunks[0], data);
            render_input(frame, chunks[1], data);
            render_status_bar(frame, chunks[2], data);
            render_password_modal(frame, area, data);
        }
    }
}

fn render_header(frame: &mut Frame, area: Rect, data: &RenderData) {
    let (icon, color) = data.status_render;
    let color = Color::Rgb(color.0, color.1, color.2);

    let locale_str = match current_locale() {
        Locale::English => "🇺🇸",
        Locale::Spanish => "🇪🇸",
    };

    // Show animated spinner in header when processing
    let status_display = if data.is_processing {
        format!("{} {}", data.spinner_frame, data.status_message)
    } else {
        format!("{} {}", icon, data.status_message)
    };

    let title_line = Line::from(vec![
        Span::styled(" neuro ", data.theme.title_style()),
        Span::styled("│", data.theme.muted_style()),
        Span::styled(
            format!(" {} ", status_display),
            if data.is_processing {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            },
        ),
    ]);

    let right_info = format!("{} {} ", locale_str, current_locale().display_name());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if data.is_processing {
            data.theme.warning_style()
        } else {
            data.theme.border_style(false)
        })
        .border_type(ratatui::widgets::BorderType::Rounded);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(Paragraph::new(title_line), inner);
    frame.render_widget(
        Paragraph::new(right_info)
            .alignment(Alignment::Right)
            .style(data.theme.muted_style()),
        inner,
    );
}

/// Parse a line of text with basic markdown support (bold, italic, code)
fn parse_markdown_line<'a>(text: &'a str, base_style: Style, accent_style: Style) -> Vec<Span<'a>> {
    let mut spans: Vec<Span> = Vec::new();
    let mut current_pos = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    while current_pos < len {
        // Check for **bold**
        if current_pos + 1 < len && chars[current_pos] == '*' && chars[current_pos + 1] == '*' {
            if let Some(end) = find_closing(&chars, current_pos + 2, "**") {
                let bold_text: String = chars[current_pos + 2..end].iter().collect();
                spans.push(Span::styled(
                    bold_text,
                    accent_style.add_modifier(Modifier::BOLD),
                ));
                current_pos = end + 2;
                continue;
            }
        }

        // Check for *italic* or _italic_
        if chars[current_pos] == '*' || chars[current_pos] == '_' {
            let marker = chars[current_pos];
            if let Some(end) = find_closing_char(&chars, current_pos + 1, marker) {
                let italic_text: String = chars[current_pos + 1..end].iter().collect();
                spans.push(Span::styled(
                    italic_text,
                    base_style.add_modifier(Modifier::ITALIC),
                ));
                current_pos = end + 1;
                continue;
            }
        }

        // Check for `code`
        if chars[current_pos] == '`' {
            if let Some(end) = find_closing_char(&chars, current_pos + 1, '`') {
                let code_text: String = chars[current_pos + 1..end].iter().collect();
                spans.push(Span::styled(code_text, Style::default().fg(Color::Cyan)));
                current_pos = end + 1;
                continue;
            }
        }

        // Regular character - collect until next special char
        let start = current_pos;
        while current_pos < len && !matches!(chars[current_pos], '*' | '_' | '`') {
            current_pos += 1;
        }
        if start < current_pos {
            let regular_text: String = chars[start..current_pos].iter().collect();
            spans.push(Span::styled(regular_text, base_style));
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base_style));
    }

    spans
}

fn find_closing(chars: &[char], start: usize, pattern: &str) -> Option<usize> {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let pattern_len = pattern_chars.len();

    (start..chars.len().saturating_sub(pattern_len - 1)).find(|&i| chars[i..i + pattern_len] == pattern_chars[..])
}

fn find_closing_char(chars: &[char], start: usize, marker: char) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == marker)
}

fn render_chat_output(frame: &mut Frame, area: Rect, data: &RenderData) {

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            data.theme
                .border_style(data.screen == AppScreen::Chat && !data.is_processing),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(" Output ", data.theme.primary_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Add padding inside the block
    let padded_inner = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    let mut lines: Vec<Line> = Vec::new();

    for msg in data.messages {
        let (icon, label, style) = match msg.sender {
            MessageSender::User => (Icons::USER, "Tú", data.theme.user_style()),
            MessageSender::Assistant => {
                (Icons::ASSISTANT, "Asistente", data.theme.assistant_style())
            }
            MessageSender::System => (Icons::SYSTEM, "Sistema", data.theme.system_style()),
            MessageSender::Tool => (Icons::TOOL, "Tarea", data.theme.tool_style()),
        };

        // Only show header for non-System messages
        if !matches!(msg.sender, MessageSender::System) {
            // Header with icon and label
            let header = if let Some(ref tool) = msg.tool_name {
                Line::from(vec![
                    Span::styled(format!("{} ", icon), style),
                    Span::styled(label.to_string(), style.add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" [{}]", tool), data.theme.code_style()),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!("{} ", icon), style),
                    Span::styled(label.to_string(), style.add_modifier(Modifier::BOLD)),
                ])
            };
            lines.push(header);
        }

        // Parse content with markdown support
        // PERFORMANCE FIX: Limit lines rendered during streaming to prevent UI freeze
        let content_lines: Vec<&str> = msg.content.lines().collect();
        let lines_to_render = if msg.is_streaming && content_lines.len() > 500 {
            // During streaming, only show last 500 lines to keep rendering fast
            &content_lines[content_lines.len() - 500..]
        } else {
            // Not streaming or small enough: render everything
            &content_lines[..]
        };

        if msg.is_streaming && content_lines.len() > 500 {
            // Show indicator that we're truncating
            let truncated_line = Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("... (mostrando últimas 500 de {} líneas) ...", content_lines.len()),
                    data.theme.system_style().add_modifier(Modifier::ITALIC)
                )
            ]);
            lines.push(truncated_line);
        }

        for content_line in lines_to_render {
            let spans = parse_markdown_line(content_line, style, data.theme.accent_style());
            // For System messages, no indent; for others, 3 spaces alignment
            let line_spans = if matches!(msg.sender, MessageSender::System) {
                spans
            } else {
                let mut indented = vec![Span::raw("   ")]; // 3 spaces for alignment with icon
                indented.extend(spans);
                indented
            };
            lines.push(Line::from(line_spans));
        }

        // Add blank line only for non-System messages (System messages are compact)
        if !matches!(msg.sender, MessageSender::System) {
            lines.push(Line::from(""));
        }
    }

    // Add simple spinner when processing
    if data.is_processing {
        let elapsed = data
            .processing_start
            .map(|start| start.elapsed().as_secs())
            .unwrap_or(0);

        // Show detailed status message instead of generic "Processing..."
        let progress_text = if data.status_message.contains("Tarea")
            || data.status_message.contains("RAPTOR")
            || data.status_message.contains(":")
        {
            format!("{} ({}s)", data.status_message, elapsed)
        } else {
            format!("Procesando... ({}s)", elapsed)
        };

        // Cursor parpadeante para indicar actividad
        let cursor_char = if (data.tick_counter / 2).is_multiple_of(2) {
            "▌"
        } else {
            " "
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<2}", Icons::ASSISTANT),
                data.theme.assistant_style(),
            ),
            Span::styled(&data.spinner_frame, Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(" {}", progress_text),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                format!(" {}", cursor_char),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let visible_lines = padded_inner.height as usize;

    // Calcular líneas reales considerando el wrap
    // Cada línea puede ocupar más de una fila si es más ancha que el área
    let wrap_width = padded_inner.width as usize;
    let mut total_wrapped_lines: usize = 0;
    for (_idx, line) in lines.iter().enumerate() {
        let line_width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        if line_width == 0 {
            total_wrapped_lines += 1; // Línea vacía
        } else {
            // Cuántas líneas ocupa después del wrap
            total_wrapped_lines += (line_width + wrap_width - 1) / wrap_width.max(1);
        }
    }

    let total_lines = total_wrapped_lines;

    // Calculate scroll with proper clamping
    // When auto_scroll is true, always scroll to the bottom
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = if data.auto_scroll {
        max_scroll  // Always show the last visible lines
    } else {
        data.scroll_offset.min(max_scroll)  // Use manual scroll offset
    };

    let paragraph = Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, padded_inner);

    // Show scroll indicator
    if total_lines > visible_lines {
        let scroll_indicator = format!(
            " [{}/{}] ",
            (scroll + visible_lines).min(total_lines),
            total_lines
        );
        let indicator_area = Rect {
            x: area.x + area.width - scroll_indicator.len() as u16 - 1,
            y: area.y,
            width: scroll_indicator.len() as u16,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(scroll_indicator).style(Style::default().fg(Color::DarkGray)),
            indicator_area,
        );
    }
}

fn render_history_sidebar(frame: &mut Frame, area: Rect, data: &RenderData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.border_style(false))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(" History ", data.theme.primary_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show recent messages as history
    let mut history_items: Vec<ListItem> = Vec::new();

    for (_idx, msg) in data.messages.iter().enumerate().rev().take(50) {
        let (icon, style) = match msg.sender {
            MessageSender::User => ("→", data.theme.user_style()),
            MessageSender::Assistant => ("←", data.theme.assistant_style()),
            MessageSender::System => ("•", data.theme.system_style()),
            MessageSender::Tool => ("🔧", data.theme.tool_style()),
        };

        let preview = if msg.content.len() > 40 {
            format!("{}...", &msg.content[..37])
        } else {
            msg.content.clone()
        };
        let preview_line = preview.lines().next().unwrap_or("").to_string();

        let item_text = if let Some(ref tool) = msg.tool_name {
            Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(format!("[{}]", tool), data.theme.code_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(preview_line, style),
            ])
        };

        history_items.push(ListItem::new(item_text));
    }

    let list = List::new(history_items);
    frame.render_widget(list, inner);
}

fn render_input(frame: &mut Frame, area: Rect, data: &RenderData) {
    let is_focused = data.screen == AppScreen::Chat && !data.is_processing;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if data.is_processing {
            data.theme.warning_style()
        } else {
            data.theme.border_style(is_focused)
        })
        .border_type(if is_focused && !data.is_processing {
            ratatui::widgets::BorderType::Thick
        } else {
            ratatui::widgets::BorderType::Rounded
        })
        .title(if data.is_processing {
            Span::styled(" Input ", data.theme.muted_style())
        } else {
            Span::styled(" Input ", data.theme.primary_style())
        });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Multi-line input with wrap
    let input_text = if data.is_processing {
        // Show the input that was sent while processing
        if data.input_buffer.is_empty() {
            vec![Line::from(Span::styled(
                "Procesando... (Presiona Ctrl+C para cancelar)",
                Style::default().fg(Color::Yellow),
            ))]
        } else {
            data.input_buffer
                .lines()
                .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::DarkGray))))
                .collect()
        }
    } else if data.input_buffer.is_empty() {
        vec![Line::from(Span::styled(
            "Escribe tu mensaje... (Enter para enviar, ↑↓ scroll)",
            data.theme.muted_style(),
        ))]
    } else {
        // Split input by lines and wrap
        data.input_buffer
            .lines()
            .map(|line| Line::from(Span::styled(line, data.theme.base_style())))
            .collect()
    };

    let paragraph = Paragraph::new(input_text).wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);

    // Show autocomplete popup if needed
    if data.show_autocomplete && !data.is_processing {
        render_autocomplete_popup(frame, area, data);
    }

    // Show blinking cursor (always when focused and not processing)
    if is_focused && !data.is_processing {
        // Fast blink based on tick counter (every ~200ms)
        let show_cursor = (data.tick_counter / 2).is_multiple_of(2);

        if show_cursor {
            let cursor_char = if data.input_buffer.is_empty() {
                "▌" // Block cursor when empty
            } else {
                "▎" // Line cursor when typing
            };

            // Calculate cursor position
            let cursor_y = if data.input_buffer.is_empty() {
                inner.y
            } else {
                inner.y
                    + (data.input_buffer.lines().count().saturating_sub(1) as u16)
                        .min(inner.height.saturating_sub(1))
            };
            let cursor_x = if data.input_buffer.is_empty() {
                inner.x
            } else {
                inner.x
                    + (data.input_buffer.lines().last().unwrap_or("").len() as u16)
                        .min(inner.width.saturating_sub(1))
            };

            frame.render_widget(
                Paragraph::new(cursor_char).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Rect {
                    x: cursor_x,
                    y: cursor_y,
                    width: 1,
                    height: 1,
                },
            );
        }
    }
}

fn render_settings(frame: &mut Frame, area: Rect, data: &RenderData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.border_style(true))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            format!(" {} ", t(Text::SettingsTitle)),
            data.theme.primary_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tools_block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.border_style(false))
        .title(Span::styled(
            format!(" {} ", t(Text::ToolsTitle)),
            data.theme.accent_style(),
        ));

    let items: Vec<ListItem> = data
        .settings_tools
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let is_selected = i == data.settings_selected;
            let checkbox = if tool.enabled {
                Icons::CHECK
            } else {
                Icons::UNCHECK
            };
            let status_text = if tool.enabled {
                t(Text::ToolsEnabled)
            } else {
                t(Text::ToolsDisabled)
            };
            let status_style = if tool.enabled {
                data.theme.success_style()
            } else {
                data.theme.muted_style()
            };

            let content = Line::from(vec![
                Span::raw(if is_selected { "► " } else { "  " }),
                Span::styled(
                    checkbox,
                    if tool.enabled {
                        data.theme.success_style()
                    } else {
                        data.theme.muted_style()
                    },
                ),
                Span::raw(" "),
                Span::styled(tool.icon, data.theme.accent_style()),
                Span::raw(" "),
                Span::styled(
                    &tool.name,
                    if is_selected {
                        data.theme.primary_style().add_modifier(Modifier::BOLD)
                    } else {
                        data.theme.base_style()
                    },
                ),
                Span::raw(" - "),
                Span::styled(&tool.description, data.theme.muted_style()),
                Span::raw(" ["),
                Span::styled(status_text, status_style),
                Span::raw("]"),
            ]);

            let style = if is_selected {
                data.theme.selection_style()
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(tools_block);
    frame.render_widget(list, inner);
}

fn render_settings_footer(frame: &mut Frame, area: Rect, data: &RenderData) {
    let current_lang = crate::i18n::current_locale().display_name();
    let shortcuts = Line::from(vec![
        Span::styled(" ↑↓ ", data.theme.shortcut_key_style()),
        Span::styled("Navigate", data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Space ", data.theme.shortcut_key_style()),
        Span::styled(t(Text::ToggleTool), data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" L ", data.theme.shortcut_key_style()),
        Span::styled(
            format!("Idioma: {}", current_lang),
            data.theme.shortcut_desc_style(),
        ),
        Span::raw("  "),
        Span::styled(" Tab ", data.theme.shortcut_key_style()),
        Span::styled(t(Text::BackToChat), data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Q ", data.theme.shortcut_key_style()),
        Span::styled(t(Text::PressQToQuit), data.theme.shortcut_desc_style()),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.border_style(false))
        .border_type(ratatui::widgets::BorderType::Rounded);

    frame.render_widget(
        Paragraph::new(shortcuts)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

fn render_status_bar(frame: &mut Frame, area: Rect, data: &RenderData) {
    let status_text = if data.is_processing {
        format!("{} {}", data.spinner_frame, data.status_message)
    } else {
        data.status_message.clone()
    };

    // Mode indicator
    let mode_info = format!(
        "{} {}",
        data.input_mode.icon(),
        data.input_mode.display_name()
    );

    let tools_info = format!("{} {}", Icons::TOOL, data.enabled_tools_count);

    // RAPTOR status con progreso detallado y ETA
    let raptor_info = if data.raptor_indexing {
        let mut info = format!("{} Indexando", data.spinner_frame);
        
        // Mostrar etapa si está disponible
        if let Some(stage) = &data.raptor_stage {
            info = format!("{} {}", data.spinner_frame, stage);
        }
        
        if let Some((current, total)) = data.raptor_progress {
            if total > 0 {
                let percentage = (current as f32 / total as f32 * 100.0) as usize;
                info.push_str(&format!(" [{}/{}] {}%", current, total, percentage));
                
                // Calcular ETA si tenemos tiempo de inicio y progreso
                if let Some(start_time) = data.raptor_start_time {
                    let elapsed = start_time.elapsed();
                    if current > 0 {
                        let time_per_item = elapsed.as_secs_f64() / current as f64;
                        let remaining = (total - current) as f64 * time_per_item;
                        
                        if remaining > 60.0 {
                            let mins = (remaining / 60.0).ceil() as u64;
                            info.push_str(&format!(" ~{}m", mins));
                        } else if remaining > 0.0 {
                            let secs = remaining.ceil() as u64;
                            info.push_str(&format!(" ~{}s", secs));
                        }
                    }
                }
            } else if current > 0 {
                info.push_str(&format!(" {}", current));
            }
        }
        
        info
    } else if let Some(ref status) = data.raptor_status {
        if status.contains("✓") || status.contains("listo") || status.contains("Listo") {
            "📊 ✓ Indexado".to_string()
        } else {
            format!("📊 {}", status)
        }
    } else {
        String::new()
    };

    let mut spans = vec![
        Span::styled(
            format!(" {} ", mode_info),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" {} ", status_text),
            if data.is_processing {
                Style::default().fg(Color::Yellow)
            } else {
                data.theme.muted_style()
            },
        ),
        Span::raw("│"),
        Span::styled(format!(" {} ", tools_info), data.theme.muted_style()),
    ];

    if !raptor_info.is_empty() {
        spans.push(Span::raw("│"));
        spans.push(Span::styled(
            format!(" {} ", raptor_info),
            if data.raptor_indexing {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Green)
            },
        ));
    }

    // Show scroll indicator when user has manually scrolled (auto_scroll disabled)
    if !data.auto_scroll {
        spans.push(Span::raw("│"));
        spans.push(Span::styled(
            " Scroll ",
            data.theme.muted_style(),
        ));
        // Add a short hint
        spans.push(Span::raw("│"));
        spans.push(Span::styled(
            "Tip: Use End to resume",
            data.theme.muted_style(),
        ));
    }

    // Shortcuts hint
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        "^N:modo  ^C×2:salir",
        Style::default().fg(Color::DarkGray),
    ));

    let line = Line::from(spans);

    frame.render_widget(Paragraph::new(line).style(data.theme.base_style()), area);
}

fn render_indexing_prompt_modal(frame: &mut Frame, area: Rect, data: &RenderData) {
    let modal_area = centered_rect(70, 60, area);
    frame.render_widget(Clear, modal_area);

    let mut content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  📊 ", data.theme.accent_style()),
            Span::styled(
                "Welcome to Neuro Agent",
                data.theme.title_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  This appears to be the first time running in this project directory.",
            data.theme.base_style(),
        )]),
        Line::from(vec![Span::styled(
            "  Would you like to index the codebase for enhanced AI assistance?",
            data.theme.base_style(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Indexing Options:",
            data.theme.primary_style().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    // Render options
    for option in [IndexingOption::RagNow, IndexingOption::RaptorOnly, IndexingOption::Later] {
        let is_selected = option == data.indexing_prompt_selected;
        let cursor = if is_selected { "▸ " } else { "  " };

        let line = Line::from(vec![
            Span::styled(
                format!("  {}", cursor),
                if is_selected {
                    data.theme.accent_style()
                } else {
                    data.theme.muted_style()
                },
            ),
            Span::styled(
                option.display_name(),
                if is_selected {
                    data.theme.primary_style().add_modifier(Modifier::BOLD)
                } else {
                    data.theme.base_style()
                },
            ),
        ]);
        content.push(line);

        // Add description for selected option
        if is_selected {
            let desc_line = Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    option.description(),
                    data.theme.muted_style().add_modifier(Modifier::ITALIC),
                ),
            ]);
            content.push(desc_line);
        }

        content.push(Line::from(""));
    }

    // Checkbox
    let checkbox = if data.indexing_prompt_dont_ask {
        "[✓]"
    } else {
        "[ ]"
    };
    content.push(Line::from(vec![
        Span::styled("  ", data.theme.base_style()),
        Span::styled(
            checkbox,
            if data.indexing_prompt_dont_ask {
                data.theme.success_style()
            } else {
                data.theme.muted_style()
            },
        ),
        Span::raw(" "),
        Span::styled(
            "Don't ask again for this project",
            data.theme.base_style(),
        ),
    ]));

    content.push(Line::from(""));
    content.push(Line::from(""));

    // Shortcuts
    content.push(Line::from(vec![
        Span::styled("  ↑↓ ", data.theme.shortcut_key_style()),
        Span::styled("Navigate", data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Space ", data.theme.shortcut_key_style()),
        Span::styled("Toggle Checkbox", data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Enter ", data.theme.shortcut_key_style()),
        Span::styled("Confirm", data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Esc ", data.theme.shortcut_key_style()),
        Span::styled("Skip", data.theme.shortcut_desc_style()),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.primary_style())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            " Project Indexing ",
            data.theme.primary_style().add_modifier(Modifier::BOLD),
        ))
        .style(data.theme.base_style());

    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .alignment(Alignment::Left),
        modal_area,
    );
}

fn render_confirmation_modal(frame: &mut Frame, area: Rect, data: &RenderData) {
    let modal_area = centered_rect(60, 30, area);
    frame.render_widget(Clear, modal_area);

    let command = data.pending_command.as_deref().unwrap_or("");

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {} ", Icons::WARNING), data.theme.warning_style()),
            Span::styled(
                t(Text::DangerousCommand),
                data.theme.warning_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  $ {}", command),
            data.theme.code_style(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}", t(Text::ConfirmCommand)),
            data.theme.base_style(),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " [Y] ",
                data.theme.success_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled("Yes", data.theme.success_style()),
            Span::raw("    "),
            Span::styled(
                " [N] ",
                data.theme.error_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled("No", data.theme.error_style()),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.warning_style())
        .border_type(ratatui::widgets::BorderType::Double)
        .title(Span::styled(
            format!(" {} ", t(Text::ConfirmCommand)),
            data.theme.warning_style().add_modifier(Modifier::BOLD),
        ))
        .style(data.theme.base_style());

    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .alignment(Alignment::Center),
        modal_area,
    );
}

fn render_password_modal(frame: &mut Frame, area: Rect, data: &RenderData) {
    let modal_area = centered_rect(50, 25, area);
    frame.render_widget(Clear, modal_area);

    let masked_password: String = "*".repeat(data.password_input_len);

    let mut content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("  {} ", Icons::LOCK), data.theme.warning_style()),
            Span::styled(
                t(Text::PasswordRequired),
                data.theme.base_style().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}: ", t(Text::EnterPassword)),
                data.theme.muted_style(),
            ),
            Span::styled(format!("[{}]", masked_password), data.theme.accent_style()),
            Span::styled("▎", data.theme.accent_style()),
        ]),
    ];

    if let Some(ref error) = data.password_error {
        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            format!("  {} {}", Icons::ERROR, error),
            data.theme.error_style(),
        )]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.warning_style())
        .border_type(ratatui::widgets::BorderType::Double)
        .title(Span::styled(
            format!(" {} {} ", Icons::LOCK, t(Text::PasswordRequired)),
            data.theme.warning_style(),
        ))
        .style(data.theme.base_style());

    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .alignment(Alignment::Center),
        modal_area,
    );
}
/// Render autocomplete popup for commands
fn render_autocomplete_popup(frame: &mut Frame, input_area: Rect, data: &RenderData) {
    // Get available commands (all slash commands)
    let commands = vec![
        // Code commands
        ("/code-review", "Análisis integral de código (linter + analyzer + deps)"),
        ("/analyze", "Análisis profundo de código y métricas"),
        ("/refactor", "Refactorización de código (próximamente)"),
        ("/format", "Formatear código con formatters automáticos"),
        ("/docs", "Generar documentación del proyecto"),
        
        // Testing
        ("/test", "Ejecutar tests con detección automática"),
        
        // Git
        ("/commit", "Crear commit con mensaje auto-generado"),
        ("/commit-push-pr", "Commit, push y crear PR"),
        
        // Context
        ("/deps", "Analizar dependencias del proyecto"),
        ("/search", "Buscar en código con regex"),
        ("/context", "Ver información del proyecto"),
        
        // System
        ("/plan", "Generar plan de ejecución (próximamente)"),
        ("/shell", "Ejecutar comando shell con seguridad"),
        ("/reindex", "Reconstruir índice RAPTOR"),
        ("/mode", "Cambiar modo del agente (próximamente)"),
        ("/help", "Mostrar ayuda de comandos"),
        
        // Legacy
        ("/stats", "Ver estadísticas del índice RAPTOR"),
    ];
    
    // Filter commands based on input
    let filtered: Vec<_> = if data.input_buffer.len() > 1 {
        commands
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(&data.input_buffer))
            .copied()
            .collect()
    } else {
        commands.clone()
    };
    
    if filtered.is_empty() {
        return;
    }
    
    // Scroll window: show max 8 items at a time
    const MAX_VISIBLE: usize = 8;
    let total_items = filtered.len();
    let selected = data.autocomplete_selected;
    
    // Calculate scroll offset to keep selected item visible
    let scroll_offset = if selected < MAX_VISIBLE / 2 {
        0
    } else if selected >= total_items - MAX_VISIBLE / 2 {
        total_items.saturating_sub(MAX_VISIBLE)
    } else {
        selected.saturating_sub(MAX_VISIBLE / 2)
    };
    
    let visible_items = filtered
        .iter()
        .skip(scroll_offset)
        .take(MAX_VISIBLE)
        .enumerate();
    
    // Calculate popup dimensions
    let max_cmd_len = filtered.iter().map(|(cmd, _)| cmd.len()).max().unwrap_or(0);
    let max_desc_len = filtered.iter().map(|(_, desc)| desc.len()).max().unwrap_or(0);
    let width = (max_cmd_len + max_desc_len + 6).min(70) as u16;
    let height = (filtered.len().min(MAX_VISIBLE) + 2) as u16;
    
    // Position popup above input area
    let popup_area = Rect {
        x: input_area.x + 2,
        y: input_area.y.saturating_sub(height + 1),
        width,
        height,
    };
    
    // Build content with scroll indicators
    let mut items: Vec<Line> = visible_items
        .map(|(visible_idx, (cmd, desc))| {
            let actual_idx = scroll_offset + visible_idx;
            let style = if actual_idx == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                data.theme.base_style()
            };
            
            Line::from(vec![
                Span::styled(format!(" {:<16}", cmd), style.fg(Color::Cyan)),
                Span::styled(format!(" {}", desc), style.fg(Color::Gray)),
            ])
        })
        .collect();
    
    // Add scroll indicators at top/bottom if needed
    if scroll_offset > 0 {
        items.insert(0, Line::from(Span::styled("  ▲ más arriba ▲", Style::default().fg(Color::DarkGray))));
    }
    if scroll_offset + MAX_VISIBLE < total_items {
        items.push(Line::from(Span::styled("  ▼ más abajo ▼", Style::default().fg(Color::DarkGray))));
    }
    
    let title = format!(" Comandos ({}/{}) ", selected + 1, total_items);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(data.theme.primary_style())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(title, data.theme.primary_style()))
        .style(data.theme.base_style());
    
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(items)
            .block(block)
            .wrap(Wrap { trim: false }),
        popup_area,
    );
}
