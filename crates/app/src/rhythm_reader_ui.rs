//! Terminal UI for Mode 4: Rhythm Reader (see `specs/keyboard-modes.md`).
//!
//! Renders the pattern as a horizontal row of note/rest symbols (no
//! staff/notehead rendering — a plain terminal has no good way to draw
//! actual notation, see the spec's rationale), a playhead that sweeps
//! through that row in real time, and colors each note green/red once
//! it's been resolved (hit within tolerance, or missed/mistapped).

use std::time::Duration;

use quietmytempo_core::{NoteValue, Pattern, PatternNote, TapResult};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::rhythm_reader_session::RhythmReaderSummary;

const NEUTRAL_COLOR: Color = Color::Rgb(150, 155, 170);
const HIT_COLOR: Color = Color::Rgb(90, 200, 130);
const MISS_COLOR: Color = Color::Rgb(220, 100, 100);
const REST_COLOR: Color = Color::Rgb(90, 96, 112);
const PLAYHEAD_COLOR: Color = Color::Rgb(120, 190, 255);

/// Per-note resolution state, tracked by index into the pattern so the UI
/// can color each symbol independently as results come in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteState {
    Pending,
    Hit,
    Missed,
}

pub struct RhythmReaderState {
    bpm: f64,
    pattern: Pattern,
    /// Cumulative start time (in beats from repetition start, i.e.
    /// including the count-in) of each pattern note — parallel array to
    /// `pattern`, precomputed once so the UI doesn't need `Schedule`
    /// knowledge.
    note_starts_beats: Vec<f64>,
    note_states: Vec<NoteState>,
    count_in_duration: Duration,
    rep_index: u32,
    total_reps: u32,
    elapsed: Duration,
    last_mistap_flash: Option<std::time::Instant>,
}

const MISTAP_FLASH_DURATION: Duration = Duration::from_millis(400);

impl RhythmReaderState {
    pub fn new(
        bpm: f64,
        pattern: Pattern,
        count_in_duration: Duration,
        rep_index: u32,
        total_reps: u32,
    ) -> Self {
        let mut cursor = 0.0;
        let mut note_starts_beats = Vec::with_capacity(pattern.len());
        for note in &pattern {
            note_starts_beats.push(cursor);
            cursor += note.value.duration_beats();
        }
        let note_states = vec![NoteState::Pending; pattern.len()];

        Self {
            bpm,
            pattern,
            note_starts_beats,
            note_states,
            count_in_duration,
            rep_index,
            total_reps,
            elapsed: Duration::ZERO,
            last_mistap_flash: None,
        }
    }

    pub fn update_playhead(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;
        // Any note whose expected time has fully passed without a hit
        // becomes "missed" for display purposes once its tolerance window
        // is long past — approximated here simply as "playhead moved past
        // the note's start". This is a display-only decision (the actual
        // missed-count for scoring comes from `rhythm_reader_session`'s
        // post-repetition diff against matched taps); marking it as
        // "missed" a beat later than its exact tolerance boundary is not
        // worth the extra bookkeeping just for a live-view color.
        let beat_duration = 60.0 / self.bpm;
        let elapsed_beats =
            (elapsed.saturating_sub(self.count_in_duration)).as_secs_f64() / beat_duration;
        for (idx, &start) in self.note_starts_beats.iter().enumerate() {
            if self.pattern[idx].is_rest {
                continue;
            }
            if self.note_states[idx] == NoteState::Pending && elapsed_beats > start + 0.5 {
                self.note_states[idx] = NoteState::Missed;
            }
        }
    }

    pub fn push_result(&mut self, result: TapResult) {
        // Find the pattern note whose expected time matches this result
        // (within a small epsilon) and mark it hit.
        let beat_duration = 60.0 / self.bpm;
        let expected_beats = (result.expected.at.saturating_sub(self.count_in_duration))
            .as_secs_f64()
            / beat_duration;
        if let Some(idx) = self
            .note_starts_beats
            .iter()
            .position(|&s| (s - expected_beats).abs() < 1e-3)
        {
            self.note_states[idx] = NoteState::Hit;
        }
    }

    pub fn push_mistap(&mut self) {
        self.last_mistap_flash = Some(std::time::Instant::now());
    }

    fn mistap_flashing(&self) -> bool {
        self.last_mistap_flash
            .is_some_and(|t| t.elapsed() < MISTAP_FLASH_DURATION)
    }

    /// Current playhead position expressed in beats from the start of the
    /// pattern (i.e. count-in already subtracted). Clamped to `0.0` at
    /// minimum by the caller ([`draw_pattern`]) since this can be
    /// negative during the count-in itself.
    fn playhead_beats(&self) -> f64 {
        let beat_duration = 60.0 / self.bpm;
        self.elapsed
            .saturating_sub(self.count_in_duration)
            .as_secs_f64()
            / beat_duration
    }

