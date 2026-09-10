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
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use ratatui::Frame;

/// How long the metronome panel stays visually "lit" after a click. This
/// drives the little pop/pulse of the gem widget — long enough to register
/// as a beat, short enough not to blur together at fast tempos.
pub const FLASH_DURATION: Duration = Duration::from_millis(220);

/// How long a tap's accuracy color stays shown on the gem after a tap,
/// before fading back to the idle/beat colors. Separate from
/// `FLASH_DURATION` since a tap and a beat click don't happen at the same
/// instant.
const TAP_FLASH_DURATION: Duration = Duration::from_millis(320);

/// How many recent tap results to keep visible in the history list.
const HISTORY_LEN: usize = 10;

/// Idle color of the pulse gem when nothing interesting is happening.
const IDLE_GEM_COLOR: Color = Color::Rgb(70, 78, 96);
/// Color of the pulse gem right as a metronome beat fires.
const BEAT_GEM_COLOR: Color = Color::Rgb(120, 190, 255);

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
    /// Set right when a tap is scored; drives the gem's brief accuracy-
    /// colored flash, independent of the beat flash above.
    last_tap_at: Option<Instant>,
    last_tap_color: Color,
    /// 1-based bar number within the session, and 1-based beat number
    /// within that bar — purely a "where am I in the 4/4 grid" display
    /// aid, computed by the caller from the session clock (see
    /// `main.rs::run_loop`). Not used for scoring, only for orientation.
    pub current_bar: usize,
    pub current_beat_in_bar: usize,
    pub beats_per_bar: usize,
    /// Running counts of scored taps by accuracy bucket (green/yellow/
    /// red, same thresholds as [`deviation_color`]), used to render the
    /// end-of-session result screen's distribution bar.
    green_count: usize,
    yellow_count: usize,
    red_count: usize,
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
            last_tap_at: None,
            last_tap_color: IDLE_GEM_COLOR,
            current_bar: 1,
            current_beat_in_bar: 1,
            beats_per_bar: 4,
            green_count: 0,
            yellow_count: 0,
            red_count: 0,
        }
    }

    pub fn push_result(&mut self, result: TapResult) {
        self.last_tap_at = Some(Instant::now());
        self.last_tap_color = deviation_color(result.deviation_ms);
        if result.is_scored() {
            match result.deviation_ms.abs() {
                d if d <= 15.0 => self.green_count += 1,
                d if d <= 40.0 => self.yellow_count += 1,
                _ => self.red_count += 1,
            }
        }
        self.history.push(result);
        if self.history.len() > HISTORY_LEN {
            self.history.remove(0);
        }
    }

    /// 0.0 (no flash) .. 1.0 (just fired) linear decay, used to drive the
    /// gem's brief "pop" in size/brightness right on the beat.
    fn beat_flash_intensity(&self) -> f64 {
        self.last_click_at.map_or(0.0, |t| {
            let elapsed = t.elapsed();
            if elapsed >= FLASH_DURATION {
                0.0
            } else {
                1.0 - elapsed.as_secs_f64() / FLASH_DURATION.as_secs_f64()
            }
        })
    }

    /// Same idea as [`Self::beat_flash_intensity`] but for the last scored
    /// tap's accuracy color, so the gem briefly reports "how was that tap"
    /// before settling back to its idle/beat look.
    fn tap_flash_intensity(&self) -> f64 {
        self.last_tap_at.map_or(0.0, |t| {
            let elapsed = t.elapsed();
            if elapsed >= TAP_FLASH_DURATION {
                0.0
            } else {
                1.0 - elapsed.as_secs_f64() / TAP_FLASH_DURATION.as_secs_f64()
            }
        })
    }
}

pub fn draw(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // big pulse gem panel
            Constraint::Length(3),  // header/info
            Constraint::Length(3),  // bar/beat position indicator
            Constraint::Min(8),     // tap history
            Constraint::Length(6),  // summary + gauges
            Constraint::Length(1),  // help line
        ])
        .split(area);

    draw_pulse_panel(frame, chunks[0], state);
    draw_header(frame, chunks[1], state);
    draw_position(frame, chunks[2], state);
    draw_history(frame, chunks[3], state);
    draw_summary(frame, chunks[4], state);
    draw_help(frame, chunks[5]);
}

