//! Status Message Strip Widget
//!
//! Displays a thin, single-line status area between messages and input.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use crate::tui_new::theme::Theme;

/// Dot characters for TUI rendering
const DOT_SMALL: char = '·'; // Middle dot (U+00B7)
const DOT_LARGE: char = '●'; // Black circle (U+25CF)

/// Working bar characters (thin to thick)
const WORKING_BAR_THIN: char = '─'; // Box drawings light horizontal (U+2500)
const WORKING_BAR_MEDIUM: char = '═'; // Box drawings double horizontal (U+2550)
const WORKING_BAR_THICK: char = '━'; // Box drawings heavy horizontal (U+2501)

/// Maximum animation frame for the working pulse
const MAX_FRAME: u8 = 20;

/// State of the FlashBar widget
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlashBarState {
    /// Single muted center dot - system is idle
    #[default]
    Idle,
    /// Animated dots expanding/contracting - system is processing
    Working,
    /// Bordered card with red indicator - error occurred
    Error,
    /// Bordered card with yellow indicator - warning (includes rate-limit)
    Warning,
}

/// FlashBar widget for displaying status indicators
pub struct FlashBar<'a> {
    message: Option<&'a str>,
    kind: FlashBarState,
    /// Animation frame (0-4) for working state
    animation_frame: u8,
    theme: &'a Theme,
}

impl<'a> FlashBar<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            message: None,
            kind: FlashBarState::Idle,
            animation_frame: 0,
            theme,
        }
    }

    pub fn message(mut self, message: &'a str) -> Self {
        self.message = Some(message);
        self
    }

    pub fn kind(mut self, kind: FlashBarState) -> Self {
        self.kind = kind;
        self
    }

    pub fn animation_frame(mut self, frame: u8) -> Self {
        self.animation_frame = frame;
        self
    }

    /// Get the default message for a state
    fn default_message(&self) -> &'static str {
        match self.kind {
            FlashBarState::Error => "CRITICAL ERROR: Connection failed.",
            FlashBarState::Warning => "Request timeout, retrying...",
            FlashBarState::Idle | FlashBarState::Working => "",
        }
    }

    /// Get the style for the current state
    fn state_style(&self) -> Style {
        let fg = match self.kind {
            FlashBarState::Idle => self.theme.text_muted,
            FlashBarState::Working => self.theme.cyan,
            FlashBarState::Error => self.theme.red,
            FlashBarState::Warning => self.theme.yellow,
        };
        Style::default().fg(fg).bg(self.theme.bg_dark)
    }
}

impl Widget for FlashBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.kind {
            FlashBarState::Idle => {
                // Render single muted center dot
                self.render_idle(area, buf);
            }
            FlashBarState::Working => {
                // Render animated dots
                self.render_working(area, buf);
            }
            FlashBarState::Error | FlashBarState::Warning => {
                // Render bordered card with message
                self.render_message_state(area, buf);
            }
        }
    }
}

