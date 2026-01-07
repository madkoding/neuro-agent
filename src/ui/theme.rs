//! Modern theme system for the TUI

use ratatui::style::{Color, Modifier, Style};

/// Color palette for the application
#[derive(Debug, Clone)]
pub struct Theme {
    // Base colors
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    
    // Accent colors
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    
    // Semantic colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    
    // UI element colors
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,
    pub highlight: Color,
    
    // Message colors
    pub user_message: Color,
    pub assistant_message: Color,
    pub system_message: Color,
    pub tool_message: Color,
}

impl Theme {
    /// Modern dark theme (default)
    pub fn dark() -> Self {
        Self {
            // Base - dark background with light text
            background: Color::Rgb(22, 22, 30),
            foreground: Color::Rgb(230, 230, 240),
            muted: Color::Rgb(120, 120, 140),
            
            // Accents - vibrant colors
            primary: Color::Rgb(130, 170, 255),      // Soft blue
            secondary: Color::Rgb(180, 130, 255),    // Purple
            accent: Color::Rgb(255, 180, 100),       // Orange
            
            // Semantic
            success: Color::Rgb(130, 255, 170),      // Green
            warning: Color::Rgb(255, 220, 100),      // Yellow
            error: Color::Rgb(255, 130, 130),        // Red
            info: Color::Rgb(100, 200, 255),         // Cyan
            
            // UI elements
            border: Color::Rgb(60, 60, 80),
            border_focused: Color::Rgb(130, 170, 255),
            selection: Color::Rgb(50, 60, 90),
            highlight: Color::Rgb(70, 80, 110),
            
            // Messages
            user_message: Color::Rgb(180, 220, 255),
            assistant_message: Color::Rgb(220, 220, 240),
            system_message: Color::Rgb(150, 150, 170),
            tool_message: Color::Rgb(180, 255, 200),
        }
    }

    /// Light theme variant
    pub fn light() -> Self {
        Self {
            background: Color::Rgb(250, 250, 252),
            foreground: Color::Rgb(30, 30, 40),
            muted: Color::Rgb(130, 130, 150),
            
            primary: Color::Rgb(60, 100, 200),
            secondary: Color::Rgb(130, 80, 200),
            accent: Color::Rgb(220, 130, 50),
            
            success: Color::Rgb(50, 180, 100),
            warning: Color::Rgb(200, 160, 50),
            error: Color::Rgb(220, 80, 80),
            info: Color::Rgb(50, 150, 220),
            
            border: Color::Rgb(200, 200, 210),
            border_focused: Color::Rgb(60, 100, 200),
            selection: Color::Rgb(220, 230, 250),
            highlight: Color::Rgb(235, 240, 250),
            
            user_message: Color::Rgb(50, 80, 150),
            assistant_message: Color::Rgb(40, 40, 60),
            system_message: Color::Rgb(100, 100, 120),
            tool_message: Color::Rgb(50, 130, 80),
        }
    }

    /// High contrast theme for accessibility
    pub fn high_contrast() -> Self {
        Self {
            background: Color::Black,
            foreground: Color::White,
            muted: Color::Gray,
            
            primary: Color::Cyan,
            secondary: Color::Magenta,
            accent: Color::Yellow,
            
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Cyan,
            
            border: Color::White,
            border_focused: Color::Cyan,
            selection: Color::DarkGray,
            highlight: Color::DarkGray,
            
            user_message: Color::Cyan,
            assistant_message: Color::White,
            system_message: Color::Gray,
            tool_message: Color::Green,
        }
    }

    // Style builders
    
