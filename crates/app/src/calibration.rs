//! Audio/visual/input latency calibration.
//!
//! Audio output hardware always has some buffering delay between "sample
//! written in the callback" and "sample audible at the speaker" (typically
//! 10-100+ ms). This is not really "fixable" in the sense of making it
//! zero — every rhythm game (osu!, Guitar Hero, Beat Saber, ...) instead
//! *measures* the delay and *compensates* for it.
//!
//! This calibration step measures the player's *visual* reaction/timing
//! precision: two bars slide inward from the left/right screen edges and
//! meet in the center; the player presses SPACE at the moment they meet.
//! Deliberately **no audio** here (per product decision) — this isolates
//! visual-perception + input latency from the audio-output-latency
//! question entirely. The two are conceptually different offsets:
//!
//!   - visual/input round-trip (measured here)
//!   - audio output latency (see [`crate::metronome::LatencyEstimate`],
//!     shown informationally elsewhere, not folded into this offset)
//!
//! For v1 we apply only the visual/input offset to keyboard timestamps,
//! since the *scored* exercise (Tap Along) is itself audio-driven — this
//! keeps the calibration honest about what it actually measures rather
//! than conflating two different latency sources into one number.
//!
//! Bar speed is deliberately slower than the session BPM (product
//! decision) — calibration is about steady, unhurried visual precision,
//! not about matching the exercise tempo.

use std::io::Stdout;
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use crate::clock::SessionClock;
use crate::keyboard::KeyboardSignal;

/// Number of converge-and-tap rounds used to estimate the offset. Enough
/// to average out one-off timing noise, short enough not to feel tedious.
pub const CALIBRATION_ROUNDS: usize = 5;

/// How long one full inward slide takes (edge -> center). Slower than the
/// session tempo on purpose (product decision) — calibration prioritizes
/// steady precision over speed.
const CONVERGE_DURATION: Duration = Duration::from_millis(1800);
/// Pause between rounds so the player has a clear "reset" moment.
const PAUSE_BETWEEN_ROUNDS: Duration = Duration::from_millis(500);
/// How long the center "MISS" / result flash stays visible after a round.
const RESULT_FLASH_DURATION: Duration = Duration::from_millis(600);

const FRAME_INTERVAL: Duration = Duration::from_millis(16); // ~60 FPS, smooth bar motion

pub enum CalibrationOutcome {
    /// Player completed all rounds; carries the estimated offset to apply
    /// to keyboard timestamps (see [`apply_offset`]).
    Measured { offset_ms: i64 },
    /// Player quit during calibration.
    Aborted,
}

type Term = Terminal<CrosstermBackend<Stdout>>;

/// Runs the full visual calibration UI loop (bars converging, tap capture,
/// per-round result flash) followed by a "press SPACE/Enter to continue"
/// confirmation screen showing the measured offset.
///
/// Must be called with the terminal already in raw mode + alternate screen
/// (see `crate::terminal`), matching how the main session loop expects the
/// terminal to be set up.
pub fn run(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
) -> anyhow::Result<CalibrationOutcome> {
    let mut deviations_ms: Vec<f64> = Vec::with_capacity(CALIBRATION_ROUNDS);

    for round in 0..CALIBRATION_ROUNDS {
        match run_round(term, rx, global_clock, round)? {
            RoundOutcome::Tapped { deviation_ms } => deviations_ms.push(deviation_ms),
            RoundOutcome::Aborted => return Ok(CalibrationOutcome::Aborted),
        }
        std::thread::sleep(PAUSE_BETWEEN_ROUNDS);
    }

    let mean = deviations_ms.iter().sum::<f64>() / deviations_ms.len() as f64;
    let offset_ms = mean.round() as i64;

    if !show_start_screen(term, rx, offset_ms)? {
        return Ok(CalibrationOutcome::Aborted);
    }

    Ok(CalibrationOutcome::Measured { offset_ms })
}

enum RoundOutcome {
    Tapped { deviation_ms: f64 },
    Aborted,
}

