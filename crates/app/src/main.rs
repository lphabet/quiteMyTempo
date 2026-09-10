//! quietMyTempo — Tap Along MVP.
//!
//! Minimal terminal prototype exercising the full pipeline described in
//! `specs/architecture.md` and `specs/keyboard-modes.md`:
//!   visual latency calibration -> metronome audio (cpal) ->
//!   KeyboardSource (crossterm) -> TimingEvaluator -> ratatui live UI
//!   (flashing metronome panel + tap history + stats).

mod calibration;
mod clock;
mod keyboard;
mod metronome;
mod terminal;
mod ui;

use std::time::{Duration, Instant};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use quietmytempo_core::{Schedule, TapAlongSchedule, TimingEvaluator};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use calibration::CalibrationOutcome;
use clock::SessionClock;
use keyboard::KeyboardSignal;
use metronome::Metronome;
use ui::UiState;

/// How long the pre-generated click/expected-tap grid covers. A live
/// session longer than this would simply run out of expected taps to match
/// against — fine for an MVP; a real session-length picker is a later
/// product decision, not a core-architecture one.
const SESSION_DURATION: Duration = Duration::from_secs(10 * 60);
const BPM: f64 = 90.0;
/// Beats per bar for the position indicator (see `ui::draw_position`).
/// Standard 4/4 time — not yet configurable, matches the "4/4 as default"
/// assumption throughout `specs/keyboard-modes.md`.
const BEATS_PER_BAR: usize = 4;
/// UI redraw cadence. Independent of audio/input timing precision (those
/// are timestamped against `SessionClock`, not tied to the render rate).
const FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS

fn main() -> anyhow::Result<()> {
    // Terminal/alternate-screen setup happens up front now, since the
    // visual calibration step (converging bars) needs a render loop of
    // its own, before the metronome/session even starts.
    let _raw_mode = terminal::RawModeGuard::enable()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    // Single process-wide clock: the keyboard thread timestamps every tap
    // against this one clock (see `clock.rs` doc comment for why this
    // matters — each phase below rebases these timestamps into its own
    // local zero-based time domain via `SessionClock::rebase`).
    let global_clock = SessionClock::start_now();
    let rx = keyboard::spawn(global_clock);

    let outcome = calibration::run(&mut term, &rx, global_clock)?;
    let offset_ms = match outcome {
        CalibrationOutcome::Measured { offset_ms } => offset_ms,
        CalibrationOutcome::Aborted => {
            restore_terminal(_raw_mode)?;
            return Ok(());
        }
    };

    // Calibration only runs once; from here the player can restart the
    // Tap Along exercise itself (SPACE on the result screen) as many
    // times as they like without having to recalibrate each time.
    let state = loop {
        let state = run_session(&mut term, &rx, global_clock, offset_ms)?;

        match show_result_screen(&mut term, &rx, &state)? {
            ResultScreenOutcome::Restart => continue,
            ResultScreenOutcome::Quit => break state,
        }
    };

    restore_terminal(_raw_mode)?;

    println!("\n--- Session Summary ---");
    println!("Latency offset applied: {offset_ms} ms");
    println!("Scored taps: {}", state.summary.scored_count);
    if state.summary.scored_count > 0 {
        println!("Mean deviation: {:+.1} ms", state.summary.mean_deviation_ms);
        println!("Std deviation (consistency): {:.1} ms", state.summary.std_dev_ms);
    }
    println!("Rejected/out-of-tolerance taps: {}", state.rejected_taps);
    Ok(())
}

fn restore_terminal(raw_mode: terminal::RawModeGuard) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    drop(raw_mode);
    Ok(())
}

/// What the player chose to do from the end-of-session result screen.
enum ResultScreenOutcome {
    /// SPACE — run the exercise again without repeating calibration.
    Restart,
    /// q / Esc / Ctrl+C, or the input thread died — leave the app.
    Quit,
}

/// Shows the end-of-session result screen (see `ui::draw_result`) and
/// waits for the player to press SPACE (restart) or q/Esc (quit) before
/// returning, so the player gets a clear takeaway instead of the terminal
/// just snapping back to the shell the instant the session ends.
fn show_result_screen(
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    state: &UiState,
) -> anyhow::Result<ResultScreenOutcome> {
    loop {
        match rx.try_recv() {
            Ok(KeyboardSignal::Tap(_)) => return Ok(ResultScreenOutcome::Restart),
            Ok(KeyboardSignal::Quit) => return Ok(ResultScreenOutcome::Quit),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Ok(ResultScreenOutcome::Quit)
            }
        }

        term.draw(|frame| ui::draw_result(frame, state))?;
        std::thread::sleep(FRAME_INTERVAL);
    }
}

