//! Reusable UI widgets for the TUI
//!
//! Provides common interactive widgets with keyboard navigation and theming support.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget as RatatuiWidget},
};

/// Common widget trait for interactive components
pub trait Widget {
    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyEvent) -> bool;
    
    /// Render the widget
    fn render(&self, area: Rect, buf: &mut Buffer);
    
    /// Check if widget has focus
    fn is_focused(&self) -> bool;
    
    /// Set focus state
    fn set_focus(&mut self, focused: bool);
}

/// Text input widget with cursor
pub struct TextInput {
    /// Current value
    value: String,
    /// Cursor position
    cursor: usize,
    /// Whether the widget is focused
    focused: bool,
    /// Label for the input
    label: String,
    /// Placeholder text
    placeholder: String,
    /// Input width
    width: u16,
}

impl TextInput {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            focused: false,
            label: label.into(),
            placeholder: String::new(),
            width: 40,
        }
    }
    
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self.cursor = self.value.len();
        self
    }
    
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
    
    pub fn with_width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }
    
    pub fn value(&self) -> &str {
        &self.value
    }
    
    pub fn set_value(&mut self, value: String) {
        self.value = value;
        self.cursor = self.value.len();
    }
    
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }
}

impl Widget for TextInput {
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) => {
                self.value.insert(self.cursor, c);
                self.cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.value.remove(self.cursor);
                }
                true
            }
            KeyCode::Delete => {
                if self.cursor < self.value.len() {
                    self.value.remove(self.cursor);
                }
                true
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                true
            }
            KeyCode::Right => {
                if self.cursor < self.value.len() {
                    self.cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.value.len();
                true
            }
            _ => false,
        }
    }
    
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(self.label.as_str());
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        // Display text with cursor
        let display_text = if self.value.is_empty() && !self.focused {
            Span::styled(&self.placeholder, Style::default().fg(Color::DarkGray))
        } else {
            let before_cursor = &self.value[..self.cursor];
            let cursor_char = self.value.chars().nth(self.cursor).unwrap_or(' ');
            let after_cursor = &self.value[self.cursor + cursor_char.len_utf8()..];
            
            if self.focused {
                Span::raw(format!("{}▋{}", before_cursor, after_cursor))
            } else {
                Span::raw(&self.value)
            }
        };
        
        let paragraph = Paragraph::new(Line::from(display_text));
        paragraph.render(inner, buf);
    }
    
    fn is_focused(&self) -> bool {
        self.focused
    }
    
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }
}

/// Select widget (dropdown)
pub struct Select {
    /// Available options
    options: Vec<String>,
    /// Currently selected index
    selected: usize,
    /// Whether the widget is focused
    focused: bool,
    /// Label for the select
    label: String,
    /// Whether dropdown is open
    open: bool,
}

impl Select {
    pub fn new(label: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            options,
            selected: 0,
            focused: false,
            label: label.into(),
            open: false,
        }
    }
    
    pub fn selected(&self) -> usize {
        self.selected
    }
    
    pub fn selected_value(&self) -> Option<&str> {
        self.options.get(self.selected).map(|s| s.as_str())
    }
    
    pub fn set_selected(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = index;
        }
    }
    
    pub fn set_selected_by_value(&mut self, value: &str) {
        if let Some(idx) = self.options.iter().position(|o| o == value) {
            self.selected = idx;
        }
    }
}

impl Widget for Select {
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.open = !self.open;
                true
            }
            KeyCode::Up => {
                if self.open && self.selected > 0 {
                    self.selected -= 1;
                }
                true
            }
            KeyCode::Down => {
                if self.open && self.selected < self.options.len() - 1 {
                    self.selected += 1;
                }
                true
            }
            KeyCode::Esc => {
                if self.open {
                    self.open = false;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(self.label.as_str());
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        // Display selected value or all options if open
        if self.open && self.focused {
            let lines: Vec<Line> = self.options
                .iter()
                .enumerate()
                .map(|(i, opt)| {
                    if i == self.selected {
                        Line::from(Span::styled(
                            format!("▶ {}", opt),
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                        ))
                    } else {
                        Line::from(Span::raw(format!("  {}", opt)))
                    }
                })
                .collect();
            
            let paragraph = Paragraph::new(lines);
            paragraph.render(inner, buf);
        } else {
            let selected_text = self.options.get(self.selected)
                .map(|s| s.as_str())
                .unwrap_or("(empty)");
            
            let display = Span::raw(format!("{} ▼", selected_text));
            let paragraph = Paragraph::new(Line::from(display));
            paragraph.render(inner, buf);
        }
    }
    
    fn is_focused(&self) -> bool {
        self.focused
    }
    
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.open = false;
        }
    }
}

/// Slider widget for numeric values
pub struct Slider {
    /// Current value
    value: f32,
    /// Minimum value
    min: f32,
    /// Maximum value
    max: f32,
    /// Step size
    step: f32,
    /// Whether the widget is focused
    focused: bool,
    /// Label for the slider
    label: String,
}

impl Slider {
    pub fn new(label: impl Into<String>, min: f32, max: f32, step: f32) -> Self {
        Self {
            value: min,
            min,
            max,
            step,
            focused: false,
            label: label.into(),
        }
    }
    
    pub fn with_value(mut self, value: f32) -> Self {
        self.value = value.clamp(self.min, self.max);
        self
    }
    
    pub fn value(&self) -> f32 {
        self.value
    }
    
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(self.min, self.max);
    }
}

impl Widget for Slider {
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Left => {
                self.value = (self.value - self.step).max(self.min);
                true
            }
            KeyCode::Right => {
                self.value = (self.value + self.step).min(self.max);
                true
            }
            KeyCode::Home => {
                self.value = self.min;
                true
            }
            KeyCode::End => {
                self.value = self.max;
                true
            }
            _ => false,
        }
    }
    
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!("{}: {:.2}", self.label, self.value));
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        // Draw slider bar
        let width = inner.width.saturating_sub(2) as f32;
        let position = ((self.value - self.min) / (self.max - self.min) * width) as u16;
        
        let bar_text = format!(
            "{}▮{}",
            "─".repeat(position as usize),
            "─".repeat((width as usize).saturating_sub(position as usize))
        );
        
        let slider_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        
        let paragraph = Paragraph::new(Line::from(Span::styled(bar_text, slider_style)));
        paragraph.render(inner, buf);
    }
    
    fn is_focused(&self) -> bool {
        self.focused
    }
    
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }
}

/// Button widget
pub struct Button {
    /// Button label
    label: String,
    /// Whether the widget is focused
    focused: bool,
    /// Whether button is pressed
    pressed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            focused: false,
            pressed: false,
        }
    }
    
    pub fn is_pressed(&self) -> bool {
        self.pressed
    }
    
    pub fn reset(&mut self) {
        self.pressed = false;
    }
}

impl Widget for Button {
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.pressed = true;
                true
            }
            _ => false,
        }
    }
    
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let style = if self.pressed {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if self.focused {
            Style::default().fg(Color::Cyan).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .style(style);
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        let text = Span::styled(format!(" {} ", self.label), style);
        let paragraph = Paragraph::new(Line::from(text));
        paragraph.render(inner, buf);
    }
    
    fn is_focused(&self) -> bool {
        self.focused
    }
    
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }
}
