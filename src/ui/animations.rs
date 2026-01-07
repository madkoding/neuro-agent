//! Animation utilities for TUI feedback

use std::time::{Duration, Instant};

/// Animated spinner with multiple styles
#[derive(Debug, Clone)]
pub struct Spinner {
    frames: &'static [&'static str],
    current_frame: usize,
    last_update: Instant,
    interval: Duration,
}

impl Spinner {
    pub fn dots() -> Self {
        Self {
            frames: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(80),
        }
    }

    pub fn braille() -> Self {
        Self {
            frames: &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(100),
        }
    }

    pub fn blocks() -> Self {
        Self {
            frames: &[
                "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█", "▉", "▊", "▋", "▌", "▍", "▎",
            ],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(120),
        }
    }

    pub fn bounce() -> Self {
        Self {
            frames: &["⠁", "⠂", "⠄", "⠂"],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(150),
        }
    }

    pub fn clock() -> Self {
        Self {
            frames: &[
                "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚", "🕛",
            ],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(200),
        }
    }

    pub fn thinking() -> Self {
        Self {
            frames: &["◐", "◓", "◑", "◒"],
            current_frame: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(150),
        }
    }

    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            self.last_update = Instant::now();
        }
    }

    pub fn frame(&self) -> &'static str {
        self.frames[self.current_frame]
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.last_update = Instant::now();
    }
}

/// Progress bar with percentage
#[derive(Debug, Clone)]
pub struct ProgressBar {
    progress: f64,
    width: usize,
    filled_char: char,
    empty_char: char,
    show_percentage: bool,
}

impl ProgressBar {
    pub fn new(width: usize) -> Self {
        Self {
            progress: 0.0,
            width,
            filled_char: '█',
            empty_char: '░',
            show_percentage: true,
        }
    }

    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    pub fn increment(&mut self, amount: f64) {
        self.progress = (self.progress + amount).clamp(0.0, 1.0);
    }

    pub fn render(&self) -> String {
        let filled = (self.progress * self.width as f64) as usize;
        let empty = self.width - filled;

        let bar: String = std::iter::repeat_n(self.filled_char, filled)
            .chain(std::iter::repeat_n(self.empty_char, empty))
            .collect();

        if self.show_percentage {
            format!("[{}] {:3.0}%", bar, self.progress * 100.0)
        } else {
            format!("[{}]", bar)
        }
    }

    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }
}

/// Pulsing animation for attention
#[derive(Debug, Clone)]
pub struct Pulse {
    intensity: f64,
    direction: f64,
    speed: f64,
    last_update: Instant,
}

impl Pulse {
    pub fn new(speed: f64) -> Self {
        Self {
            intensity: 0.0,
            direction: 1.0,
            speed,
            last_update: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        let elapsed = self.last_update.elapsed().as_secs_f64();
        self.last_update = Instant::now();

        self.intensity += self.direction * self.speed * elapsed;

        if self.intensity >= 1.0 {
            self.intensity = 1.0;
            self.direction = -1.0;
        } else if self.intensity <= 0.0 {
            self.intensity = 0.0;
            self.direction = 1.0;
        }
    }

    pub fn intensity(&self) -> f64 {
        self.intensity
    }

    /// Get a color intensity value (0-255)
    pub fn color_value(&self) -> u8 {
        (self.intensity * 255.0) as u8
    }
}

/// Typing indicator animation
#[derive(Debug, Clone)]
pub struct TypingIndicator {
    dots: usize,
    max_dots: usize,
    last_update: Instant,
    interval: Duration,
}

impl TypingIndicator {
    pub fn new() -> Self {
        Self {
            dots: 0,
            max_dots: 3,
            last_update: Instant::now(),
            interval: Duration::from_millis(500),
        }
    }

    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.dots = (self.dots + 1) % (self.max_dots + 1);
            self.last_update = Instant::now();
        }
    }

    pub fn render(&self) -> String {
        format!("●{}", ".".repeat(self.dots))
    }

    pub fn render_with_text(&self, text: &str) -> String {
        format!("{}{}", text, ".".repeat(self.dots))
    }
}

/// Wave animation for text
#[derive(Debug, Clone)]
pub struct TextWave {
    offset: usize,
    last_update: Instant,
    interval: Duration,
}

impl TextWave {
    pub fn new() -> Self {
        Self {
            offset: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(150),
        }
    }

    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.offset = self.offset.wrapping_add(1);
            self.last_update = Instant::now();
        }
    }

    /// Apply wave effect to text (returns vector of (char, y_offset) pairs)
    pub fn apply(&self, text: &str) -> Vec<(char, i32)> {
        text.chars()
            .enumerate()
            .map(|(i, c)| {
                let phase = (i + self.offset) as f64 * 0.5;
                let y_offset = (phase.sin() * 1.0) as i32;
                (c, y_offset)
            })
            .collect()
    }
}

/// Status indicator with color cycling
#[derive(Debug, Clone)]
pub struct StatusIndicator {
    state: StatusState,
    spinner: Spinner,
    pulse: Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusState {
    Idle,
    Working,
    Success,
    Error,
    Warning,
}

impl StatusIndicator {
    pub fn new() -> Self {
        Self {
            state: StatusState::Idle,
            spinner: Spinner::dots(),
            pulse: Pulse::new(3.0),
        }
    }

    pub fn set_state(&mut self, state: StatusState) {
        if self.state != state {
            self.state = state;
            self.spinner.reset();
        }
    }

    pub fn tick(&mut self) {
        match self.state {
            StatusState::Working => self.spinner.tick(),
            StatusState::Error | StatusState::Warning => self.pulse.tick(),
            _ => {}
        }
    }

    pub fn render(&self) -> (&'static str, (u8, u8, u8)) {
        match self.state {
            StatusState::Idle => ("●", (100, 100, 100)),
            StatusState::Working => (self.spinner.frame(), (100, 200, 255)),
            StatusState::Success => ("✓", (100, 255, 100)),
            StatusState::Error => {
                let v = 150 + (self.pulse.color_value() / 3);
                ("✗", (255, v, v))
            }
            StatusState::Warning => {
                let v = 200 + (self.pulse.color_value() / 5);
                ("⚠", (255, v, 100))
            }
        }
    }

    pub fn state(&self) -> StatusState {
        self.state
    }
}

impl Default for TypingIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TextWave {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for StatusIndicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_cycles() {
        let mut spinner = Spinner::dots();
        let first = spinner.frame();

        // Force update
        spinner.last_update = Instant::now() - Duration::from_secs(1);
        spinner.tick();

        // Frame should have changed
        assert!(spinner.current_frame > 0 || spinner.frame() != first);
    }

    #[test]
    fn test_progress_bar() {
        let mut bar = ProgressBar::new(10);
        assert!(!bar.is_complete());

        bar.set_progress(0.5);
        let rendered = bar.render();
        assert!(rendered.contains("50%"));

        bar.set_progress(1.0);
        assert!(bar.is_complete());
    }

    #[test]
    fn test_typing_indicator() {
        let indicator = TypingIndicator::new();
        let rendered = indicator.render();
        assert!(rendered.starts_with('●'));
    }
}
