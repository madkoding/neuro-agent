//! Modern TUI Application with async processing

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
    OrchestratorResponse, PlanningOrchestrator, PlanningResponse, TaskProgressInfo,
    TaskProgressStatus,
};
use crate::i18n::{current_locale, init_locale, t, Locale, Text};
use crate::tools::TaskPlan;

use super::animations::{Spinner, StatusIndicator, StatusState};
use super::layout::centered_rect;
use super::settings::{SettingsPanel, ToolConfig};
use super::theme::{Icons, Theme};
// Plan widgets available but not used in modern_app directly
// use super::widgets::{PlanViewer, PlanSummary};

/// Application mode/screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Chat,
    Settings,
    Confirmation,
    Password,
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

/// Message from background task to UI
#[derive(Debug)]
#[allow(dead_code)]
enum BackgroundMessage {
    Response(Result<OrchestratorResponse, String>),
    PlanningResponse(Result<PlanningResponse, String>),
    Thinking(String),
    /// Progress update for a task in a plan
    TaskProgress(TaskProgressInfo),
    /// RAPTOR indexing status update
    RaptorStatus(String),
    /// RAPTOR indexing complete
    RaptorComplete,
}

/// Main application state
pub struct ModernApp {
    // Core
    terminal: Terminal<CrosstermBackend<Stdout>>,
    orchestrator: Arc<Mutex<PlanningOrchestrator>>,
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
    current_thinking: Option<String>,

    // Background task communication
    response_rx: Option<mpsc::Receiver<BackgroundMessage>>,

    // Settings
    settings_panel: SettingsPanel,

    // Confirmation
    pending_command: Option<String>,
    password_input: String,
    password_error: Option<String>,

    // Background RAPTOR indexing
    raptor_indexing: bool,
    raptor_status: Option<String>,
    raptor_rx: Option<mpsc::Receiver<BackgroundMessage>>,

    // Input mode
    input_mode: InputMode,

    // Ctrl+C counter for exit
    ctrl_c_count: u8,
    last_ctrl_c: Option<Instant>,

    // Tick counter for animations
    tick_counter: u64,
}

impl ModernApp {
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

            messages: vec![DisplayMessage {
                sender: MessageSender::System,
                content: format!("{} ({})", t(Text::Ready), locale.display_name()),
                timestamp: Instant::now(),
                is_streaming: false,
                tool_name: None,
            }],
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
            current_thinking: None,

            response_rx: None,

            settings_panel: SettingsPanel::new(),

            pending_command: None,
            password_input: String::new(),
            password_error: None,

            raptor_indexing: false,
            raptor_status: None,
            raptor_rx: None,

