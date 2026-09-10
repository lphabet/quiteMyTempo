//! Generic live-session runner shared by the three "continuous pulse"
//! modes (Tap Along, Quiet Four, Tuplets) — see `specs/keyboard-modes.md`.
//!
//! All three share the exact same wiring: a `Schedule` produces clicks +
//! expected taps up front, `Metronome::play` renders the clicks as audio,
//! keyboard taps get rebased/offset-corrected and fed into a
//! `TimingEvaluator`, and `ui::draw`/`ui::draw_result` render the live/
//! result screens. They only differ in *which* `Schedule` is used and
//! what tolerance/label to use — captured here as a small
//! [`SessionConfig`] rather than duplicating the loop three times in
//! `main.rs` (which is what an earlier version of this app did before
//! Quiet Four/Tuplets/Rhythm Reader were wired up, see
//! `specs/keyboard-modes.md`'s "Implementierungsstatus").
//!
//! Rhythm Reader (Mode 4) is deliberately **not** run through this module
//! — its per-repetition-with-count-in structure and note-based UI differ
//! enough that forcing it into this shape would obscure more than it
//! shares (see `rhythm_reader_session.rs`).

use std::time::{Duration, Instant};

use quietmytempo_core::{ClickEvent, Schedule, TimingEvaluator};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::clock::SessionClock;
use crate::keyboard::KeyboardSignal;
use crate::metronome::Metronome;
use crate::ui::{self, UiState};

/// How long the pre-generated click/expected-tap grid covers. A live
/// session longer than this would simply run out of expected taps to
/// match against — fine for an MVP; a real session-length picker is a
/// later product decision, not a core-architecture one.
const SESSION_DURATION: Duration = Duration::from_secs(10 * 60);
/// UI redraw cadence. Independent of audio/input timing precision (those
/// are timestamped against `SessionClock`, not tied to the render rate).
const FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

/// What a session ended with, so the caller (`main.rs`) knows whether to
/// go back to the menu or quit the whole app. Restarting (SPACE on the
/// result screen) is handled entirely inside [`run`]'s loop and never
/// surfaces to the caller as its own outcome.
pub enum SessionOutcome {
    /// Player wants to return to the main menu (`m` on the result
    /// screen).
    BackToMenu,
    /// Player quit the whole app (q/Esc, or the input thread died).
    Quit,
}

/// Everything a `Schedule`-driven continuous-pulse mode needs beyond the
/// schedule itself.
pub struct SessionConfig<S: Schedule> {
    pub schedule: S,
    pub bpm: f64,
    pub mode_name: &'static str,
    pub beats_per_bar: usize,
    /// Matching tolerance for `TimingEvaluator` — see doc comment on
    /// `TimingEvaluator::new` for why this should scale with tempo rather
    /// than be a fixed ms constant.
    pub tolerance: Duration,
}

/// Runs one full mode session (live loop) followed by the result screen,
/// looping on "restart" until the player backs out to the menu or quits.
/// Returns once the player has left this mode (menu/quit); the returned
/// [`SessionOutcome`] tells `main.rs` where to go next.
pub fn run<S: Schedule + Clone>(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    config: &SessionConfig<S>,
) -> anyhow::Result<SessionOutcome> {
    loop {
        let state = run_live(term, rx, global_clock, offset_ms, config)?;
        match state {
            LiveOutcome::Finished(ui_state) => match show_result_screen(term, rx, &ui_state)? {
                ResultScreenOutcome::Restart => continue,
                ResultScreenOutcome::BackToMenu => return Ok(SessionOutcome::BackToMenu),
                ResultScreenOutcome::Quit => return Ok(SessionOutcome::Quit),
            },
            LiveOutcome::Quit => return Ok(SessionOutcome::Quit),
        }
    }
}

enum LiveOutcome {
    Finished(UiState),
    Quit,
}

