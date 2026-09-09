//! Terminal UI rendering (ratatui) for the Tap Along session.
//!
//! Kept deliberately separate from the input/audio/session-loop wiring in
//! `main.rs`: this module only knows how to draw a given `UiState` snapshot.
//! It has no opinion on where that state comes from, which keeps it trivial
//! to later reuse for Quiet Four / Tuplets (see `specs/keyboard-modes.md`)
//! by feeding it a differently-populated `UiState`.

use std::time::{Duration, Instant};

use quietmytempo_core::{SessionSummary, TapResult};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// How long the metronome panel stays visually "lit" after a click, purely
/// a UI/perception choice — long enough to register as a flash, short
/// enough not to blur together at fast tempos.
pub const FLASH_DURATION: Duration = Duration::from_millis(90);

/// How many recent tap results to keep visible in the history list.
const HISTORY_LEN: usize = 10;

/// Everything the UI needs to render one frame. Rebuilt/updated by the main
/// loop each iteration; contains no rendering logic itself.
pub struct UiState {
    pub bpm: f64,
    pub mode_name: &'static str,
    /// Set right when a metronome click fires; used to drive the panel
    /// flash countdown against `Instant::now()` at render time.
    pub last_click_at: Option<Instant>,
    pub history: Vec<TapResult>,
    pub summary: SessionSummary,
    pub rejected_taps: usize,
    /// Measured latency offset (ms) being compensated for, from the
    /// calibration step in `calibration.rs`. Shown to the player so the
    /// sync fix is visible/trustworthy rather than a hidden magic number.
    pub latency_offset_ms: i64,
    /// Device-reported output latency estimate (informational only, see
    /// `metronome::LatencyEstimate`) — shown alongside the calibrated
    /// offset so the player can see both numbers.
    pub device_latency_ms: i64,
}

impl UiState {
    pub fn new(bpm: f64, mode_name: &'static str) -> Self {
        Self {
            bpm,
            mode_name,
            last_click_at: None,
            history: Vec::new(),
            summary: SessionSummary::default(),
            rejected_taps: 0,
            latency_offset_ms: 0,
            device_latency_ms: 0,
        }
    }

    pub fn push_result(&mut self, result: TapResult) {
        self.history.push(result);
        if self.history.len() > HISTORY_LEN {
            self.history.remove(0);
        }
    }

    fn is_flashing(&self) -> bool {
        self.last_click_at
            .is_some_and(|t| t.elapsed() < FLASH_DURATION)
    }
}

pub fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // metronome flash panel
            Constraint::Length(3),  // header/info
            Constraint::Min(6),     // tap history
            Constraint::Length(5),  // summary
            Constraint::Length(1),  // help line
        ])
        .split(area);

    draw_metronome_panel(frame, chunks[0], state);
    draw_header(frame, chunks[1], state);
    draw_history(frame, chunks[2], state);
    draw_summary(frame, chunks[3], state);
    draw_help(frame, chunks[4]);
}

fn draw_metronome_panel(frame: &mut Frame, area: Rect, state: &UiState) {
    let (bg, label) = if state.is_flashing() {
        (Color::Yellow, "  ●  BEAT  ●  ")
    } else {
        (Color::DarkGray, "     tick     ")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Metronome ");
    let paragraph = Paragraph::new(Line::from(Span::styled(
        label,
        Style::default()
            .fg(Color::Black)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(block);
    frame.render_widget(paragraph, area);
}

fn draw_header(frame: &mut Frame, area: Rect, state: &UiState) {
    let text = format!(
        "Mode: {}   BPM: {:.0}   Calibrated offset: {} ms   Device latency (info): {} ms   (rejected taps: {})",
        state.mode_name, state.bpm, state.latency_offset_ms, state.device_latency_ms, state.rejected_taps
    );
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Session "));
    frame.render_widget(paragraph, area);
}

fn draw_history(frame: &mut Frame, area: Rect, state: &UiState) {
    const SCALE_MS: f64 = 100.0;
    const WIDTH: usize = 31;
    let half = (WIDTH / 2) as f64;

    let lines: Vec<Line> = state
        .history
        .iter()
        .rev()
        .map(|result| {
            let clamped = result.deviation_ms.clamp(-SCALE_MS, SCALE_MS);
            let pos = (clamped / SCALE_MS * half).round() as isize + half as isize;
            let pos = pos.clamp(0, WIDTH as isize - 1) as usize;

            let mut meter = vec!['.'; WIDTH];
            meter[WIDTH / 2] = '|';
            meter[pos] = '#';
            let meter: String = meter.into_iter().collect();

            let color = deviation_color(result.deviation_ms);
            let label = if result.expected.informational {
                " (info)"
            } else {
                ""
            };

            Line::from(vec![
                Span::styled(
                    format!("[{meter}]"),
                    Style::default().fg(color),
                ),
                Span::raw(format!(" {:>+7.1} ms{label}", result.deviation_ms)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Recent Taps (newest first) "),
    );
    frame.render_widget(paragraph, area);
}

fn draw_summary(frame: &mut Frame, area: Rect, state: &UiState) {
    let text = if state.summary.scored_count > 0 {
        format!(
            "Scored taps: {}   Mean: {:+.1} ms   Consistency (stddev): {:.1} ms",
            state.summary.scored_count, state.summary.mean_deviation_ms, state.summary.std_dev_ms
        )
    } else {
        "Scored taps: 0".to_string()
    };
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Summary "));
    frame.render_widget(paragraph, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let paragraph = Paragraph::new("SPACE = tap   |   q / Esc = quit")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(paragraph, area);
}

fn deviation_color(deviation_ms: f64) -> Color {
    match deviation_ms.abs() {
        d if d <= 15.0 => Color::Green,
        d if d <= 40.0 => Color::Yellow,
        _ => Color::Red,
    }
}