            input_mode: InputMode::Question,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            tick_counter: 0,
        })
    }

    /// Start background RAPTOR indexing (two phases: quick index + full RAPTOR)
    fn start_background_raptor_indexing(&mut self) {
        if self.raptor_indexing {
            return; // Already indexing
        }

        self.raptor_indexing = true;
        self.raptor_status = Some("Leyendo archivos...".to_string());

        let orchestrator = self.orchestrator.clone();
        let (tx, rx) = mpsc::channel::<BackgroundMessage>(50);
        self.raptor_rx = Some(rx);

        // Spawn background task with two phases
        tokio::spawn(async move {
            use crate::raptor::builder::{has_full_index, quick_index_sync};

            // Phase 1: Quick index (very fast - just read files) - run in blocking thread
            let _ = tx
                .send(BackgroundMessage::RaptorStatus(
                    "📖 Leyendo archivos...".to_string(),
                ))
                .await;

            let project_path = std::env::current_dir().unwrap_or_default();
            let path_clone = project_path.clone();

            let quick_result =
                tokio::task::spawn_blocking(move || quick_index_sync(&path_clone, 1500, 200)).await;

            match quick_result {
                Ok(Ok(chunks)) => {
                    let _ = tx
                        .send(BackgroundMessage::RaptorStatus(format!(
                            "📄 {} chunks listos",
                            chunks
                        )))
                        .await;
                }
                _ => {
                    let _ = tx
                        .send(BackgroundMessage::RaptorStatus(
                            "⚠ Error en lectura".to_string(),
                        ))
                        .await;
                }
            }

            // Phase 2: Full RAPTOR index (embeddings, clustering, summarization)
            let is_full = tokio::task::spawn_blocking(has_full_index)
                .await
                .unwrap_or(false);

            if !is_full {
                let _ = tx
                    .send(BackgroundMessage::RaptorStatus(
                        "🔬 Indexando RAPTOR...".to_string(),
                    ))
                    .await;

                let mut orch = orchestrator.lock().await;
                match orch.initialize_raptor().await {
                    Ok(true) => {
                        let _ = tx
                            .send(BackgroundMessage::RaptorStatus(
                                "✓ RAPTOR listo".to_string(),
                            ))
                            .await;
                    }
                    Ok(false) => {
                        let _ = tx
                            .send(BackgroundMessage::RaptorStatus("📄 Solo texto".to_string()))
                            .await;
                    }
                    Err(_) => {
                        let _ = tx
                            .send(BackgroundMessage::RaptorStatus(
                                "⚠ Error RAPTOR".to_string(),
                            ))
                            .await;
                    }
                }
            } else {
                let _ = tx
                    .send(BackgroundMessage::RaptorStatus(
                        "✓ RAPTOR listo".to_string(),
                    ))
                    .await;
            }

            let _ = tx.send(BackgroundMessage::RaptorComplete).await;
        });
    }

    /// Check for RAPTOR indexing updates
    fn check_raptor_status(&mut self) {
        if let Some(ref mut rx) = self.raptor_rx {
            loop {
                match rx.try_recv() {
                    Ok(BackgroundMessage::RaptorStatus(status)) => {
                        self.raptor_status = Some(status);
                    }
                    Ok(BackgroundMessage::RaptorComplete) => {
                        self.raptor_indexing = false;
                        self.raptor_status = Some("Índice listo ✓".to_string());
                        // Clear status after a moment (will be done on next message)
                    }
                    Ok(_) => {} // Ignore other messages
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.raptor_indexing = false;
                        self.raptor_rx = None;
                        break;
                    }
                }
            }
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        // Start RAPTOR indexing in background immediately
        self.start_background_raptor_indexing();

        let tick_rate = Duration::from_millis(80); // Faster tick for smoother animations
        let mut last_tick = Instant::now();

        loop {
            // Draw UI first
            self.draw()?;

            // Check for background task completion
            self.check_background_response().await;

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
        // Colectar mensajes primero para evitar problemas de borrow
        let mut messages_to_add: Vec<(MessageSender, String, Option<String>)> = Vec::new();
        let mut final_response: Option<Result<PlanningResponse, String>> = None;
        let mut orch_response: Option<Result<OrchestratorResponse, String>> = None;
        let mut should_close = false;
        let mut new_thinking: Option<String> = None;
        let mut new_status_message: Option<String> = None;

        if let Some(ref mut rx) = self.response_rx {
            // Non-blocking check for response - puede haber múltiples mensajes
            loop {
                match rx.try_recv() {
                    Ok(BackgroundMessage::Response(result)) => {
                        orch_response = Some(result);
                        should_close = true;
                        break;
                    }
                    Ok(BackgroundMessage::PlanningResponse(result)) => {
                        final_response = Some(result);
                        should_close = true;
                        break;
                    }
                    Ok(BackgroundMessage::Thinking(thought)) => {
                        new_thinking = Some(thought);
                    }
                    Ok(BackgroundMessage::TaskProgress(progress)) => {
                        // Mostrar progreso de la tarea en tiempo real (menos verbose)
                        let TaskProgressInfo {
                            task_index,
                            total_tasks,
                            description,
                            status,
                        } = progress;
                        let msg = match status {
                            TaskProgressStatus::Started => {
                                new_status_message = Some(format!(
                                    "Tarea {}/{}: {}",
                                    task_index + 1,
                                    total_tasks,
                                    description
                                ));
                                // Solo actualizar status bar, no añadir mensaje
                                continue;
                            }
                            TaskProgressStatus::Completed(_) => {
                                // Solo mostrar descripción sin el contenido
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
                    Ok(BackgroundMessage::RaptorStatus(_))
                    | Ok(BackgroundMessage::RaptorComplete) => {
                        // Handled by check_raptor_status, ignore here
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // No more messages for now
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // Task completed or failed
                        should_close = true;
                        self.status.set_state(StatusState::Error);
                        self.status_message = t(Text::Error).to_string();
                        break;
                    }
                }
            }
        }

        // Aplicar cambios fuera del borrow de rx
        for (sender, content, tool) in messages_to_add {
            self.add_message(sender, content, tool);
        }

        if let Some(thinking) = new_thinking {
            self.current_thinking = Some(thinking);
        }

        if let Some(status) = new_status_message {
            self.status_message = status;
        }

        if let Some(result) = orch_response {
            self.handle_orchestrator_response(result);
            self.is_processing = false;
            self.processing_start = None;
            self.current_thinking = None;
            self.status_message = t(Text::Ready).to_string();
            self.status.set_state(StatusState::Idle);
            self.response_rx = None;
        } else if let Some(result) = final_response {
            self.handle_planning_response(result);
            self.is_processing = false;
            self.processing_start = None;
            self.current_thinking = None;
            self.status_message = t(Text::Ready).to_string();
            self.status.set_state(StatusState::Idle);
            self.response_rx = None;
        } else if should_close {
            self.is_processing = false;
            self.processing_start = None;
            self.response_rx = None;
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
                        // Handle streaming
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
        // Clone all data needed for rendering
        let render_data = RenderData {
            theme: self.theme.clone(),
            screen: self.screen,
            status_render: self.status.render(),
            status_message: self.status_message.clone(),
            messages: self.messages.clone(),
            input_buffer: self.input_buffer.clone(),
            cursor_position: self.cursor_position,
            scroll_offset: self.scroll_offset,
            is_processing: self.is_processing,
            processing_start: self.processing_start,
            spinner_frame: self.spinner.frame().to_string(),
            current_thinking: self.current_thinking.clone(),
            settings_tools: self.settings_panel.tools.clone(),
            settings_selected: self.settings_panel.selected_index,
            pending_command: self.pending_command.clone(),
            password_input_len: self.password_input.len(),
            password_error: self.password_error.clone(),
            enabled_tools_count: self.settings_panel.get_enabled_tools().len(),
            show_plan_panel: self.show_plan_panel,
            active_plan: self.active_plan.clone(),
            raptor_indexing: self.raptor_indexing,
            raptor_status: self.raptor_status.clone(),
            input_mode: self.input_mode,
            tick_counter: self.tick_counter,
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
                self.start_processing().await;
            }
            KeyCode::Char(c) if !self.is_processing => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
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
                // Scroll up - siempre disponible
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
                self.auto_scroll = false;
            }
            KeyCode::Down => {
                // Scroll down - siempre disponible
                self.scroll_offset = self.scroll_offset.saturating_add(3);
                self.auto_scroll = false;
            }
            KeyCode::PageUp => {
                // Scroll up by page
                self.scroll_offset = self.scroll_offset.saturating_sub(15);
                self.auto_scroll = false;
            }
            KeyCode::PageDown => {
                // Scroll down by page
                self.scroll_offset = self.scroll_offset.saturating_add(15);
                self.auto_scroll = false;
            }
            KeyCode::Home if self.is_processing || self.input_buffer.is_empty() => {
                // Ir al inicio del chat
                self.scroll_offset = 0;
                self.auto_scroll = false;
            }
            KeyCode::End if self.is_processing || self.input_buffer.is_empty() => {
                // Ir al final del chat - reactivar auto-scroll
                self.scroll_offset = self.messages.len() * 10; // Será clampeado en render
                self.auto_scroll = true;
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
        self.status.set_state(StatusState::Working);
        self.status_message = t(Text::Processing).to_string();
        self.spinner = Spinner::thinking(); // Reset spinner
        self.auto_scroll = true; // Reactivar auto-scroll al empezar a procesar

        // Get enabled tools
        let _enabled_tools = self.settings_panel.get_enabled_tool_ids();

        // Create channel for background communication
        let (tx, rx) = mpsc::channel(100);
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
                    .send(BackgroundMessage::TaskProgress(progress))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Spawn background task
        tokio::spawn(async move {
            let mut orch = orchestrator.lock().await;
            let result = orch
                .process_with_planning_and_progress(&user_input, Some(progress_tx))
                .await;
            let msg = match result {
                Ok(response) => BackgroundMessage::PlanningResponse(Ok(response)),
                Err(e) => BackgroundMessage::PlanningResponse(Err(e.to_string())),
            };
            let _ = tx.send(msg).await;
        });
    }

    fn handle_settings_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Esc => {
                // Esc/Tab vuelve al chat, no sale de la app
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
            _ => {}
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

    fn cancel_processing(&mut self) {
        self.is_processing = false;
        self.processing_start = None;
        self.current_thinking = None;
        self.response_rx = None;
        self.status.set_state(StatusState::Warning);
        self.status_message = t(Text::Cancelled).to_string();
        self.add_message(MessageSender::System, t(Text::Cancelled).to_string(), None);
    }

    fn add_message(&mut self, sender: MessageSender, content: String, tool_name: Option<String>) {
        self.messages.push(DisplayMessage {
            sender,
            content,
            timestamp: Instant::now(),
            is_streaming: false,
            tool_name,
        });
        // Auto-scroll: estimar el número de líneas y hacer scroll al final
        // Usamos un valor grande pero razonable que será clampeado en render
        if self.auto_scroll {
            // Estimamos ~3 líneas por mensaje como promedio
            self.scroll_offset = self.messages.len() * 10;
        }
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                // Scroll hacia arriba - 3 líneas por evento
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
                self.auto_scroll = false;
            }
            MouseEventKind::ScrollDown => {
                // Scroll hacia abajo - 3 líneas por evento
                self.scroll_offset = self.scroll_offset.saturating_add(3);
                self.auto_scroll = false;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Click izquierdo - desactiva auto-scroll
                self.auto_scroll = false;
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

struct RenderData {
    theme: Theme,
    screen: AppScreen,
    status_render: (&'static str, (u8, u8, u8)),
    status_message: String,
    messages: Vec<DisplayMessage>,
    input_buffer: String,
    #[allow(dead_code)]
    cursor_position: usize,
    scroll_offset: usize,
    is_processing: bool,
    processing_start: Option<Instant>,
    spinner_frame: String,
    #[allow(dead_code)]
    current_thinking: Option<String>,
    settings_tools: Vec<ToolConfig>,
    settings_selected: usize,
    pending_command: Option<String>,
    password_input_len: usize,
    password_error: Option<String>,
    enabled_tools_count: usize,
    #[allow(dead_code)]
    show_plan_panel: bool,
    #[allow(dead_code)]
    active_plan: Option<TaskPlan>,
    raptor_indexing: bool,
    raptor_status: Option<String>,
    input_mode: InputMode,
    tick_counter: u64,
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

    for i in start..chars.len().saturating_sub(pattern_len - 1) {
        if chars[i..i + pattern_len] == pattern_chars[..] {
            return Some(i);
        }
    }
    None
}

fn find_closing_char(chars: &[char], start: usize, marker: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == marker {
            return Some(i);
        }
    }
    None
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

    for msg in &data.messages {
        let (icon, label, style) = match msg.sender {
            MessageSender::User => (Icons::USER, "Tú", data.theme.user_style()),
            MessageSender::Assistant => {
                (Icons::ASSISTANT, "Asistente", data.theme.assistant_style())
            }
            MessageSender::System => (Icons::SYSTEM, "Sistema", data.theme.system_style()),
            MessageSender::Tool => (Icons::TOOL, "Tarea", data.theme.tool_style()),
        };

        // Header with icon and label
        let header = if let Some(ref tool) = msg.tool_name {
            Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(format!("{}", label), style.add_modifier(Modifier::BOLD)),
                Span::styled(format!(" [{}]", tool), data.theme.code_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(format!("{}", label), style.add_modifier(Modifier::BOLD)),
            ])
        };
        lines.push(header);

        // Parse content with markdown support
        for content_line in msg.content.lines() {
            let spans = parse_markdown_line(content_line, style, data.theme.accent_style());
            let mut line_spans = vec![Span::raw("   ")]; // 3 spaces for alignment with icon
            line_spans.extend(spans);
            lines.push(Line::from(line_spans));
        }

        lines.push(Line::from(""));
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
        let cursor_char = if (data.tick_counter / 2) % 2 == 0 {
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
    for line in &lines {
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
    let max_scroll = if total_lines > visible_lines {
        total_lines - visible_lines
    } else {
        0
    };
    let scroll = data.scroll_offset.min(max_scroll);

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

    // Add padding inside the block - start 1 line down
    let padded_inner = Rect {
        x: inner.x + 1,
        y: inner.y + 1,
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(1),
    };

    // Multi-line input with wrap
    let input_text = if data.is_processing {
        // Show the input that was sent while processing
        if data.input_buffer.is_empty() {
            vec![Line::from(Span::styled(
                "Esperando respuesta...",
                Style::default().fg(Color::Gray),
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

    frame.render_widget(paragraph, padded_inner);

    // Show blinking cursor (always when focused and not processing)
    if is_focused && !data.is_processing {
        // Fast blink based on tick counter (every ~200ms)
        let show_cursor = (data.tick_counter / 2) % 2 == 0;

        if show_cursor {
            let cursor_char = if data.input_buffer.is_empty() {
                "▌" // Block cursor when empty
            } else {
                "▎" // Line cursor when typing
            };

            // Calculate cursor position
            let cursor_y = if data.input_buffer.is_empty() {
                padded_inner.y
            } else {
                padded_inner.y
                    + (data.input_buffer.lines().count().saturating_sub(1) as u16)
                        .min(padded_inner.height.saturating_sub(1))
            };
            let cursor_x = if data.input_buffer.is_empty() {
                padded_inner.x
            } else {
                padded_inner.x
                    + (data.input_buffer.lines().last().unwrap_or("").len() as u16)
                        .min(padded_inner.width.saturating_sub(1))
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
    let shortcuts = Line::from(vec![
        Span::styled(" ↑↓ ", data.theme.shortcut_key_style()),
        Span::styled("Navigate", data.theme.shortcut_desc_style()),
        Span::raw("  "),
        Span::styled(" Space ", data.theme.shortcut_key_style()),
        Span::styled(t(Text::ToggleTool), data.theme.shortcut_desc_style()),
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
    // Show only simple status, detailed progress is in chat area
    let status_text = if data.is_processing {
        format!("{} Procesando", data.spinner_frame)
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

    // RAPTOR status
    let raptor_info = if data.raptor_indexing {
        format!(
            "{} {}",
            data.spinner_frame,
            data.raptor_status.as_deref().unwrap_or("Indexando")
        )
    } else if let Some(ref status) = data.raptor_status {
        if status.contains("✓") || status.contains("listo") {
            "📊✓".to_string()
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

    // Shortcuts hint
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        "^N:modo  ^C×2:salir",
        Style::default().fg(Color::DarkGray),
    ));

    let line = Line::from(spans);

    frame.render_widget(Paragraph::new(line).style(data.theme.base_style()), area);
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