/// Runs a single converge-and-tap round: bars slide inward starting fresh
/// from this round's own clock (so drift across rounds can't accumulate),
/// player taps once, we measure how far from "bars exactly met" the tap
/// landed.
fn run_round(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    round_index: usize,
) -> anyhow::Result<RoundOutcome> {
    let round_clock = SessionClock::start_now();
    let mut tapped: Option<(f64, Duration)> = None; // (deviation_ms, elapsed_at_tap)

    loop {
        let elapsed = round_clock.elapsed();

        // Drain any pending input without blocking rendering.
        match rx.try_recv() {
            Ok(KeyboardSignal::Tap(event)) if tapped.is_none() => {
                // `event.at` is in the global clock's domain (see
                // `keyboard::spawn`); rebase into this round's own local
                // time domain (started fresh above) before comparing
                // against `CONVERGE_DURATION`.
                let round_relative_at = round_clock.rebase(global_clock, event.at);
                // Deviation from the moment the bars are exactly meeting
                // (round_relative_at == CONVERGE_DURATION), in ms.
                // Negative = tapped too early, positive = too late.
                let deviation_ms = round_relative_at.as_secs_f64() * 1000.0
                    - CONVERGE_DURATION.as_secs_f64() * 1000.0;
                tapped = Some((deviation_ms, elapsed));
            }
            Ok(KeyboardSignal::Tap(_)) => {} // already tapped this round, ignore extras
            Ok(KeyboardSignal::Quit) => return Ok(RoundOutcome::Aborted),
            Ok(
                KeyboardSignal::Up
                | KeyboardSignal::Down
                | KeyboardSignal::Select
                | KeyboardSignal::BackToMenu
                | KeyboardSignal::Shortcut(_),
            ) => {} // no menu-navigation meaning during calibration
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(RoundOutcome::Aborted),
        }

        let tapped_deviation_ms = tapped.map(|(d, _)| d);
        term.draw(|frame| draw_converge_scene(frame, elapsed, round_index, tapped_deviation_ms))?;

        // End the round once the result flash has had time to display
        // after a tap, or if the bars have long since met without a tap
        // (give a generous grace window rather than hanging forever).
        if let Some((deviation_ms, elapsed_at_tap)) = tapped {
            if elapsed.saturating_sub(elapsed_at_tap) >= RESULT_FLASH_DURATION {
                return Ok(RoundOutcome::Tapped { deviation_ms });
            }
        } else if elapsed > CONVERGE_DURATION * 2 {
            // No tap at all within a generous window: record a large
            // penalty deviation rather than hanging the calibration
            // forever on a missed round.
            return Ok(RoundOutcome::Tapped {
                deviation_ms: CONVERGE_DURATION.as_secs_f64() * 1000.0,
            });
        }

        std::thread::sleep(FRAME_INTERVAL);
    }
}

fn draw_converge_scene(
    frame: &mut ratatui::Frame,
    elapsed: Duration,
    round_index: usize,
    tapped_deviation_ms: Option<f64>,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // converge scene
            Constraint::Length(1), // help
        ])
        .split(area);

    draw_calibration_header(frame, chunks[0], round_index);
    draw_bars(frame, chunks[1], elapsed, tapped_deviation_ms);

    let help = Paragraph::new("Press SPACE the instant the bars meet")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(help, chunks[2]);
}

