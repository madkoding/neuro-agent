//! Layout components for modern UI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::animations::{ProgressBar, Spinner, StatusIndicator};
use super::theme::{Icons, Theme};
use crate::i18n::{current_locale, t, Locale, Text};

/// Main application layout manager
pub struct AppLayout {
    pub theme: Theme,
}

impl AppLayout {
    pub fn new(theme: Theme) -> Self {
        Self { theme }
    }

    /// Create the main layout areas
    pub fn create_areas(&self, area: Rect) -> LayoutAreas {
        // Main vertical split: header, content, footer
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(3), // Footer/Input
            ])
            .split(area);

        LayoutAreas {
            header: main_chunks[0],
            content: main_chunks[1],
            footer: main_chunks[2],
        }
    }

    /// Create content area with optional sidebar
    pub fn create_content_areas(&self, content: Rect, show_sidebar: bool) -> ContentAreas {
        if show_sidebar {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(70), // Main content
                    Constraint::Percentage(30), // Sidebar
                ])
                .split(content);

            ContentAreas {
                main: chunks[0],
                sidebar: Some(chunks[1]),
            }
        } else {
            ContentAreas {
                main: content,
                sidebar: None,
            }
        }
    }

    /// Render header with title and status
    pub fn render_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        status: &StatusIndicator,
        status_text: &str,
    ) {
        let (icon, color) = status.render();

        let title = Line::from(vec![
            Span::styled(format!(" {} ", t(Text::AppTitle)), self.theme.title_style()),
            Span::raw(" "),
            Span::styled(
                icon,
                Style::default().fg(ratatui::style::Color::Rgb(color.0, color.1, color.2)),
            ),
            Span::raw(" "),
            Span::styled(status_text, self.theme.muted_style()),
        ]);

        let locale_indicator = match current_locale() {
            Locale::English => "🇺🇸 EN",
            Locale::Spanish => "🇪🇸 ES",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style(false))
            .title(title)
            .title_alignment(Alignment::Left);

        // Render locale indicator on the right
        let header_text = Paragraph::new(Line::from(vec![Span::styled(
            locale_indicator,
            self.theme.muted_style(),
        )]))
        .alignment(Alignment::Right)
        .block(block);

        frame.render_widget(header_text, area);
    }

    /// Render footer with shortcuts
    pub fn render_footer(&self, frame: &mut Frame, area: Rect, mode: FooterMode) {
        let shortcuts = match mode {
            FooterMode::Chat => vec![
                (Icons::SEND, t(Text::PressEnterToSend)),
                (Icons::CANCEL, t(Text::PressEscToCancel)),
                (Icons::SETTINGS, t(Text::PressTabForSettings)),
                ("Q", t(Text::PressQToQuit)),
            ],
            FooterMode::Settings => vec![
                ("↑↓", "Navigate"),
                ("Space", t(Text::ToggleTool)),
                (Icons::SETTINGS, t(Text::BackToChat)),
                ("Q", t(Text::PressQToQuit)),
            ],
            FooterMode::Confirmation => vec![
                ("Y", "Yes"),
                ("N", "No"),
                (Icons::CANCEL, t(Text::PressEscToCancel)),
            ],
        };

        let spans: Vec<Span> = shortcuts
            .into_iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(format!(" {} ", key), self.theme.shortcut_key_style()),
                    Span::styled(format!("{} ", desc), self.theme.shortcut_desc_style()),
                    Span::raw("│"),
                ]
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style(false));

        let footer = Paragraph::new(Line::from(spans))
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(footer, area);
    }

    /// Render a message bubble
    pub fn render_message_bubble(
        &self,
        content: &str,
        sender: MessageSender,
        is_streaming: bool,
    ) -> Vec<Line<'static>> {
        let (icon, style, _alignment) = match sender {
            MessageSender::User => (Icons::USER, self.theme.user_style(), "right"),
            MessageSender::Assistant => (Icons::ASSISTANT, self.theme.assistant_style(), "left"),
            MessageSender::System => (Icons::SYSTEM, self.theme.system_style(), "center"),
            MessageSender::Tool => (Icons::TOOL, self.theme.tool_style(), "left"),
        };

        let header = Line::from(vec![
            Span::styled(format!("{} ", icon), style),
            if is_streaming {
                Span::styled("...", self.theme.muted_style())
            } else {
                Span::raw("")
            },
        ]);

        let mut lines = vec![header];

        for line in content.lines() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line.to_string(), style),
            ]));
        }

        lines.push(Line::from("")); // Spacing
        lines
    }

    /// Render thinking/processing indicator
    pub fn render_thinking_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        spinner: &Spinner,
        message: &str,
        elapsed_secs: u64,
    ) {
        let time_str = format_duration(elapsed_secs);

        let content = vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled(spinner.frame(), self.theme.primary_style()),
                Span::raw(" "),
                Span::styled(message, self.theme.info_style()),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(format!("⏱ {}", time_str), self.theme.muted_style()),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style(false))
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(Span::styled(
                format!(" {} ", t(Text::Thinking)),
                self.theme.accent_style(),
            ));

        let paragraph = Paragraph::new(content).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render a modal dialog
    pub fn render_modal(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        content: Vec<Line<'_>>,
        style: ModalStyle,
    ) {
        let modal_area = centered_rect(60, 40, area);

        // Clear background
        frame.render_widget(Clear, modal_area);

        let border_style = match style {
            ModalStyle::Info => self.theme.info_style(),
            ModalStyle::Warning => self.theme.warning_style(),
            ModalStyle::Error => self.theme.error_style(),
            ModalStyle::Success => self.theme.success_style(),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_type(ratatui::widgets::BorderType::Double)
            .title(Span::styled(
                format!(" {} ", title),
                border_style.add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, modal_area);
    }

    /// Render progress indicator
    pub fn render_progress(
        &self,
        frame: &mut Frame,
        area: Rect,
        progress: &ProgressBar,
        label: &str,
    ) {
        let content = Line::from(vec![
            Span::styled(format!("{}: ", label), self.theme.muted_style()),
            Span::styled(progress.render(), self.theme.primary_style()),
        ]);

        frame.render_widget(Paragraph::new(content), area);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutAreas {
    pub header: Rect,
    pub content: Rect,
    pub footer: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct ContentAreas {
    pub main: Rect,
    pub sidebar: Option<Rect>,
}

#[derive(Debug, Clone, Copy)]
pub enum FooterMode {
    Chat,
    Settings,
    Confirmation,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageSender {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Copy)]
pub enum ModalStyle {
    Info,
    Warning,
    Error,
    Success,
}

/// Create a centered rectangle
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Format duration in human readable format
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Input field with modern styling
pub struct ModernInput<'a> {
    content: &'a str,
    #[allow(dead_code)]
    cursor_pos: usize,
    placeholder: &'a str,
    focused: bool,
    theme: &'a Theme,
}

impl<'a> ModernInput<'a> {
    pub fn new(content: &'a str, cursor_pos: usize, theme: &'a Theme) -> Self {
        Self {
            content,
            cursor_pos,
            placeholder: "",
            focused: true,
            theme,
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let display_text = if self.content.is_empty() {
            Span::styled(self.placeholder, self.theme.muted_style())
        } else {
            Span::styled(self.content, self.theme.base_style())
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_style(self.focused))
            .border_type(if self.focused {
                ratatui::widgets::BorderType::Thick
            } else {
                ratatui::widgets::BorderType::Rounded
            })
            .title(if self.focused {
                Span::styled(" > ", self.theme.accent_style())
            } else {
                Span::raw("")
            });

        let input = Paragraph::new(Line::from(vec![
            Span::raw(" "),
            display_text,
            if self.focused {
                Span::styled("▎", self.theme.accent_style())
            } else {
                Span::raw("")
            },
        ]))
        .block(block);

        frame.render_widget(input, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 50, area);

        assert!(centered.x > 0);
        assert!(centered.y > 0);
        assert!(centered.width < area.width);
        assert!(centered.height < area.height);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
    }
}