fn run_live<S: Schedule>(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    config: &SessionConfig<S>,
) -> anyhow::Result<LiveOutcome> {
    let clicks = config.schedule.clicks(SESSION_DURATION);
    let expected_taps = config.schedule.expected_taps(SESSION_DURATION);

    let clock = SessionClock::start_now();
    let (_metronome, latency_estimate) = Metronome::play(clicks.clone(), clock)?;
    // Give the audio backend a moment to run its first callback so the
    // estimate has a real value (falls back to 0 if never reported).
    std::thread::sleep(Duration::from_millis(50));

    let mut evaluator = TimingEvaluator::new(expected_taps, config.tolerance);
    let mut state = UiState::new(config.bpm, config.mode_name);
    state.latency_offset_ms = offset_ms;
    state.device_latency_ms = latency_estimate.get().as_millis() as i64;
    let mut all_results: Vec<quietmytempo_core::TapResult> = Vec::new();

    loop {
        loop {
            match rx.try_recv() {
                Ok(KeyboardSignal::Tap(event)) => {
                    let session_relative_at = clock.rebase(global_clock, event.at);
                    let adjusted_at =
                        crate::calibration::apply_offset(session_relative_at, offset_ms);
                    let adjusted_event = quietmytempo_core::TimingEvent { at: adjusted_at };
                    match evaluator.record(adjusted_event) {
                        Some(result) => {
                            all_results.push(result);
                            state.push_result(result);
                        }
                        None => state.rejected_taps += 1,
                    }
                }
                Ok(KeyboardSignal::Quit) => return Ok(LiveOutcome::Finished(state)),
                Ok(
                    KeyboardSignal::Up
                    | KeyboardSignal::Down
                    | KeyboardSignal::Select
                    | KeyboardSignal::BackToMenu,
                ) => {} // no menu navigation meaning during a live session
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(LiveOutcome::Quit),
            }
        }

        state.summary = TimingEvaluator::summarize(&all_results);
        state.last_click_at = most_recent_click_instant(&clicks, clock);
        update_position(&mut state, clock, config.beats_per_bar);

        term.draw(|frame| ui::draw(frame, &state))?;

        if clock.elapsed() >= SESSION_DURATION {
            return Ok(LiveOutcome::Finished(state));
        }

        std::thread::sleep(FRAME_INTERVAL);
    }
}

enum ResultScreenOutcome {
    Restart,
    BackToMenu,
    Quit,
}

/// Shows the end-of-session result screen (see `ui::draw_result`) and
/// waits for the player to choose restart / back-to-menu / quit.
fn show_result_screen(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    state: &UiState,
) -> anyhow::Result<ResultScreenOutcome> {
    loop {
        match rx.try_recv() {
            Ok(KeyboardSignal::Tap(_)) | Ok(KeyboardSignal::Select) => {
                return Ok(ResultScreenOutcome::Restart)
            }
            Ok(KeyboardSignal::BackToMenu) => return Ok(ResultScreenOutcome::BackToMenu),
            Ok(KeyboardSignal::Quit) => return Ok(ResultScreenOutcome::Quit),
            Ok(KeyboardSignal::Up | KeyboardSignal::Down) => {}
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Ok(ResultScreenOutcome::Quit)
            }
        }

        term.draw(|frame| ui::draw_result(frame, state))?;
        std::thread::sleep(FRAME_INTERVAL);
    }
}

/// Computes 1-based bar/beat-in-bar numbers from elapsed session time and
/// tempo, purely for the "where am I in the 4/4 grid" display aid (see
/// `ui::draw_position`) — not used for any scoring logic.
fn update_position(state: &mut UiState, clock: SessionClock, beats_per_bar: usize) {
    let beat_duration = Duration::from_secs_f64(60.0 / state.bpm);
    let beats_elapsed =
        (clock.elapsed().as_secs_f64() / beat_duration.as_secs_f64()).floor() as usize;
    state.beats_per_bar = beats_per_bar;
    state.current_bar = beats_elapsed / beats_per_bar + 1;
    state.current_beat_in_bar = beats_elapsed % beats_per_bar + 1;
}

/// Finds the most recently-elapsed click and converts it to an `Instant`
/// so the UI's flash countdown (`UiState::is_flashing`, driven by
/// `Instant::elapsed()`) can use it directly, without the UI module
/// needing to know anything about `SessionClock` or the click schedule
/// itself.
fn most_recent_click_instant(clicks: &[ClickEvent], clock: SessionClock) -> Option<Instant> {
    let now_elapsed = clock.elapsed();
    let last = clicks.iter().rev().find(|c| c.at <= now_elapsed)?;
    let since_click = now_elapsed - last.at;
    Some(Instant::now() - since_click)
}
