//! Settings panel for tool configuration

use crate::i18n::{t, Text};
use super::theme::Icons;
use super::animations::StatusIndicator;

/// Tool configuration for settings panel
#[derive(Debug, Clone)]
pub struct ToolConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub icon: &'static str,
    pub category: ToolCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    FileSystem,
    Execution,
    Analysis,
    Git,
    Project,
    Utilities,
}

impl ToolCategory {
    pub fn name(&self) -> &'static str {
        match self {
            ToolCategory::FileSystem => "📁 File System",
            ToolCategory::Execution => "⌨ Execution",
            ToolCategory::Analysis => "🔍 Analysis",
            ToolCategory::Git => "🔀 Git",
            ToolCategory::Project => "📦 Project",
            ToolCategory::Utilities => "🛠 Utilities",
        }
    }
}

/// Settings panel state
#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub tools: Vec<ToolConfig>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visible_items: usize,
    pub animation: StatusIndicator,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            tools: Self::default_tools(),
            selected_index: 0,
            scroll_offset: 0,
            visible_items: 10,
            animation: StatusIndicator::new(),
        }
    }

    fn default_tools() -> Vec<ToolConfig> {
        vec![
            // File System
            ToolConfig {
                id: "file_read".to_string(),
                name: t(Text::ToolFileRead).to_string(),
                description: t(Text::ToolFileReadDesc).to_string(),
                enabled: true,
                icon: Icons::FILE,
                category: ToolCategory::FileSystem,
            },
            ToolConfig {
                id: "file_write".to_string(),
                name: t(Text::ToolFileWrite).to_string(),
                description: t(Text::ToolFileWriteDesc).to_string(),
                enabled: true,
                icon: Icons::FILE,
                category: ToolCategory::FileSystem,
            },
            ToolConfig {
                id: "list_dir".to_string(),
                name: t(Text::ToolListDir).to_string(),
                description: t(Text::ToolListDirDesc).to_string(),
                enabled: true,
                icon: Icons::FOLDER,
                category: ToolCategory::FileSystem,
            },
            ToolConfig {
                id: "indexer".to_string(),
                name: t(Text::ToolIndexer).to_string(),
                description: t(Text::ToolIndexerDesc).to_string(),
                enabled: true,
                icon: "📇",
                category: ToolCategory::FileSystem,
            },
            ToolConfig {
                id: "search".to_string(),
                name: t(Text::ToolSearch).to_string(),
                description: t(Text::ToolSearchDesc).to_string(),
                enabled: true,
                icon: "🔎",
                category: ToolCategory::FileSystem,
            },
            // Execution
            ToolConfig {
                id: "shell_exec".to_string(),
                name: t(Text::ToolShellExec).to_string(),
                description: t(Text::ToolShellExecDesc).to_string(),
                enabled: true,
                icon: Icons::TERMINAL,
                category: ToolCategory::Execution,
            },
            ToolConfig {
                id: "shell_advanced".to_string(),
                name: t(Text::ToolShellAdvanced).to_string(),
                description: t(Text::ToolShellAdvancedDesc).to_string(),
                enabled: true,
                icon: "💻",
                category: ToolCategory::Execution,
            },
            ToolConfig {
                id: "test_runner".to_string(),
                name: t(Text::ToolTestRunner).to_string(),
                description: t(Text::ToolTestRunnerDesc).to_string(),
                enabled: true,
                icon: "🧪",
                category: ToolCategory::Execution,
            },
            // Analysis
            ToolConfig {
                id: "linter".to_string(),
                name: t(Text::ToolLinter).to_string(),
                description: t(Text::ToolLinterDesc).to_string(),
                enabled: true,
                icon: Icons::CODE,
                category: ToolCategory::Analysis,
            },
            ToolConfig {
                id: "analyzer".to_string(),
                name: t(Text::ToolAnalyzer).to_string(),
                description: t(Text::ToolAnalyzerDesc).to_string(),
                enabled: true,
                icon: "📊",
                category: ToolCategory::Analysis,
            },
            ToolConfig {
                id: "formatter".to_string(),
                name: t(Text::ToolFormatter).to_string(),
                description: t(Text::ToolFormatterDesc).to_string(),
                enabled: true,
                icon: "✨",
                category: ToolCategory::Analysis,
            },
            ToolConfig {
                id: "refactor".to_string(),
                name: t(Text::ToolRefactor).to_string(),
                description: t(Text::ToolRefactorDesc).to_string(),
                enabled: true,
                icon: "🔄",
                category: ToolCategory::Analysis,
            },
            // Git
            ToolConfig {
                id: "git".to_string(),
                name: t(Text::ToolGit).to_string(),
                description: t(Text::ToolGitDesc).to_string(),
                enabled: true,
                icon: "🔀",
                category: ToolCategory::Git,
            },
            // Project
            ToolConfig {
                id: "dependencies".to_string(),
                name: t(Text::ToolDependencies).to_string(),
                description: t(Text::ToolDependenciesDesc).to_string(),
                enabled: true,
                icon: "📦",
                category: ToolCategory::Project,
            },
            ToolConfig {
                id: "documentation".to_string(),
                name: t(Text::ToolDocumentation).to_string(),
                description: t(Text::ToolDocumentationDesc).to_string(),
                enabled: true,
                icon: "📝",
                category: ToolCategory::Project,
            },
            ToolConfig {
                id: "context".to_string(),
                name: t(Text::ToolContext).to_string(),
                description: t(Text::ToolContextDesc).to_string(),
                enabled: true,
                icon: "🎯",
                category: ToolCategory::Project,
            },
            // Utilities
            ToolConfig {
                id: "http".to_string(),
                name: t(Text::ToolHttp).to_string(),
                description: t(Text::ToolHttpDesc).to_string(),
                enabled: true,
                icon: "🌐",
                category: ToolCategory::Utilities,
            },
            ToolConfig {
                id: "snippets".to_string(),
                name: t(Text::ToolSnippets).to_string(),
                description: t(Text::ToolSnippetsDesc).to_string(),
                enabled: true,
                icon: "📋",
                category: ToolCategory::Utilities,
            },
            ToolConfig {
                id: "environment".to_string(),
                name: t(Text::ToolEnvironment).to_string(),
                description: t(Text::ToolEnvironmentDesc).to_string(),
                enabled: true,
                icon: "🖥",
                category: ToolCategory::Utilities,
            },
            ToolConfig {
                id: "planner".to_string(),
                name: t(Text::ToolPlanner).to_string(),
                description: t(Text::ToolPlannerDesc).to_string(),
                enabled: true,
                icon: "📋",
                category: ToolCategory::Utilities,
            },
        ]
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.tools.len().saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + self.visible_items {
                self.scroll_offset = self.selected_index - self.visible_items + 1;
            }
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(tool) = self.tools.get_mut(self.selected_index) {
            tool.enabled = !tool.enabled;
        }
    }

    pub fn get_enabled_tools(&self) -> Vec<&ToolConfig> {
        self.tools.iter().filter(|t| t.enabled).collect()
    }

    pub fn get_enabled_tool_ids(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn is_tool_enabled(&self, id: &str) -> bool {
        self.tools.iter().any(|t| t.id == id && t.enabled)
    }

    pub fn tick(&mut self) {
        self.animation.tick();
    }

    /// Get visible tools for rendering
    pub fn visible_tools(&self) -> impl Iterator<Item = (usize, &ToolConfig)> {
        self.tools
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.visible_items)
    }

    /// Check if we can scroll
    pub fn can_scroll_up(&self) -> bool {
        self.scroll_offset > 0
    }

    pub fn can_scroll_down(&self) -> bool {
        self.scroll_offset + self.visible_items < self.tools.len()
    }

    /// Update tool names based on current locale
    pub fn refresh_locale(&mut self) {
        // Re-create tools with updated locale strings
        let enabled_states: Vec<bool> = self.tools.iter().map(|t| t.enabled).collect();
        self.tools = Self::default_tools();
        
        // Restore enabled states
        for (i, enabled) in enabled_states.into_iter().enumerate() {
            if let Some(tool) = self.tools.get_mut(i) {
                tool.enabled = enabled;
            }
        }
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// App-wide settings
#[derive(Debug, Clone)]
pub struct AppSettings {
    pub theme_name: String,
    pub show_thinking: bool,
    pub auto_scroll: bool,
    pub confirm_dangerous: bool,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_name: "dark".to_string(),
            show_thinking: true,
            auto_scroll: true,
            confirm_dangerous: true,
            timeout_secs: 1200,
            max_retries: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_panel_navigation() {
        let mut panel = SettingsPanel::new();
        assert_eq!(panel.selected_index, 0);
        
        panel.move_down();
        assert_eq!(panel.selected_index, 1);
        
        panel.move_up();
        assert_eq!(panel.selected_index, 0);
        
        panel.move_up(); // Should stay at 0
        assert_eq!(panel.selected_index, 0);
    }

    #[test]
    fn test_tool_toggle() {
        let mut panel = SettingsPanel::new();
        let initial = panel.tools[0].enabled;
        
        panel.toggle_selected();
        assert_eq!(panel.tools[0].enabled, !initial);
        
        panel.toggle_selected();
        assert_eq!(panel.tools[0].enabled, initial);
    }

    #[test]
    fn test_get_enabled_tools() {
        let mut panel = SettingsPanel::new();
        let initial_count = panel.get_enabled_tools().len();
        
        panel.toggle_selected(); // Disable first tool
        assert_eq!(panel.get_enabled_tools().len(), initial_count - 1);
    }
}