    pub fn base_style(&self) -> Style {
        Style::default()
            .bg(self.background)
            .fg(self.foreground)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn primary_style(&self) -> Style {
        Style::default().fg(self.primary)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn info_style(&self) -> Style {
        Style::default().fg(self.info)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        Style::default().fg(if focused { self.border_focused } else { self.border })
    }

    pub fn selection_style(&self) -> Style {
        Style::default().bg(self.selection)
    }

    pub fn highlight_style(&self) -> Style {
        Style::default().bg(self.highlight)
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_style(&self) -> Style {
        Style::default()
            .fg(self.foreground)
            .add_modifier(Modifier::BOLD)
    }

    pub fn user_style(&self) -> Style {
        Style::default().fg(self.user_message)
    }

    pub fn assistant_style(&self) -> Style {
        Style::default().fg(self.assistant_message)
    }

    pub fn system_style(&self) -> Style {
        Style::default()
            .fg(self.system_message)
            .add_modifier(Modifier::ITALIC)
    }

    pub fn tool_style(&self) -> Style {
        Style::default().fg(self.tool_message)
    }

    pub fn code_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn link_style(&self) -> Style {
        Style::default()
            .fg(self.info)
            .add_modifier(Modifier::UNDERLINED)
    }

    pub fn shortcut_key_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn shortcut_desc_style(&self) -> Style {
        Style::default().fg(self.muted)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Icon set for the UI
pub struct Icons;

impl Icons {
    // Status icons
    pub const READY: &'static str = "●";
    pub const WORKING: &'static str = "◐";
    pub const SUCCESS: &'static str = "✓";
    pub const ERROR: &'static str = "✗";
    pub const WARNING: &'static str = "⚠";
    pub const INFO: &'static str = "ℹ";
    
    // Navigation
    pub const ARROW_RIGHT: &'static str = "→";
    pub const ARROW_LEFT: &'static str = "←";
    pub const ARROW_UP: &'static str = "↑";
    pub const ARROW_DOWN: &'static str = "↓";
    pub const CHEVRON_RIGHT: &'static str = "›";
    pub const CHEVRON_DOWN: &'static str = "⌄";
    
    // Actions
    pub const SEND: &'static str = "⏎";
    pub const CANCEL: &'static str = "⎋";
    pub const SETTINGS: &'static str = "⚙";
    pub const SEARCH: &'static str = "🔍";
    pub const REFRESH: &'static str = "⟳";
    
    // Tools
    pub const FILE: &'static str = "📄";
    pub const FOLDER: &'static str = "📁";
    pub const TERMINAL: &'static str = "⌨";
    pub const CODE: &'static str = "💻";
    pub const TOOL: &'static str = "🔧";
    
    // Messages
    pub const USER: &'static str = "👤";
    pub const ASSISTANT: &'static str = "🤖";
    pub const SYSTEM: &'static str = "⚡";
    pub const THINKING: &'static str = "🤔";
    
    // Misc
    pub const LOCK: &'static str = "🔒";
    pub const UNLOCK: &'static str = "🔓";
    pub const CHECK: &'static str = "☑";
    pub const UNCHECK: &'static str = "☐";
    pub const STAR: &'static str = "★";
    pub const CIRCLE: &'static str = "○";
    pub const FILLED_CIRCLE: &'static str = "●";
}

/// Box drawing characters for custom borders
pub struct BoxChars;

impl BoxChars {
    // Single line
    pub const HORIZONTAL: &'static str = "─";
    pub const VERTICAL: &'static str = "│";
    pub const TOP_LEFT: &'static str = "┌";
    pub const TOP_RIGHT: &'static str = "┐";
    pub const BOTTOM_LEFT: &'static str = "└";
    pub const BOTTOM_RIGHT: &'static str = "┘";
    pub const T_DOWN: &'static str = "┬";
    pub const T_UP: &'static str = "┴";
    pub const T_RIGHT: &'static str = "├";
    pub const T_LEFT: &'static str = "┤";
    pub const CROSS: &'static str = "┼";
    
    // Double line
    pub const DOUBLE_HORIZONTAL: &'static str = "═";
    pub const DOUBLE_VERTICAL: &'static str = "║";
    pub const DOUBLE_TOP_LEFT: &'static str = "╔";
    pub const DOUBLE_TOP_RIGHT: &'static str = "╗";
    pub const DOUBLE_BOTTOM_LEFT: &'static str = "╚";
    pub const DOUBLE_BOTTOM_RIGHT: &'static str = "╝";
    
    // Rounded
    pub const ROUNDED_TOP_LEFT: &'static str = "╭";
    pub const ROUNDED_TOP_RIGHT: &'static str = "╮";
    pub const ROUNDED_BOTTOM_LEFT: &'static str = "╰";
    pub const ROUNDED_BOTTOM_RIGHT: &'static str = "╯";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_styles() {
        let theme = Theme::dark();
        let style = theme.base_style();
        assert!(style.bg.is_some());
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_all_themes() {
        let _ = Theme::dark();
        let _ = Theme::light();
        let _ = Theme::high_contrast();
    }
}
