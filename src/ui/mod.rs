//! UI module - Modern TUI interface using ratatui

mod widgets;
pub mod animations;
pub mod theme;
pub mod settings;
pub mod layout;
pub mod modern_app;

pub use modern_app::ModernApp;
pub use theme::Theme;
pub use settings::SettingsPanel;
pub use animations::{Spinner, StatusIndicator, StatusState};
