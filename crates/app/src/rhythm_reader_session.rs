//! Live-session runner for Mode 4: Rhythm Reader (see
//! `specs/keyboard-modes.md`). Kept separate from `session.rs` because its
//! structure genuinely differs from the continuous-pulse modes: each
//! repetition has its own count-in + pattern playthrough, notes (not an
//! even beat grid) drive the expected-tap timeline, and misses/mistaps are
//! tracked in addition to timing deviation.

use std::time::Duration;

use quietmytempo_core::{
    generate_pattern, ExpectedTap, GeneratorConfig, Pattern, RhythmReaderSchedule, Schedule,
    TapResult, TimingEvaluator, TimingEvent,
};
use rand::Rng;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::clock::SessionClock;
use crate::keyboard::KeyboardSignal;
use crate::metronome::Metronome;
use crate::rhythm_reader_ui::{self, RhythmReaderState};
use crate::session::SessionOutcome;

const FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS
/// Pause shown between repetitions before the next count-in starts, so the
/// player gets a clear "that one's done" beat before the next pattern
/// appears (see `specs/keyboard-modes.md` Mode 4, step 4).
const INTER_REP_PAUSE: Duration = Duration::from_millis(900);

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

pub struct RhythmReaderConfig {
    pub bpm: f64,
    pub repetitions: u32,
}

/// Aggregated stats across every repetition in the session, shown on the
/// result screen (see `specs/keyboard-modes.md` Mode 4 "Result Screen").
#[derive(Debug, Clone, Default)]
pub struct RhythmReaderSummary {
    pub all_results: Vec<TapResult>,
    pub missed_notes: usize,
    pub mistaps: usize,
    pub total_expected_notes: usize,
}

impl RhythmReaderSummary {
    pub fn hit_rate(&self) -> f64 {
        if self.total_expected_notes == 0 {
            return 0.0;
        }
        let hits = self.total_expected_notes - self.missed_notes;
        hits as f64 / self.total_expected_notes as f64
    }
}

pub fn run(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    config: &RhythmReaderConfig,
) -> anyhow::Result<SessionOutcome> {
    loop {
        match run_repetitions(term, rx, global_clock, offset_ms, config)? {
            RepetitionsOutcome::Finished(summary) => {
                match show_result_screen(term, rx, &summary)? {
                    ResultScreenOutcome::Restart => continue,
                    ResultScreenOutcome::BackToMenu => return Ok(SessionOutcome::BackToMenu),
                    ResultScreenOutcome::Quit => return Ok(SessionOutcome::Quit),
                }
            }
            RepetitionsOutcome::Quit => return Ok(SessionOutcome::Quit),
        }
    }
}

enum RepetitionsOutcome {
    Finished(RhythmReaderSummary),
    Quit,
}

fn run_repetitions(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    config: &RhythmReaderConfig,
) -> anyhow::Result<RepetitionsOutcome> {
    let mut rng = rand::thread_rng();
    let generator_config = GeneratorConfig::default();
    let mut summary = RhythmReaderSummary::default();
    let mut previous_pattern: Option<Pattern> = None;

    for rep in 0..config.repetitions {
        let pattern = draw_pattern(&generator_config, previous_pattern.as_ref(), &mut rng);
        previous_pattern = Some(pattern.clone());

        match run_one_repetition(term, rx, global_clock, offset_ms, config, pattern, rep)? {
            RepOutcome::Done {
                results,
                missed,
                mistaps,
                expected_count,
            } => {
                summary.all_results.extend(results);
                summary.missed_notes += missed;
                summary.mistaps += mistaps;
                summary.total_expected_notes += expected_count;
            }
            // Player pressed q/Esc mid-repetition: stop repeating early but
            // still route through the result screen (see `session.rs::run_live`
            // for the same pattern) so `q`/Esc lands them on the menu instead
            // of killing the whole process.
            RepOutcome::Aborted {
                results,
                missed,
                mistaps,
                expected_count,
            } => {
                summary.all_results.extend(results);
                summary.missed_notes += missed;
                summary.mistaps += mistaps;
                summary.total_expected_notes += expected_count;
                return Ok(RepetitionsOutcome::Finished(summary));
            }
            RepOutcome::Quit => return Ok(RepetitionsOutcome::Quit),
        }

        if rep + 1 < config.repetitions {
            std::thread::sleep(INTER_REP_PAUSE);
        }
    }

    Ok(RepetitionsOutcome::Finished(summary))
}