    fn in_count_in(&self) -> bool {
        self.elapsed < self.count_in_duration
    }
}

pub fn draw(frame: &mut Frame, state: &RhythmReaderState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // count-in / status
            Constraint::Min(7),    // pattern row + playhead
            Constraint::Length(1), // help
        ])
        .split(area);

    draw_header(frame, chunks[0], state);
    draw_status(frame, chunks[1], state);
    draw_pattern(frame, chunks[2], state);
    draw_help(frame, chunks[3]);
}

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

fn draw_header(frame: &mut Frame, area: Rect, state: &RhythmReaderState) {
    let label_style = Style::default().fg(Color::Rgb(150, 155, 170));
    let value_style = Style::default()
        .fg(Color::Rgb(220, 224, 235))
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::styled("Mode ", label_style),
        Span::styled("Rhythm Reader", value_style),
        Span::raw("   "),
        Span::styled("BPM ", label_style),
        Span::styled(format!("{:.0}", state.bpm), value_style),
        Span::raw("   "),
        Span::styled("Repetition ", label_style),
        Span::styled(
            format!("{}/{}", state.rep_index + 1, state.total_reps),
            value_style,
        ),
    ]);

    let paragraph = Paragraph::new(line).block(panel_block("Session"));
    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut Frame, area: Rect, state: &RhythmReaderState) {
    let text = if state.in_count_in() {
        Span::styled(
            "Count-in...",
            Style::default()
                .fg(PLAYHEAD_COLOR)
                .add_modifier(Modifier::BOLD),
        )
    } else if state.mistap_flashing() {
        Span::styled(
            "Mistap!",
            Style::default().fg(MISS_COLOR).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("Play the pattern", Style::default().fg(NEUTRAL_COLOR))
    };

    let paragraph = Paragraph::new(Line::from(text)).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Renders the pattern as a proportionally-widthed row of hand-built note
/// glyphs (see [`note_glyph_rows`]) — each spanning several character rows
/// (stem/flags/notehead) rather than a single ready-made Unicode note
/// symbol, so notes read clearly at a larger, more legible size in the
/// terminal — with a vertical playhead marker sweeping through it in real
/// time.
fn draw_pattern(frame: &mut Frame, area: Rect, state: &RhythmReaderState) {
    let block = panel_block("Pattern");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height < GLYPH_ROWS as u16 + 1 {
        return;
    }

    let total_beats: f64 = state.pattern.iter().map(|n| n.value.duration_beats()).sum();
    if total_beats <= 0.0 {
        return;
    }

    let width = inner.width as f64;

    // Vertically center the glyph block within the available inner area.
    let glyph_top_y = inner.y + (inner.height.saturating_sub(GLYPH_ROWS as u16)) / 2;

    // One row of spans per glyph row (stem/flags/notehead), built by
    // iterating notes outer, rows inner, so each note's glyph stays
    // together column-wise across all rows.
    let mut rows: Vec<Vec<Span>> = vec![Vec::new(); GLYPH_ROWS];

    for (idx, note) in state.pattern.iter().enumerate() {
        let note_width = ((note.value.duration_beats() / total_beats) * width).round() as usize;
        let note_width = note_width.max(GLYPH_WIDTH + 1);
        let glyph = note_glyph_rows(note);
        let color = match state.note_states[idx] {
            NoteState::Pending if note.is_rest => REST_COLOR,
            NoteState::Pending => NEUTRAL_COLOR,
            NoteState::Hit => HIT_COLOR,
            NoteState::Missed => MISS_COLOR,
        };
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let label_width = note_width.saturating_sub(1);

        for (row_idx, glyph_row) in glyph.iter().enumerate() {
            let label = format!("{glyph_row:^label_width$}");
            rows[row_idx].push(Span::styled(label, style));
            rows[row_idx].push(Span::raw("│"));
        }
    }

    for (row_idx, spans) in rows.into_iter().enumerate() {
        let line = Line::from(spans);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, glyph_top_y + row_idx as u16, inner.width, 1),
        );
    }

    // Playhead: only draw while inside the pattern itself (not during
    // count-in), as a vertical bar at the corresponding column.
    if !state.in_count_in() {
        let playhead_beats = state.playhead_beats().clamp(0.0, total_beats);
        let col = ((playhead_beats / total_beats) * width).round() as u16;
        let col = col.min(inner.width.saturating_sub(1));
        let marker_area = Rect::new(inner.x + col, inner.y, 1, inner.height);
        let marker_lines: Vec<Line> = (0..inner.height).map(|_| Line::from("▏")).collect();
        frame.render_widget(
            Paragraph::new(marker_lines).style(Style::default().fg(PLAYHEAD_COLOR)),
            marker_area,
        );
    }
}