/// Base panel chrome shared by every section: rounded corners, a dim
/// border, and a bold title read clearly against a dark background.
fn panel_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(90, 96, 112)))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::Rgb(200, 205, 220))
                .add_modifier(Modifier::BOLD),
        ))
}

/// Renders the big centerpiece: a shaded "gem" (filled rhombus) that pops
/// brighter right on each metronome beat, and briefly tints red/yellow/
/// green to report the accuracy of the most recent tap — a much larger,
/// more alive replacement for the old flat metronome flash bar.
fn draw_pulse_panel(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Pulse");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 6 || inner.height < 3 {
        return;
    }

    let beat = state.beat_flash_intensity();
    let tap = state.tap_flash_intensity();

    // Tap accuracy color briefly overrides the beat color (a tap tells you
    // more than the passive beat pulse does); otherwise the gem blends
    // from idle towards its bright beat color as `beat` rises.
    let color = if tap > 0.0 {
        blend(IDLE_GEM_COLOR, state.last_tap_color, tap)
    } else {
        blend(IDLE_GEM_COLOR, BEAT_GEM_COLOR, beat)
    };

    // The gem "pops" slightly larger right on the beat, then eases back to
    // its resting size — a cheap but effective way to make a static shape
    // feel like it's breathing in time with the music.
    let scale = 1.0 + 0.12 * beat;

    render_gem(frame, inner, scale, color);
}

/// Blends two colors linearly by `t` in `0.0..=1.0` (0 = `a`, 1 = `b`).
fn blend(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (ar, ag, ab) = as_rgb(a);
    let (br, bg, bb) = as_rgb(b);
    let lerp = |x: u8, y: u8| -> u8 { (x as f64 + (y as f64 - x as f64) * t).round() as u8 };
    Color::Rgb(lerp(ar, br), lerp(ag, bg), lerp(ab, bb))
}

fn as_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    }
}

/// Draws a shaded diamond/rhombus filling most of `area`, using four
/// density levels of Unicode block shading (from a faint outer ring to a
/// solid core) so a plain filled shape reads as a soft-edged "gem" rather
/// than a hard-edged box.
fn render_gem(frame: &mut Frame, area: Rect, scale: f64, color: Color) {
    let width = area.width as f64;
    let height = area.height as f64;
    let cx = width / 2.0;
    let cy = height / 2.0;

    // Terminal cells are noticeably taller than wide, so the horizontal
    // radius is given more room than the vertical one to end up looking
    // like a diamond rather than a tall lozenge.
    let rx = (cx * 0.92 * scale).max(1.0);
    let ry = (cy * 0.85 * scale).max(1.0);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(area.height as usize);
    for row in 0..area.height {
        let dy = (row as f64 + 0.5) - cy;
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(area.width as usize);
        let mut run = String::new();
        let mut run_symbol: Option<char> = None;

        let flush = |run: &mut String, symbol: Option<char>, spans: &mut Vec<Span<'static>>| {
            if run.is_empty() {
                return;
            }
            let style = match symbol {
                Some(' ') | None => Style::default(),
                Some(_) => Style::default().fg(color),
            };
            spans.push(Span::styled(std::mem::take(run), style));
        };

        for col in 0..area.width {
            let dx = (col as f64 + 0.5) - cx;
            let ratio = (dx / rx).abs() + (dy / ry).abs();
            let symbol = gem_symbol(ratio);
            if run_symbol != Some(symbol) {
                flush(&mut run, run_symbol, &mut spans);
                run_symbol = Some(symbol);
            }
            run.push(symbol);
        }
        flush(&mut run, run_symbol, &mut spans);
        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

fn gem_symbol(ratio: f64) -> char {
    if ratio > 1.0 {
        ' '
    } else if ratio > 0.82 {
        '░'
    } else if ratio > 0.55 {
        '▒'
    } else if ratio > 0.25 {
        '▓'
    } else {
        '█'
    }
}

fn draw_header(frame: &mut Frame, area: Rect, state: &UiState) {
    let label_style = Style::default().fg(Color::Rgb(150, 155, 170));
    let value_style = Style::default()
        .fg(Color::Rgb(220, 224, 235))
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::styled("Mode ", label_style),
        Span::styled(state.mode_name, value_style),
        Span::raw("   "),
        Span::styled("BPM ", label_style),
        Span::styled(format!("{:.0}", state.bpm), value_style),
        Span::raw("   "),
        Span::styled("Offset ", label_style),
        Span::styled(format!("{} ms", state.latency_offset_ms), value_style),
        Span::raw("   "),
        Span::styled("Device latency ", label_style),
        Span::styled(format!("{} ms", state.device_latency_ms), value_style),
        Span::raw("   "),
        Span::styled("Rejected ", label_style),
        Span::styled(
            format!("{}", state.rejected_taps),
            value_style.fg(if state.rejected_taps > 0 {
                Color::Rgb(230, 140, 90)
            } else {
                Color::Rgb(220, 224, 235)
            }),
        ),
    ]);

    let paragraph = Paragraph::new(line).block(panel_block("Session"));
    frame.render_widget(paragraph, area);
}