/// Maximum number of retries when trying to avoid an immediate pattern
/// repeat. A degenerate `GeneratorConfig` (e.g. a `note_pool` with a single
/// value and `rest_probability: 0.0`) can deterministically produce the same
/// pattern on every call, in which case retrying forever would hang; this
/// cap ensures `draw_pattern` always terminates.
const MAX_REPEAT_AVOIDANCE_ATTEMPTS: usize = 20;

/// Generates a random pattern, avoiding an immediate repeat of `previous`
/// (see `specs/keyboard-modes.md` Mode 4 "Pattern-Erzeugung"). Patterns are
/// generated fresh each call rather than drawn from a fixed library, so an
/// immediate repeat is only possible by coincidence; retrying on a match
/// reduces (but, for a degenerate config that always produces the same
/// pattern, cannot fully eliminate) that chance. After
/// `MAX_REPEAT_AVOIDANCE_ATTEMPTS` retries the last candidate is accepted
/// even if it repeats `previous`, so this function always terminates.
fn draw_pattern(
    config: &GeneratorConfig,
    previous: Option<&Pattern>,
    rng: &mut impl Rng,
) -> Pattern {
    let mut candidate = generate_pattern(config, || rng.gen::<f64>());
    for _ in 0..MAX_REPEAT_AVOIDANCE_ATTEMPTS {
        if Some(&candidate) != previous {
            return candidate;
        }
        candidate = generate_pattern(config, || rng.gen::<f64>());
    }
    candidate
}

enum RepOutcome {
    Done {
        results: Vec<TapResult>,
        missed: usize,
        mistaps: usize,
        expected_count: usize,
    },
    /// Player pressed q/Esc mid-repetition. Carries whatever was scored so
    /// far so the result screen still reflects the partial attempt, mirroring
    /// `session.rs::run_live`'s `Quit`-during-live-play → `LiveOutcome::Finished`
    /// behavior (see doc comment there).
    Aborted {
        results: Vec<TapResult>,
        missed: usize,
        mistaps: usize,
        expected_count: usize,
    },
    Quit,
}