/// Runs the actual Tap Along exercise: starts the metronome on a fresh
/// `SessionClock` (deliberately separate from calibration's clocks — the
/// metronome's click schedule must start counting from t=0 exactly when
/// audio playback begins, not from whenever calibration happened to
/// start), then drives the live UI loop until the player quits.
fn run_session(
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
) -> anyhow::Result<UiState> {
    let schedule = TapAlongSchedule::new(BPM);
    let clicks = schedule.clicks(SESSION_DURATION);
    let expected_taps = schedule.expected_taps(SESSION_DURATION);

    // Matching tolerance: half a beat. A tap further than this from the
    // nearest unmatched expected tap is rejected rather than incorrectly
    // consuming a distant slot (see doc comment on `TimingEvaluator`) —
    // this is the fix for the double-tap-derails-everything bug found in
    // manual testing.
    let beat_duration = Duration::from_secs_f64(60.0 / BPM);
    let tolerance = beat_duration / 2;

    let clock = SessionClock::start_now();
    let (_metronome, latency_estimate) = Metronome::play(clicks.clone(), clock)?;
    // Give the audio backend a moment to run its first callback so the
    // estimate has a real value (falls back to 0 if never reported).
    std::thread::sleep(Duration::from_millis(50));

    let mut evaluator = TimingEvaluator::new(expected_taps, tolerance);
    let mut state = UiState::new(BPM, "Tap Along");
    state.latency_offset_ms = offset_ms;
    state.device_latency_ms = latency_estimate.get().as_millis() as i64;
    let mut all_results: Vec<quietmytempo_core::TapResult> = Vec::new();

    run_loop(RunLoopArgs {
        term,
        rx,
        evaluator: &mut evaluator,
        state: &mut state,
        all_results: &mut all_results,
        clicks: &clicks,
        clock,
        global_clock,
        offset_ms,
    })?;

    Ok(state)
}

/// Bundles the arguments `run_loop` needs, purely to keep the function
/// signature within clippy's default arity limit — no behavioral meaning
/// beyond that.
struct RunLoopArgs<'a> {
    term: &'a mut Terminal<CrosstermBackend<std::io::Stdout>>,
    rx: &'a std::sync::mpsc::Receiver<KeyboardSignal>,
    evaluator: &'a mut TimingEvaluator,
    state: &'a mut UiState,
    all_results: &'a mut Vec<quietmytempo_core::TapResult>,
    clicks: &'a [quietmytempo_core::ClickEvent],
    clock: SessionClock,
    global_clock: SessionClock,
    offset_ms: i64,
}

fn run_loop(args: RunLoopArgs) -> anyhow::Result<()> {
    let RunLoopArgs {
        term,
        rx,
        evaluator,
        state,
        all_results,
        clicks,
        clock,
        global_clock,
        offset_ms,
    } = args;

    loop {
        // Drain all pending keyboard signals since the last frame without
        // blocking the render loop.
        loop {
            match rx.try_recv() {
                Ok(KeyboardSignal::Tap(event)) => {
                    // `event.at` is in the global clock's time domain
                    // (see `keyboard::spawn`); rebase into this session's
                    // own local (metronome-relative) time domain before
                    // applying the calibration offset and evaluating.
                    let session_relative_at = clock.rebase(global_clock, event.at);
                    let adjusted_at = calibration::apply_offset(session_relative_at, offset_ms);
                    let adjusted_event = quietmytempo_core::TimingEvent { at: adjusted_at };
                    match evaluator.record(adjusted_event) {
                        Some(result) => {
                            all_results.push(result);
                            state.push_result(result);
                        }
                        None => state.rejected_taps += 1,
                    }
                }
                Ok(KeyboardSignal::Quit) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }

        state.summary = TimingEvaluator::summarize(all_results);
        state.last_click_at = most_recent_click_instant(clicks, clock);
        update_position(state, clock, BEATS_PER_BAR);

        term.draw(|frame| ui::draw(frame, state))?;
        std::thread::sleep(FRAME_INTERVAL);
    }
}

/// Computes 1-based bar/beat-in-bar numbers from elapsed session time and
/// tempo, purely for the "where am I in the 4/4 grid" display aid (see
/// `ui::draw_position`) — not used for any scoring logic.
fn update_position(state: &mut UiState, clock: SessionClock, beats_per_bar: usize) {
    let beat_duration = Duration::from_secs_f64(60.0 / state.bpm);
    let beats_elapsed = (clock.elapsed().as_secs_f64() / beat_duration.as_secs_f64()).floor() as usize;
    state.beats_per_bar = beats_per_bar;
    state.current_bar = beats_elapsed / beats_per_bar + 1;
    state.current_beat_in_bar = beats_elapsed % beats_per_bar + 1;
}

/// Finds the most recently-elapsed click and converts it to an `Instant` so
/// the UI's flash countdown (`UiState::is_flashing`, driven by
/// `Instant::elapsed()`) can use it directly, without the UI module needing
/// to know anything about `SessionClock` or the click schedule itself.
fn most_recent_click_instant(
    clicks: &[quietmytempo_core::ClickEvent],
    clock: SessionClock,
) -> Option<Instant> {
    let now_elapsed = clock.elapsed();
    let last = clicks.iter().rev().find(|c| c.at <= now_elapsed)?;
    let since_click = now_elapsed - last.at;
    Some(Instant::now() - since_click)
}
