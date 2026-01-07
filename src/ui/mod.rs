//! UI module - Modern TUI interface using ratatui

pub mod animations;
pub mod layout;
pub mod modern_app;
pub mod settings;
pub mod theme;
mod widgets;

pub use animations::{Spinner, StatusIndicator, StatusState};
pub use modern_app::ModernApp;
pub use settings::SettingsPanel;
pub use theme::Theme;