/// Shows "where are we in the bar" as a row of small diamonds — one per
/// beat in the bar (4 for a standard 4/4 bar), with the current beat lit
/// up and a bar counter alongside. Purely an orientation aid; has no
/// effect on scoring.
fn draw_position(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Position");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 20 || inner.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = vec![
        Span::styled(
            format!("Bar {:<4}", state.current_bar),
            Style::default()
                .fg(Color::Rgb(200, 205, 220))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];

    for beat in 1..=state.beats_per_bar {
        let is_current = beat == state.current_beat_in_bar;
        let (symbol, color) = if is_current {
            ("◆", Color::Rgb(120, 190, 255))
        } else {
            ("◇", Color::Rgb(90, 96, 112))
        };
        spans.push(Span::styled(
            format!("{symbol} "),
            Style::default().fg(color).add_modifier(if is_current {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("Beat {}/{}", state.current_beat_in_bar, state.beats_per_bar),
        Style::default().fg(Color::Rgb(150, 155, 170)),
    ));

    let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

fn draw_history(frame: &mut Frame, area: Rect, state: &UiState) {
    const SCALE_MS: f64 = 100.0;

    let block = panel_block("Recent Taps (newest first)");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || state.history.is_empty() {
        if state.history.is_empty() {
            let hint = Paragraph::new("Tap SPACE on the beat to see your timing here…")
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM));
            frame.render_widget(hint, inner);
        }
        return;
    }

    // Reserve space for the trailing " +123.4 ms (info)" label so the
    // meter itself can stretch to fill the rest of the panel's width —
    // this is what actually fixes "takes up too little space on screen".
    let label_width: usize = 18;
    let meter_width = inner.width.saturating_sub(label_width as u16 + 2).max(10) as usize;
    let half = (meter_width / 2) as f64;
    let center_col = meter_width / 2;

    let lines: Vec<Line> = state
        .history
        .iter()
        .rev()
        .map(|result| {
            let clamped = result.deviation_ms.clamp(-SCALE_MS, SCALE_MS);
            let pos = (clamped / SCALE_MS * half).round() as isize + half as isize;
            let pos = pos.clamp(0, meter_width as isize - 1) as usize;

            let color = deviation_color(result.deviation_ms);
            let label = if result.expected.informational {
                " (info)"
            } else {
                ""
            };

            let mut meter = vec!['·'; meter_width];
            meter[center_col] = '│';
            meter[pos] = '◆';
            let meter: String = meter.into_iter().collect();

            Line::from(vec![
                Span::styled(meter, Style::default().fg(color)),
                Span::raw("  "),
                Span::styled(
                    format!("{:>+7.1} ms{label}", result.deviation_ms),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn draw_summary(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Summary");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 20 {
        let text = summary_text(state);
        frame.render_widget(Paragraph::new(text), inner);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(inner);

    frame.render_widget(Paragraph::new(summary_text(state)), rows[0]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    let accuracy_ratio = score_ratio(state.summary.mean_deviation_ms.abs());
    let consistency_ratio = score_ratio(state.summary.std_dev_ms);

    render_score_gauge(frame, cols[0], "Accuracy", accuracy_ratio);
    render_score_gauge(frame, cols[1], "Consistency", consistency_ratio);
}

fn summary_text(state: &UiState) -> String {
    if state.summary.scored_count > 0 {
        format!(
            "Scored taps: {}   Mean: {:+.1} ms   Consistency (stddev): {:.1} ms",
            state.summary.scored_count, state.summary.mean_deviation_ms, state.summary.std_dev_ms
        )
    } else {
        "Scored taps: 0".to_string()
    }
}

/// Maps a "distance from perfect" in ms (0 = perfect) to a 0.0..1.0 score,
/// using the same 15ms/40ms thresholds as [`deviation_color`] so the gauge
/// colors line up with the rest of the UI's green/yellow/red language.
fn score_ratio(distance_ms: f64) -> f64 {
    const WORST_MS: f64 = 60.0;
    (1.0 - (distance_ms.abs() / WORST_MS)).clamp(0.0, 1.0)
}

fn render_score_gauge(frame: &mut Frame, area: Rect, label: &str, ratio: f64) {
    let color = match ratio {
        r if r >= (1.0 - 15.0 / 60.0) => Color::Rgb(90, 200, 130),
        r if r >= (1.0 - 40.0 / 60.0) => Color::Rgb(220, 190, 90),
        _ => Color::Rgb(220, 100, 100),
    };

    // Empty/unfilled portion needs to be noticeably lighter than the
    // panel's own background, otherwise a low ratio (e.g. right after a
    // rough session) renders as an almost-invisible dark bar. The label's
    // own style always wins over whatever the gauge painted underneath it
    // (filled or not), so pure white text keeps it readable regardless of
    // how full the bar is — relying on the gauge's fill color for
    // contrast (as before) breaks down once the ratio is low enough that
    // the label sits over the unfilled part of the bar instead of the
    // filled one.
    const EMPTY_BG: Color = Color::Rgb(55, 60, 72);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(color).bg(EMPTY_BG))
        .use_unicode(true)
        .label(Span::styled(
            format!("{label} {:.0}%", ratio * 100.0),
            Style::default()
                .fg(Color::White)
                .bg(EMPTY_BG)
                .add_modifier(Modifier::BOLD),
        ))
        .ratio(ratio);
    frame.render_widget(gauge, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            " SPACE ",
            Style::default()
                .fg(Color::Rgb(20, 20, 24))
                .bg(Color::Rgb(150, 155, 170))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" tap    "),
        Span::styled(
            " q / Esc ",
            Style::default()
                .fg(Color::Rgb(20, 20, 24))
                .bg(Color::Rgb(150, 155, 170))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" quit "),
    ]);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn deviation_color(deviation_ms: f64) -> Color {
    match deviation_ms.abs() {
        d if d <= 15.0 => Color::Rgb(90, 200, 130),
        d if d <= 40.0 => Color::Rgb(220, 190, 90),
        _ => Color::Rgb(220, 100, 100),
    }
}

/// Renders the end-of-session result screen: an overall grade, headline
/// stats, a green/yellow/red distribution bar over every scored tap, and
/// the same accuracy/consistency gauges used in the live summary panel —
/// shown once after the player quits so the session doesn't just end on
/// a blank terminal with no takeaway.
pub fn draw_result(frame: &mut Frame, state: &UiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(7), // grade + headline stats
            Constraint::Length(4), // distribution bar
            Constraint::Length(5), // gauges
            Constraint::Min(1),    // spacer
            Constraint::Length(1), // help line
        ])
        .split(area);

    let title = Paragraph::new(Span::styled(
        "Session Complete",
        Style::default()
            .fg(Color::Rgb(220, 224, 235))
            .add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center)
    .block(panel_block("Result"));
    frame.render_widget(title, chunks[0]);

    draw_result_headline(frame, chunks[1], state);
    draw_result_distribution(frame, chunks[2], state);
    draw_result_gauges(frame, chunks[3], state);

    let help = Paragraph::new("SPACE restart   |   q / Esc quit")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(help, chunks[5]);
}

fn draw_result_headline(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Overview");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (grade, grade_color) = overall_grade(state);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(10), Constraint::Min(20)])
        .split(inner);

    let grade_widget = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            grade,
            Style::default()
                .fg(grade_color)
                .add_modifier(Modifier::BOLD),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(grade_widget, cols[0]);

    let label_style = Style::default().fg(Color::Rgb(150, 155, 170));
    let value_style = Style::default()
        .fg(Color::Rgb(220, 224, 235))
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(vec![
            Span::styled("Scored taps  ", label_style),
            Span::styled(format!("{}", state.summary.scored_count), value_style),
        ]),
        Line::from(vec![
            Span::styled("Mean deviation  ", label_style),
            Span::styled(
                format!("{:+.1} ms", state.summary.mean_deviation_ms),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("Consistency (stddev)  ", label_style),
            Span::styled(format!("{:.1} ms", state.summary.std_dev_ms), value_style),
        ]),
        Line::from(vec![
            Span::styled("Rejected taps  ", label_style),
            Span::styled(format!("{}", state.rejected_taps), value_style),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), cols[1]);
}

/// A horizontal stacked bar showing the proportion of green/yellow/red
/// scored taps across the whole session — a quick "how consistent was I"
/// visual that a single mean/stddev pair doesn't convey on its own.
fn draw_result_distribution(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Accuracy Distribution");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height == 0 {
        return;
    }

    let total = state.green_count + state.yellow_count + state.red_count;
    if total == 0 {
        let hint = Paragraph::new("No scored taps this session")
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::DIM));
        frame.render_widget(hint, inner);
        return;
    }

    let width = inner.width as usize;
    let green_w = (state.green_count * width) / total;
    let yellow_w = (state.yellow_count * width) / total;
    // Remainder goes to red so rounding never leaves the bar short.
    let red_w = width.saturating_sub(green_w + yellow_w);

    let bar = Line::from(vec![
        Span::styled(
            "█".repeat(green_w),
            Style::default().fg(Color::Rgb(90, 200, 130)),
        ),
        Span::styled(
            "█".repeat(yellow_w),
            Style::default().fg(Color::Rgb(220, 190, 90)),
        ),
        Span::styled(
            "█".repeat(red_w),
            Style::default().fg(Color::Rgb(220, 100, 100)),
        ),
    ]);

    let legend = Line::from(vec![
        Span::styled("● ", Style::default().fg(Color::Rgb(90, 200, 130))),
        Span::raw(format!("{} on time   ", state.green_count)),
        Span::styled("● ", Style::default().fg(Color::Rgb(220, 190, 90))),
        Span::raw(format!("{} close   ", state.yellow_count)),
        Span::styled("● ", Style::default().fg(Color::Rgb(220, 100, 100))),
        Span::raw(format!("{} off", state.red_count)),
    ]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);
    frame.render_widget(Paragraph::new(bar), rows[0]);
    frame.render_widget(
        Paragraph::new(legend).alignment(Alignment::Center),
        rows[1],
    );
}

fn draw_result_gauges(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = panel_block("Scores");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let accuracy_ratio = score_ratio(state.summary.mean_deviation_ms.abs());
    let consistency_ratio = score_ratio(state.summary.std_dev_ms);

    render_score_gauge(frame, cols[0], "Accuracy", accuracy_ratio);
    render_score_gauge(frame, cols[1], "Consistency", consistency_ratio);
}

/// A single letter grade (S/A/B/C/D) plus color, derived from the same
/// accuracy/consistency ratios as the live gauges — gives the result
/// screen one headline number to anchor on rather than only raw ms stats.
fn overall_grade(state: &UiState) -> (&'static str, Color) {
    if state.summary.scored_count == 0 {
        return ("--", Color::Rgb(150, 155, 170));
    }
    let accuracy_ratio = score_ratio(state.summary.mean_deviation_ms.abs());
    let consistency_ratio = score_ratio(state.summary.std_dev_ms);
    let combined = (accuracy_ratio + consistency_ratio) / 2.0;

    match combined {
        r if r >= 0.9 => ("S", Color::Rgb(120, 190, 255)),
        r if r >= 0.75 => ("A", Color::Rgb(90, 200, 130)),
        r if r >= 0.55 => ("B", Color::Rgb(150, 210, 120)),
        r if r >= 0.35 => ("C", Color::Rgb(220, 190, 90)),
        _ => ("D", Color::Rgb(220, 100, 100)),
    }
}
