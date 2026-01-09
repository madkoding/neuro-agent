//! Model configuration panel for the TUI
//!
//! Allows users to configure model providers and settings interactively.

use crate::config::{AppConfig, ModelProvider};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

/// Field being edited in the model config panel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    FastProvider,
    FastUrl,
    FastModel,
    FastApiKey,
    FastTemperature,
    FastTopP,
    HeavyProvider,
    HeavyUrl,
    HeavyModel,
    HeavyApiKey,
    HeavyTemperature,
    HeavyTopP,
    SaveButton,
    TestConnectionButton,
}

impl ConfigField {
    fn all_fields() -> Vec<Self> {
        vec![
            Self::FastProvider,
            Self::FastUrl,
            Self::FastModel,
            Self::FastApiKey,
            Self::FastTemperature,
            Self::FastTopP,
            Self::HeavyProvider,
            Self::HeavyUrl,
            Self::HeavyModel,
            Self::HeavyApiKey,
            Self::HeavyTemperature,
            Self::HeavyTopP,
            Self::SaveButton,
            Self::TestConnectionButton,
        ]
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::FastProvider => "Fast Model Provider",
            Self::FastUrl => "Fast Model URL",
            Self::FastModel => "Fast Model Name",
            Self::FastApiKey => "Fast Model API Key",
            Self::FastTemperature => "Fast Model Temperature",
            Self::FastTopP => "Fast Model Top P",
            Self::HeavyProvider => "Heavy Model Provider",
            Self::HeavyUrl => "Heavy Model URL",
            Self::HeavyModel => "Heavy Model Name",
            Self::HeavyApiKey => "Heavy Model API Key",
            Self::HeavyTemperature => "Heavy Model Temperature",
            Self::HeavyTopP => "Heavy Model Top P",
            Self::SaveButton => "💾 Save Configuration",
            Self::TestConnectionButton => "🔌 Test Connection",
        }
    }

    fn is_button(&self) -> bool {
        matches!(self, Self::SaveButton | Self::TestConnectionButton)
    }
}

/// Model configuration panel
pub struct ModelConfigPanel {
    /// Current configuration
    config: AppConfig,
    /// Currently selected field
    selected_field: usize,
    /// Whether a field is being edited
    pub editing: bool,
    /// Current edit buffer
    edit_buffer: String,
    /// Cursor position in edit buffer
    cursor_position: usize,
    /// Provider selection for fast model (0=Ollama, 1=OpenAI, 2=Anthropic, 3=Groq)
    fast_provider_index: usize,
    /// Provider selection for heavy model
    heavy_provider_index: usize,
    /// Status message
    status_message: Option<String>,
    /// Whether status is an error
    status_is_error: bool,
}