/// Number of character rows each note glyph occupies (see
/// [`note_glyph_rows`]) — flag row, second flag row, stem row, notehead
/// row, top to bottom.
const GLYPH_ROWS: usize = 4;
/// Character width of each note glyph, before centering it in its
/// duration-proportional slot.
const GLYPH_WIDTH: usize = 2;

/// Builds a note/rest as hand-drawn ASCII art spanning [`GLYPH_ROWS`] rows
/// of [`GLYPH_WIDTH`] characters each (top to bottom: upper flag, lower
/// flag, stem, notehead/dot) instead of reaching for a single premade
/// Unicode musical symbol (♩/♪/𝅘𝅥𝅯/𝄽/…) — at normal terminal font sizes those
/// render tiny and are hard to tell apart at a glance, whereas a note built
/// from several rows of stem/flag/notehead characters reads clearly larger
/// and closer to how the shape is actually drawn on paper.
///
/// Layout (all notes/rests share the same 4-row canvas so columns line up
/// across the whole pattern row):
/// - Row 0/1: flags — sixteenth notes get one flag on each of these rows
///   (`│╮`), eighth notes get a single flag on row 1 only, quarters/dotted
///   quarters have no flag (bare stem)
/// - Row 2: stem (`│`) for any note; blank for rests
/// - Row 3: notehead (`●`) for notes, plus a trailing `.` for dotted
///   values; rests use a distinct mark on this row instead (no notehead)
///
/// Rests reuse the same canvas but replace the stem/notehead with a small
/// zigzag/dot glyph (loosely echoing the real rest glyphs' shapes) so they
/// are unmistakably not notes at a glance.
fn note_glyph_rows(note: &PatternNote) -> [&'static str; GLYPH_ROWS] {
    use NoteValue::*;
    match (note.value, note.is_rest) {
        (Quarter, false) => ["  ", "│ ", "│ ", "● "],
        (Eighth, false) => ["  ", "│╮", "│ ", "● "],
        (Sixteenth, false) => ["│╮", "│╮", "│ ", "● "],
        (DottedQuarter, false) => ["  ", "│ ", "│ ", "●."],
        (Quarter, true) => [" ╱", "╱ ", " ╲", "╲ "],
        (Eighth, true) => ["  ", " ●", "╱ ", "  "],
        (Sixteenth, true) => [" ●", " ●", "╱ ", "  "],
        (DottedQuarter, true) => [" ╱", "╱ ", " ╲", "╲."],
    }
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
        Span::raw(" end session "),
    ]);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Result screen shown after the configured number of repetitions has
/// completed (see `specs/keyboard-modes.md` Mode 4 "Result Screen").
pub fn draw_result(frame: &mut Frame, summary: &RhythmReaderSummary) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(8), // stats
            Constraint::Min(1),    // spacer
            Constraint::Length(1), // help
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

    draw_result_stats(frame, chunks[1], summary);

    let help = Paragraph::new("SPACE restart   |   m / q / Esc menu")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(help, chunks[3]);
}

fn draw_result_stats(frame: &mut Frame, area: Rect, summary: &RhythmReaderSummary) {
    let block = panel_block("Overview");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let label_style = Style::default().fg(Color::Rgb(150, 155, 170));
    let value_style = Style::default()
        .fg(Color::Rgb(220, 224, 235))
        .add_modifier(Modifier::BOLD);

    let scored: Vec<f64> = summary.all_results.iter().map(|r| r.deviation_ms).collect();
    let mean = if scored.is_empty() {
        0.0
    } else {
        scored.iter().sum::<f64>() / scored.len() as f64
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Hit rate  ", label_style),
            Span::styled(format!("{:.0}%", summary.hit_rate() * 100.0), value_style),
        ]),
        Line::from(vec![
            Span::styled("Notes hit  ", label_style),
            Span::styled(
                format!(
                    "{}/{}",
                    summary.total_expected_notes - summary.missed_notes,
                    summary.total_expected_notes
                ),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("Notes missed  ", label_style),
            Span::styled(format!("{}", summary.missed_notes), value_style),
        ]),
        Line::from(vec![
            Span::styled("Mistaps  ", label_style),
            Span::styled(format!("{}", summary.mistaps), value_style),
        ]),
        Line::from(vec![
            Span::styled("Mean deviation  ", label_style),
            Span::styled(format!("{mean:+.1} ms"), value_style),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
