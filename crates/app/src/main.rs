//! quietMyTempo — terminal rhythm-timing trainer.
//!
//! Top-level flow (see `specs/app-flow.md`):
//!   load persisted calibration -> main menu (modes locked until
//!   calibrated) -> calibration flow | mode session -> back to menu | quit.
//!
//! Each mode's actual session loop lives in `session.rs` (Tap Along, Quiet
//! Four, Tuplets — all driven by the same `Schedule`-based pipeline) or
//! `rhythm_reader_session.rs` (Rhythm Reader, structurally different
//! enough to warrant its own runner — see that module's doc comment).

mod calibration;
mod clock;
mod keyboard;
mod menu;
mod metronome;
mod rhythm_reader_session;
mod rhythm_reader_ui;
mod session;
mod settings;
mod terminal;
mod ui;

use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use quietmytempo_core::{QuietFourSchedule, TapAlongSchedule, TupletSchedule};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use calibration::CalibrationOutcome;
use clock::SessionClock;
use keyboard::KeyboardSignal;
use menu::{MenuEntry, MenuOutcome};
use rhythm_reader_session::RhythmReaderConfig;
use session::{SessionConfig, SessionOutcome};

/// Beats per bar for the position indicator (see `ui::draw_position`).
/// Standard 4/4 time — not yet configurable, matches the "4/4 as default"
/// assumption throughout `specs/keyboard-modes.md`.
const BEATS_PER_BAR: usize = 4;
const DEFAULT_BPM: f64 = 90.0;
const RHYTHM_READER_BPM: f64 = 80.0;
const RHYTHM_READER_REPETITIONS: u32 = 5;
const QUIET_FOUR_CYCLES: u32 = 20;
const TUPLET_BARS: u32 = 40;
const TUPLET_SUBDIVISIONS: u32 = 3;

fn main() -> anyhow::Result<()> {
    // Terminal/alternate-screen setup happens up front, since both the
    // menu and the calibration step need a render loop of their own.
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

    // Calibration no longer runs unconditionally on every launch (see
    // `specs/app-flow.md`) — try to load a previously persisted offset
    // first; the menu just shows locked/unlocked modes accordingly.
    let mut offset_ms: Option<i64> = settings::load_offset_ms();

    run_app(&mut term, &rx, global_clock, &mut offset_ms)?;

    restore_terminal(_raw_mode)?;
    Ok(())
}

fn restore_terminal(raw_mode: terminal::RawModeGuard) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    drop(raw_mode);
    Ok(())
}

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

/// The main menu <-> mode-session loop. Runs until the player quits from
/// either the menu or a session's result screen.
fn run_app(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: &mut Option<i64>,
) -> anyhow::Result<()> {
    loop {
        let is_calibrated = offset_ms.is_some();
        match menu::run(term, rx, is_calibrated)? {
            MenuOutcome::Quit => return Ok(()),
            MenuOutcome::Selected(MenuEntry::Calibrate) => {
                match calibration::run(term, rx, global_clock)? {
                    CalibrationOutcome::Measured {
                        offset_ms: measured,
                    } => {
                        *offset_ms = Some(measured);
                        // Persisting is best-effort: if it fails (e.g. no
                        // writable config dir), calibration still applies
                        // for the rest of this process — just won't be
                        // remembered next launch (see `settings.rs`).
                        let _ = settings::save_offset_ms(measured);
                    }
                    CalibrationOutcome::Aborted => {}
                }
            }
            MenuOutcome::Selected(entry) => {
                // Locked entries can't be selected from the menu itself
                // (see `menu::run`), so `offset_ms` is guaranteed `Some`
                // here for every remaining entry.
                let offset = offset_ms.expect("mode entries require calibration");
                match run_mode(term, rx, global_clock, offset, entry)? {
                    SessionOutcome::BackToMenu => continue,
                    SessionOutcome::Quit => return Ok(()),
                }
            }
        }
    }
}

fn run_mode(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    entry: MenuEntry,
) -> anyhow::Result<SessionOutcome> {
    match entry {
        MenuEntry::TapAlong => {
            let beat_duration = Duration::from_secs_f64(60.0 / DEFAULT_BPM);
            let config = SessionConfig {
                schedule: TapAlongSchedule::new(DEFAULT_BPM),
                bpm: DEFAULT_BPM,
                mode_name: "Tap Along",
                beats_per_bar: BEATS_PER_BAR,
                tolerance: beat_duration / 2,
            };
            session::run(term, rx, global_clock, offset_ms, &config)
        }
        MenuEntry::QuietFour => {
            let beat_duration = Duration::from_secs_f64(60.0 / DEFAULT_BPM);
            let config = SessionConfig {
                schedule: QuietFourSchedule::new(
                    DEFAULT_BPM,
                    BEATS_PER_BAR as u32,
                    QUIET_FOUR_CYCLES,
                ),
                bpm: DEFAULT_BPM,
                mode_name: "Quiet Four",
                beats_per_bar: BEATS_PER_BAR,
                tolerance: beat_duration / 2,
            };
            session::run(term, rx, global_clock, offset_ms, &config)
        }
        MenuEntry::Tuplets => {
            let beat_duration = Duration::from_secs_f64(60.0 / DEFAULT_BPM);
            // Tuplet spacing is tighter than a full beat (n subdivisions
            // across 4 beats) — half a subdivision interval is the
            // matching tolerance basis here, not half a grounding beat.
            let subdivision_duration = beat_duration.mul_f64(4.0 / TUPLET_SUBDIVISIONS as f64);
            let config = SessionConfig {
                schedule: TupletSchedule::new(DEFAULT_BPM, TUPLET_SUBDIVISIONS, TUPLET_BARS),
                bpm: DEFAULT_BPM,
                mode_name: "Tuplets",
                beats_per_bar: BEATS_PER_BAR,
                tolerance: subdivision_duration / 2,
            };
            session::run(term, rx, global_clock, offset_ms, &config)
        }
        MenuEntry::RhythmReader => {
            let config = RhythmReaderConfig {
                bpm: RHYTHM_READER_BPM,
                repetitions: RHYTHM_READER_REPETITIONS,
            };
            rhythm_reader_session::run(term, rx, global_clock, offset_ms, &config)
        }
        MenuEntry::Calibrate | MenuEntry::Quit => {
            unreachable!("handled directly in run_app")
        }
    }
}