fn run_one_repetition(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    global_clock: SessionClock,
    offset_ms: i64,
    config: &RhythmReaderConfig,
    pattern: Pattern,
    rep_index: u32,
) -> anyhow::Result<RepOutcome> {
    let schedule = RhythmReaderSchedule::new(config.bpm, pattern.clone());
    // Session duration just needs to cover count-in + pattern once.
    let session_duration = schedule.count_in_duration()
        + Duration::from_secs_f64(
            60.0 / config.bpm * quietmytempo_core::pattern_duration_beats(&pattern),
        )
        + Duration::from_millis(200); // small tail so the last note isn't clipped

    let clicks = schedule.clicks(session_duration);
    let expected_taps = schedule.expected_taps(session_duration);
    let expected_count = expected_taps.len();

    let beat_duration = Duration::from_secs_f64(60.0 / config.bpm);
    // Tolerance scales with the shortest note in the pattern (see
    // `RhythmReaderSchedule::shortest_note_beats` doc comment) rather than
    // a fixed beat-based tolerance — a sixteenth-note run needs a much
    // tighter window than a run of quarters.
    let tolerance = beat_duration.mul_f64(schedule.shortest_note_beats() / 2.0);

    let clock = SessionClock::start_now();
    let (_metronome, _latency_estimate) = Metronome::play(clicks, clock)?;
    std::thread::sleep(Duration::from_millis(50));

    let mut evaluator = TimingEvaluator::new(expected_taps.clone(), tolerance);
    let mut results: Vec<TapResult> = Vec::new();
    let mut mistaps = 0usize;

    let mut state = RhythmReaderState::new(
        config.bpm,
        pattern,
        schedule.count_in_duration(),
        rep_index,
        config.repetitions,
    );

    loop {
        loop {
            match rx.try_recv() {
                Ok(KeyboardSignal::Tap(event)) => {
                    let session_relative_at = clock.rebase(global_clock, event.at);
                    let adjusted_at =
                        crate::calibration::apply_offset(session_relative_at, offset_ms);
                    let adjusted_event = TimingEvent { at: adjusted_at };
                    match evaluator.record(adjusted_event) {
                        Some(result) => {
                            state.push_result(result);
                            results.push(result);
                        }
                        None => {
                            mistaps += 1;
                            state.push_mistap();
                        }
                    }
                }
                // `q`/Esc during live play ends the current session (like
                // `session.rs::run_live`) and drops to the result screen
                // instead of quitting the whole app — see `RepOutcome::Aborted`.
                Ok(KeyboardSignal::Quit) => {
                    let missed = count_missed(&results, &expected_taps);
                    return Ok(RepOutcome::Aborted {
                        results,
                        missed,
                        mistaps,
                        expected_count,
                    });
                }
                Ok(
                    KeyboardSignal::Up
                    | KeyboardSignal::Down
                    | KeyboardSignal::Select
                    | KeyboardSignal::BackToMenu
                    | KeyboardSignal::Shortcut(_),
                ) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(RepOutcome::Quit),
            }
        }

        let elapsed = clock.elapsed();
        state.update_playhead(elapsed);
        term.draw(|frame| rhythm_reader_ui::draw(frame, &state))?;

        if elapsed >= session_duration {
            break;
        }
        std::thread::sleep(FRAME_INTERVAL);
    }

    let missed = count_missed(&results, &expected_taps);

    Ok(RepOutcome::Done {
        results,
        missed,
        mistaps,
        expected_count,
    })
}

/// Counts expected taps that never got matched by a recorded result —
/// shared between the normal end-of-repetition path and the `q`/Esc abort
/// path (`RepOutcome::Aborted`) so both compute "missed" the same way.
fn count_missed(results: &[TapResult], expected_taps: &[ExpectedTap]) -> usize {
    let matched_ats: std::collections::HashSet<_> = results.iter().map(|r| r.expected.at).collect();
    expected_taps
        .iter()
        .filter(|t: &&ExpectedTap| !matched_ats.contains(&t.at))
        .count()
}

enum ResultScreenOutcome {
    Restart,
    BackToMenu,
    Quit,
}

fn show_result_screen(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    summary: &RhythmReaderSummary,
) -> anyhow::Result<ResultScreenOutcome> {
    loop {
        match rx.try_recv() {
            Ok(KeyboardSignal::Tap(_)) | Ok(KeyboardSignal::Select) => {
                return Ok(ResultScreenOutcome::Restart)
            }
            // See `session.rs::show_result_screen` — `q`/Esc returns to the
            // main menu instead of quitting the process.
            Ok(KeyboardSignal::BackToMenu) | Ok(KeyboardSignal::Quit) => {
                return Ok(ResultScreenOutcome::BackToMenu)
            }
            Ok(KeyboardSignal::Up | KeyboardSignal::Down | KeyboardSignal::Shortcut(_)) => {}
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Ok(ResultScreenOutcome::Quit)
            }
        }

        term.draw(|frame| rhythm_reader_ui::draw_result(frame, summary))?;
        std::thread::sleep(FRAME_INTERVAL);
    }
}