impl ModelConfigPanel {
    pub fn new(config: AppConfig) -> Self {
        let fast_provider_index = match config.fast_model.provider {
            ModelProvider::Ollama => 0,
            ModelProvider::OpenAI => 1,
            ModelProvider::Anthropic => 2,
            ModelProvider::Groq => 3,
        };
        
        let heavy_provider_index = match config.heavy_model.provider {
            ModelProvider::Ollama => 0,
            ModelProvider::OpenAI => 1,
            ModelProvider::Anthropic => 2,
            ModelProvider::Groq => 3,
        };

        Self {
            config,
            selected_field: 0,
            editing: false,
            edit_buffer: String::new(),
            cursor_position: 0,
            fast_provider_index,
            heavy_provider_index,
            status_message: None,
            status_is_error: false,
        }
    }

    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    pub fn move_up(&mut self) {
        if self.editing {
            return;
        }
        if self.selected_field > 0 {
            self.selected_field -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.editing {
            return;
        }
        let max = ConfigField::all_fields().len() - 1;
        if self.selected_field < max {
            self.selected_field += 1;
        }
    }

    pub fn start_editing(&mut self) {
        let field = ConfigField::all_fields()[self.selected_field];
        
        if field.is_button() {
            // Buttons don't enter edit mode
            return;
        }

        self.editing = true;
        self.edit_buffer = self.get_field_value(&field);
        self.cursor_position = self.edit_buffer.len();
    }

    pub fn finish_editing(&mut self) {
        if !self.editing {
            return;
        }

        let field = ConfigField::all_fields()[self.selected_field];
        self.set_field_value(&field, self.edit_buffer.clone());
        self.editing = false;
        self.edit_buffer.clear();
        self.cursor_position = 0;
    }

    pub fn cancel_editing(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
        self.cursor_position = 0;
    }

    pub fn handle_char(&mut self, c: char) {
        if !self.editing {
            return;
        }
        self.edit_buffer.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    pub fn handle_backspace(&mut self) {
        if !self.editing || self.cursor_position == 0 {
            return;
        }
        self.cursor_position -= 1;
        self.edit_buffer.remove(self.cursor_position);
    }

    pub fn handle_delete(&mut self) {
        if !self.editing || self.cursor_position >= self.edit_buffer.len() {
            return;
        }
        self.edit_buffer.remove(self.cursor_position);
    }

    pub fn handle_left(&mut self) {
        if self.editing && self.cursor_position > 0 {
            self.cursor_position -= 1;
        } else if !self.editing {
            // Cycle provider backwards
            self.cycle_provider_left();
        }
    }

    pub fn handle_right(&mut self) {
        if self.editing && self.cursor_position < self.edit_buffer.len() {
            self.cursor_position += 1;
        } else if !self.editing {
            // Cycle provider forwards
            self.cycle_provider_right();
        }
    }

    fn cycle_provider_left(&mut self) {
        let field = ConfigField::all_fields()[self.selected_field];
        match field {
            ConfigField::FastProvider => {
                self.fast_provider_index = if self.fast_provider_index > 0 {
                    self.fast_provider_index - 1
                } else {
                    3
                };
                self.update_fast_provider();
            }
            ConfigField::HeavyProvider => {
                self.heavy_provider_index = if self.heavy_provider_index > 0 {
                    self.heavy_provider_index - 1
                } else {
                    3
                };
                self.update_heavy_provider();
            }
            _ => {}
        }
    }

    fn cycle_provider_right(&mut self) {
        let field = ConfigField::all_fields()[self.selected_field];
        match field {
            ConfigField::FastProvider => {
                self.fast_provider_index = (self.fast_provider_index + 1) % 4;
                self.update_fast_provider();
            }
            ConfigField::HeavyProvider => {
                self.heavy_provider_index = (self.heavy_provider_index + 1) % 4;
                self.update_heavy_provider();
            }
            _ => {}
        }
    }

    fn update_fast_provider(&mut self) {
        self.config.fast_model.provider = match self.fast_provider_index {
            0 => ModelProvider::Ollama,
            1 => ModelProvider::OpenAI,
            2 => ModelProvider::Anthropic,
            3 => ModelProvider::Groq,
            _ => ModelProvider::Ollama,
        };
    }

    fn update_heavy_provider(&mut self) {
        self.config.heavy_model.provider = match self.heavy_provider_index {
            0 => ModelProvider::Ollama,
            1 => ModelProvider::OpenAI,
            2 => ModelProvider::Anthropic,
            3 => ModelProvider::Groq,
            _ => ModelProvider::Ollama,
        };
    }

    fn get_field_value(&self, field: &ConfigField) -> String {
        match field {
            ConfigField::FastProvider => format!("{}", self.config.fast_model.provider),
            ConfigField::FastUrl => self.config.fast_model.url.clone(),
            ConfigField::FastModel => self.config.fast_model.model.clone(),
            ConfigField::FastApiKey => self.config.fast_model.api_key.clone().unwrap_or_default(),
            ConfigField::FastTemperature => format!("{:.2}", self.config.fast_model.temperature),
            ConfigField::FastTopP => format!("{:.2}", self.config.fast_model.top_p),
            ConfigField::HeavyProvider => format!("{}", self.config.heavy_model.provider),
            ConfigField::HeavyUrl => self.config.heavy_model.url.clone(),
            ConfigField::HeavyModel => self.config.heavy_model.model.clone(),
            ConfigField::HeavyApiKey => self.config.heavy_model.api_key.clone().unwrap_or_default(),
            ConfigField::HeavyTemperature => format!("{:.2}", self.config.heavy_model.temperature),
            ConfigField::HeavyTopP => format!("{:.2}", self.config.heavy_model.top_p),
            _ => String::new(),
        }
    }

    fn set_field_value(&mut self, field: &ConfigField, value: String) {
        match field {
            ConfigField::FastUrl => self.config.fast_model.url = value,
            ConfigField::FastModel => self.config.fast_model.model = value,
            ConfigField::FastApiKey => {
                self.config.fast_model.api_key = if value.is_empty() { None } else { Some(value) };
            }
            ConfigField::FastTemperature => {
                if let Ok(temp) = value.parse::<f32>() {
                    self.config.fast_model.temperature = temp.clamp(0.0, 2.0);
                }
            }
            ConfigField::FastTopP => {
                if let Ok(top_p) = value.parse::<f32>() {
                    self.config.fast_model.top_p = top_p.clamp(0.0, 1.0);
                }
            }
            ConfigField::HeavyUrl => self.config.heavy_model.url = value,
            ConfigField::HeavyModel => self.config.heavy_model.model = value,
            ConfigField::HeavyApiKey => {
                self.config.heavy_model.api_key = if value.is_empty() { None } else { Some(value) };
            }
            ConfigField::HeavyTemperature => {
                if let Ok(temp) = value.parse::<f32>() {
                    self.config.heavy_model.temperature = temp.clamp(0.0, 2.0);
                }
            }
            ConfigField::HeavyTopP => {
                if let Ok(top_p) = value.parse::<f32>() {
                    self.config.heavy_model.top_p = top_p.clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }

    pub fn activate_button(&mut self) -> Option<ButtonAction> {
        let field = ConfigField::all_fields()[self.selected_field];
        match field {
            ConfigField::SaveButton => Some(ButtonAction::Save),
            ConfigField::TestConnectionButton => Some(ButtonAction::TestConnection),
            _ => None,
        }
    }

    pub fn set_status(&mut self, message: String, is_error: bool) {
        self.status_message = Some(message);
        self.status_is_error = is_error;
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Fields
                Constraint::Length(3),  // Status
                Constraint::Length(2),  // Footer
            ])
            .split(area);

        // Title
        let title = Paragraph::new("⚙️  Model Configuration")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        title.render(chunks[0], buf);

        // Fields
        let fields = ConfigField::all_fields();
        let items: Vec<ListItem> = fields
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let is_selected = i == self.selected_field;
                let is_editing = is_selected && self.editing;

                let value = if is_editing {
                    // Show edit buffer with cursor
                    let before = &self.edit_buffer[..self.cursor_position];
                    let after = &self.edit_buffer[self.cursor_position..];
                    format!("{}▋{}", before, after)
                } else {
                    self.get_field_value(field)
                };

                let style = if field.is_button() {
                    if is_selected {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan)
                    }
                } else if is_editing {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };

                let prefix = if is_selected { "▶ " } else { "  " };
                
                let line = if field.is_button() {
                    Line::from(Span::styled(format!("{}{}", prefix, field.display_name()), style))
                } else {
                    Line::from(vec![
                        Span::styled(format!("{}{}: ", prefix, field.display_name()), style),
                        Span::styled(value, if is_editing {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::White)
                        }),
                    ])
                };

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Settings"));
        list.render(chunks[1], buf);

        // Status
        if let Some(ref msg) = self.status_message {
            let style = if self.status_is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            let status = Paragraph::new(msg.as_str())
                .style(style)
                .alignment(ratatui::layout::Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            status.render(chunks[2], buf);
        }

        // Footer
        let footer_text = if self.editing {
            "Enter: Save | Esc: Cancel | ←→: Move cursor"
        } else {
            "↑↓: Navigate | Enter: Edit/Activate | Tab: Back | ←→: Change provider"
        };
        let footer = Paragraph::new(footer_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        footer.render(chunks[3], buf);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    Save,
    TestConnection,
}