fn draw_calibration_header(frame: &mut ratatui::Frame, area: Rect, round_index: usize) {
    let text = format!(
        "Latency Calibration — round {}/{}",
        round_index + 1,
        CALIBRATION_ROUNDS
    );
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

/// Renders the two bars sliding in from the left/right edges toward the
/// center, plus a result flash (HIT/label + ms) once tapped.
fn draw_bars(
    frame: &mut ratatui::Frame,
    area: Rect,
    elapsed: Duration,
    tapped_deviation_ms: Option<f64>,
) {
    let block = Block::default().borders(Borders::ALL).title(" Converge ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height == 0 {
        return;
    }

    // Progress 0.0 (at edges) -> 1.0 (meeting in center), clamped so bars
    // don't overshoot past the midpoint even if rendering runs a little
    // past CONVERGE_DURATION before the round ends.
    let progress = (elapsed.as_secs_f64() / CONVERGE_DURATION.as_secs_f64()).clamp(0.0, 1.0);

    let half_width = inner.width / 2;
    // How far each bar has traveled from its edge, in columns.
    let traveled = (progress * half_width as f64).round() as u16;

    let mid_row = inner.y + inner.height / 2;
    let mut line = vec![' '; inner.width as usize];

    // Left bar grows from the left edge inward; right bar from the right
    // edge inward. Using a filled run (not just a single leading edge)
    // reads more clearly as "a bar" than a single moving character.
    for i in 0..traveled.min(half_width) {
        line[i as usize] = '#';
        let right_idx = inner.width as usize - 1 - i as usize;
        line[right_idx] = '#';
    }

    let mut spans: Vec<Span> = Vec::new();
    let color = match tapped_deviation_ms {
        Some(d) if d.abs() <= 50.0 => Color::Green,
        Some(d) if d.abs() <= 150.0 => Color::Yellow,
        Some(_) => Color::Red,
        None => Color::Cyan,
    };
    spans.push(Span::styled(
        line.into_iter().collect::<String>(),
        Style::default().fg(color),
    ));

    let bar_line = Line::from(spans);
    frame.render_widget(
        Paragraph::new(bar_line),
        Rect::new(inner.x, mid_row, inner.width, 1),
    );

    if let Some(deviation_ms) = tapped_deviation_ms {
        let label = format!("{:+.0} ms", deviation_ms);
        let label_area = Rect::new(
            inner.x,
            mid_row.saturating_sub(2).max(inner.y),
            inner.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
            label_area,
        );
    }
}

/// Shows the measured offset and waits for SPACE/Enter to confirm and
/// return to the main menu (or Quit). Returns `Ok(true)` to proceed,
/// `Ok(false)` if the player quit instead.
fn show_start_screen(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    offset_ms: i64,
) -> anyhow::Result<bool> {
    loop {
        match rx.try_recv() {
            Ok(KeyboardSignal::Tap(_)) => return Ok(true),
            Ok(KeyboardSignal::Quit) => return Ok(false),
            Ok(
                KeyboardSignal::Up
                | KeyboardSignal::Down
                | KeyboardSignal::Select
                | KeyboardSignal::BackToMenu
                | KeyboardSignal::Shortcut(_),
            ) => {}
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(false),
        }

        term.draw(|frame| {
            let area = frame.area();
            let text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Calibration complete",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("Measured visual/input offset: {offset_ms} ms")),
                Line::from(""),
                Line::from(Span::styled(
                    "Press SPACE to continue",
                    Style::default().fg(Color::Green),
                )),
                Line::from("(q / Esc to quit)"),
            ];
            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(" Ready "));
            frame.render_widget(paragraph, area);
        })?;

        std::thread::sleep(FRAME_INTERVAL);
    }
}

/// Compensates a raw tap timestamp using a previously measured
/// `offset_ms` (see [`CalibrationOutcome::Measured`]).
///
/// Sign convention: `offset_ms` is the mean signed deviation measured
/// during calibration (positive = taps arrived late relative to the
/// target moment). To realign future taps with the logical click
/// schedule, we subtract that offset.
pub fn apply_offset(at: Duration, offset_ms: i64) -> Duration {
    if offset_ms >= 0 {
        at.saturating_sub(Duration::from_millis(offset_ms as u64))
    } else {
        at + Duration::from_millis((-offset_ms) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_offset_shifts_timestamp_earlier() {
        let at = Duration::from_millis(500);
        let adjusted = apply_offset(at, 50);
        assert_eq!(adjusted, Duration::from_millis(450));
    }

    #[test]
    fn negative_offset_shifts_timestamp_later() {
        let at = Duration::from_millis(500);
        let adjusted = apply_offset(at, -50);
        assert_eq!(adjusted, Duration::from_millis(550));
    }

    #[test]
    fn offset_larger_than_timestamp_saturates_at_zero() {
        let at = Duration::from_millis(10);
        let adjusted = apply_offset(at, 50);
        assert_eq!(adjusted, Duration::ZERO);
    }
}