impl FlashBar<'_> {
    /// Render idle state: single muted center dot
    fn render_idle(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Fill background
        let base_style = Style::default().bg(self.theme.bg_dark);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_style(base_style);
            }
        }

        if let Some(message) = self.message {
            self.render_idle_message(area, buf, message);
            return;
        }

        // Calculate center position
        let center_x = area.left() + area.width / 2;
        let center_y = area.top();

        // Render single muted dot at center
        let dot_style = Style::default()
            .fg(self.theme.text_muted)
            .bg(self.theme.bg_dark);

        buf[(center_x, center_y)].set_char('·').set_style(dot_style);
    }

    fn render_idle_message(&self, area: Rect, buf: &mut Buffer, message: &str) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = Style::default()
            .fg(self.theme.text_muted)
            .bg(self.theme.bg_dark);

        let trimmed: String = message.chars().take(area.width as usize).collect();
        let text_width = trimmed.chars().count() as u16;
        let start_x = area.left() + (area.width.saturating_sub(text_width)) / 2;
        let y = area.top();

        for (i, ch) in trimmed.chars().enumerate() {
            let x = start_x + i as u16;
            if x < area.right() {
                buf[(x, y)].set_char(ch).set_style(style);
            }
        }
    }

    /// Render working state: smooth pulse bar
    fn render_working(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Fill background
        let base_style = Style::default().bg(self.theme.bg_dark);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_style(base_style);
            }
        }

        // Clamp animation frame to valid range
        let frame = self.animation_frame.min(MAX_FRAME);
        let center_y = area.top();

        let width = area.width as f32;
        let center = area.left() as f32 + (width - 1.0) / 2.0;
        let t = if MAX_FRAME == 0 {
            0.0
        } else {
            frame as f32 / MAX_FRAME as f32
        };
        let max_radius = (width * 0.5).max(1.0);
        let radius = 1.0 + (max_radius - 1.0) * t;
        let base_strength = 0.18;
        let pulse_strength = 0.45 + 0.55 * t;

        for x in area.left()..area.right() {
            let dist = ((x as f32) - center).abs();
            let falloff = if radius <= 0.0 {
                0.0
            } else {
                (1.0 - (dist / radius)).clamp(0.0, 1.0)
            };
            let strength = (base_strength + (pulse_strength * falloff)).clamp(0.0, 1.0);
            let ch = if falloff >= 0.66 {
                WORKING_BAR_THICK
            } else if falloff >= 0.33 {
                WORKING_BAR_MEDIUM
            } else {
                WORKING_BAR_THIN
            };
            let fg = if dist <= 0.5 {
                let cap = blend_color(self.theme.cyan, self.theme.text_primary, 0.45);
                blend_color(self.theme.bg_dark, cap, strength.max(0.9))
            } else {
                blend_color(self.theme.bg_dark, self.theme.cyan, strength)
            };
            let style = Style::default().fg(fg).bg(self.theme.bg_dark);
            buf[(x, center_y)].set_char(ch).set_style(style);
        }
    }

    /// Render message state (error, warning) with bordered card
    fn render_message_state(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Get the message to display (custom or default)
        let message = self.message.unwrap_or_else(|| self.default_message());

        // Get the state-specific style
        let state_style = self.state_style();

        // Fill background
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_style(state_style);
            }
        }

        let inner_width = area.width.saturating_sub(2) as usize;
        let indicator = format!("{} {} ", DOT_LARGE, DOT_SMALL);
        let indicator_width = indicator.chars().count();

        let mut inside = String::new();
        if inner_width > 0 {
            if indicator_width >= inner_width {
                inside.extend(indicator.chars().take(inner_width));
            } else {
                inside.push_str(&indicator);
                let available = inner_width.saturating_sub(indicator_width);
                let message_trimmed: String = message.chars().take(available).collect();
                let message_len = message_trimmed.chars().count();
                let left_pad = (available.saturating_sub(message_len)) / 2;
                let right_pad = available.saturating_sub(message_len + left_pad);
                inside.push_str(&" ".repeat(left_pad));
                inside.push_str(&message_trimmed);
                inside.push_str(&" ".repeat(right_pad));
            }
        }

        let content = format!("│{}│", inside);
        let y = area.top();

        for (i, ch) in content.chars().enumerate() {
            let x = area.left() + i as u16;
            if x < area.right() {
                buf[(x, y)].set_char(ch).set_style(state_style);
            }
        }
    }
}

fn blend_color(
    base: ratatui::style::Color,
    accent: ratatui::style::Color,
    t: f32,
) -> ratatui::style::Color {
    let t = t.clamp(0.0, 1.0);
    match (base, accent) {
        (ratatui::style::Color::Rgb(br, bg, bb), ratatui::style::Color::Rgb(ar, ag, ab)) => {
            let r = br as f32 + (ar as f32 - br as f32) * t;
            let g = bg as f32 + (ag as f32 - bg as f32) * t;
            let b = bb as f32 + (ab as f32 - bb as f32) * t;
            ratatui::style::Color::Rgb(r.round() as u8, g.round() as u8, b.round() as u8)
        }
        _ => accent,
    }
}
