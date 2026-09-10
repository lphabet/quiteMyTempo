//! Mode 4: Rhythm Reader (see `specs/keyboard-modes.md`).
//!
//! Unlike the other three modes, the expected-tap grid here is **not** an
//! evenly spaced pulse — it is a concrete, possibly irregular note pattern
//! (quarters/eighths/sixteenths/dotted quarters, plus rests) that the
//! player must read and reproduce. A one-bar audible count-in (a plain
//! 4-beat click, no notes) gives the player the tempo reference before the
//! pattern itself starts; the pattern itself plays with no grounding click
//! (the player is meant to read + internalize the rhythm, not tap along to
//! a click during the pattern itself).
//!
//! Patterns are generated procedurally per repetition (see
//! [`generate_pattern`]) rather than drawn from a fixed library — see
//! `specs/keyboard-modes.md` Mode 4 "Pattern-Erzeugung".

use std::time::Duration;

use crate::schedule::{ClickEvent, ExpectedTap, Schedule};

/// A note value expressed as a fraction of one quarter note ("beat" in
/// 4/4), which is all the `Schedule` implementation needs to place it on
/// the timeline. Deliberately limited to the even subdivisions named in
/// `specs/keyboard-modes.md` — odd subdivisions (triplets etc.) are Mode 3
/// (Tuplets)'s job, not this mode's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteValue {
    Quarter,
    Eighth,
    Sixteenth,
    DottedQuarter,
}

impl NoteValue {
    /// Duration of this note value in beats (1 beat = 1 quarter note),
    /// as an exact fraction to avoid float drift when summing many notes
    /// (see [`pattern_duration_beats`]).
    pub fn duration_beats(&self) -> f64 {
        match self {
            NoteValue::Quarter => 1.0,
            NoteValue::Eighth => 0.5,
            NoteValue::Sixteenth => 0.25,
            NoteValue::DottedQuarter => 1.5,
        }
    }
}

/// A single slot in a pattern: a note value, plus whether it is a rest
/// (silence — no tap expected) rather than a note the player must tap.
/// Reusing `NoteValue` for rests too (rather than separate rest types)
/// keeps the type small; `is_rest` is what actually changes the
/// evaluation/scoring behavior (see [`RhythmReaderSchedule::expected_taps`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternNote {
    pub value: NoteValue,
    pub is_rest: bool,
}

impl PatternNote {
    pub fn note(value: NoteValue) -> Self {
        Self {
            value,
            is_rest: false,
        }
    }

    pub fn rest(value: NoteValue) -> Self {
        Self {
            value,
            is_rest: true,
        }
    }
}

/// A rhythm pattern: an ordered sequence of notes/rests. Must sum to a
/// whole number of 4/4 bars (see [`pattern_duration_beats`] /
/// [`RhythmReaderSchedule::new`]'s assertion) so it can cleanly follow a
/// bar-aligned count-in.
pub type Pattern = Vec<PatternNote>;

/// Total duration of a pattern in beats (sum of each note's duration).
pub fn pattern_duration_beats(pattern: &Pattern) -> f64 {
    pattern.iter().map(|n| n.value.duration_beats()).sum()
}

const BEATS_PER_BAR: f64 = 4.0;

/// One count-in bar (plain 4-beat click, no notes) followed by one
/// playthrough of `pattern`. Running multiple repetitions with
/// potentially different (randomly drawn) patterns is a session-level
/// concern handled by the app layer re-instantiating this schedule per
/// repetition — see `specs/keyboard-modes.md` Mode 4 and
/// `specs/app-flow.md`.
#[derive(Debug, Clone)]
pub struct RhythmReaderSchedule {
    bpm: f64,
    pattern: Pattern,
}

impl RhythmReaderSchedule {
    /// `pattern` must sum to a whole number of 4/4 bars (>= 1 bar); this is
    /// a content invariant guaranteed by [`generate_pattern`], not
    /// something derived at runtime, so it is checked with an assertion
    /// rather than a recoverable error.
    pub fn new(bpm: f64, pattern: Pattern) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        assert!(!pattern.is_empty(), "pattern must not be empty");
        let beats = pattern_duration_beats(&pattern);
        assert!(
            ((beats / BEATS_PER_BAR).round() * BEATS_PER_BAR - beats).abs() < 1e-6,
            "pattern must sum to a whole number of 4/4 bars, got {beats} beats"
        );
        Self { bpm, pattern }
    }

    pub fn beat_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm)
    }

    /// Length of the count-in: exactly one 4/4 bar, always audible,
    /// always click-only (no notes to tap during it).
    pub fn count_in_duration(&self) -> Duration {
        self.beat_duration().mul_f64(BEATS_PER_BAR)
    }

    /// Shortest note value actually used in the pattern, in beats — the
    /// natural basis for a caller to derive a matching tolerance (e.g.
    /// half of this), since a fixed beat-based tolerance (as used by
    /// Tap Along) would be far too generous for sixteenth-note runs.
    pub fn shortest_note_beats(&self) -> f64 {
        self.pattern
            .iter()
            .map(|n| n.value.duration_beats())
            .fold(f64::INFINITY, f64::min)
    }
}

impl Schedule for RhythmReaderSchedule {
    /// Only the count-in clicks (4 plain beats). The pattern itself plays
    /// with no grounding click — see module docs.
    fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent> {
        let beat = self.beat_duration();
        (0..BEATS_PER_BAR as u32)
            .map(|i| ClickEvent {
                at: beat.mul_f64(f64::from(i)),
            })
            .filter(|c| c.at <= session_duration)
            .collect()
    }

    /// One `ExpectedTap` per non-rest note, positioned after the count-in.
    /// All are scored (`informational: false`) — unlike Quiet Four, this
    /// mode has no "informative only" beats.
    fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap> {
        let beat = self.beat_duration();
        let count_in = self.count_in_duration();
        let mut cursor_beats = 0.0;
        let mut taps = Vec::new();

        for note in &self.pattern {
            let at = count_in + beat.mul_f64(cursor_beats);
            if !note.is_rest {
                taps.push(ExpectedTap::scored(at));
            }
            cursor_beats += note.value.duration_beats();
        }

        taps.into_iter()
            .filter(|t| t.at <= session_duration)
            .collect()
    }
}

/// Number of sixteenth-note units in one 4/4 bar (4 beats * 4 sixteenths
/// per beat) — the integer grid [`generate_pattern`] fills, so bars always
/// come out to an exact whole number of beats regardless of which note
/// values get picked (all of `NoteValue`'s durations are whole multiples
/// of a sixteenth).
const UNITS_PER_BAR: u32 = 16;

/// Config for procedural pattern generation (see
/// `specs/keyboard-modes.md` Mode 4 "Pattern-Erzeugung"). Patterns are no
/// longer drawn from a fixed curated library — they are generated fresh
/// per repetition from these parameters, so difficulty can be tuned later
/// by widening/narrowing `note_pool` or adjusting `rest_probability`
/// without touching the generation algorithm itself.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Number of 4/4 bars the generated pattern should span.
    pub bars: u32,
    /// Note values eligible to be picked for any given slot. Must contain
    /// at least one value; [`NoteValue::Sixteenth`] is used as a fallback
    /// whenever nothing else in the pool still fits the bar's remaining
    /// space, so a pool without it still terminates correctly.
    pub note_pool: Vec<NoteValue>,
    /// Independent probability, in `[0.0, 1.0]`, that any given slot is
    /// generated as a rest rather than a note.
    pub rest_probability: f64,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            bars: 1,
            note_pool: vec![
                NoteValue::Quarter,
                NoteValue::Eighth,
                NoteValue::Sixteenth,
                NoteValue::DottedQuarter,
            ],
            rest_probability: 0.15,
        }
    }
}

/// Duration of `value` expressed in sixteenth-note units (an exact
/// integer, unlike [`NoteValue::duration_beats`]'s `f64`), used by
/// [`generate_pattern`] so bar-filling arithmetic never drifts.
fn units(value: NoteValue) -> u32 {
    (value.duration_beats() * 4.0).round() as u32
}

/// Procedurally generates a random pattern spanning `config.bars` bars of
/// 4/4, per `specs/keyboard-modes.md` Mode 4 "Pattern-Erzeugung". Each bar
/// is filled slot by slot: a note value is picked at random from whichever
/// entries in `config.note_pool` still fit the bar's remaining space, then
/// independently turned into a rest with probability
/// `config.rest_probability`. Filling in sixteenth-unit space guarantees
/// each bar sums to exactly `UNITS_PER_BAR` (i.e. a whole bar), so the
/// result always satisfies [`RhythmReaderSchedule::new`]'s bar-alignment
/// assertion.
///
/// Takes a source of randomness as a generic closure returning a value in
/// `[0.0, 1.0)` (like `rand::Rng::gen::<f64>()`) rather than depending on
/// the `rand` crate directly — `quietmytempo-core` deliberately has zero
/// dependencies (see `crates/core/Cargo.toml`); RNG stays an app-layer
/// concern, with the caller (e.g. `rhythm_reader_session.rs`) supplying
/// `rand::thread_rng()`.
pub fn generate_pattern(config: &GeneratorConfig, mut random: impl FnMut() -> f64) -> Pattern {
    assert!(config.bars >= 1, "bars must be >= 1");
    assert!(!config.note_pool.is_empty(), "note_pool must not be empty");
    assert!(
        (0.0..=1.0).contains(&config.rest_probability),
        "rest_probability must be within [0.0, 1.0]"
    );

    let mut pattern = Pattern::new();

    for _ in 0..config.bars {
        let mut remaining = UNITS_PER_BAR;
        while remaining > 0 {
            let candidates: Vec<NoteValue> = config
                .note_pool
                .iter()
                .copied()
                .filter(|v| units(*v) <= remaining)
                .collect();
            // A sixteenth (1 unit) always fits any remaining space >= 1,
            // so this fallback guarantees the loop always makes progress
            // even if the configured pool has no value narrow enough for
            // whatever's left in the bar.
            let value = if candidates.is_empty() {
                NoteValue::Sixteenth
            } else {
                let idx = ((random() * candidates.len() as f64) as usize).min(candidates.len() - 1);
                candidates[idx]
            };
            let is_rest = random() < config.rest_probability;
            pattern.push(if is_rest {
                PatternNote::rest(value)
            } else {
                PatternNote::note(value)
            });
            remaining -= units(value);
        }
    }

    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn note_durations_are_fractions_of_a_beat() {
        assert_eq!(NoteValue::Quarter.duration_beats(), 1.0);
        assert_eq!(NoteValue::Eighth.duration_beats(), 0.5);
        assert_eq!(NoteValue::Sixteenth.duration_beats(), 0.25);
        assert_eq!(NoteValue::DottedQuarter.duration_beats(), 1.5);
    }

    /// Deterministic pseudo-random sequence (xorshift32) so generator
    /// tests are reproducible without depending on the `rand` crate
    /// (core has zero dependencies — see `crates/core/Cargo.toml`).
    fn deterministic_random(seed: u32) -> impl FnMut() -> f64 {
        let mut state = seed.max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            f64::from(state) / f64::from(u32::MAX)
        }
    }

    #[test]
    fn generated_patterns_sum_to_a_whole_number_of_bars() {
        for seed in 1..50u32 {
            let config = GeneratorConfig::default();
            let pattern = generate_pattern(&config, deterministic_random(seed));
            let beats = pattern_duration_beats(&pattern);
            assert!(
                ((beats / BEATS_PER_BAR).round() * BEATS_PER_BAR - beats).abs() < 1e-6,
                "pattern beats {beats} is not a whole number of bars"
            );
        }
    }

    #[test]
    fn generate_pattern_respects_bars_config() {
        let config = GeneratorConfig {
            bars: 2,
            ..GeneratorConfig::default()
        };
        let pattern = generate_pattern(&config, deterministic_random(7));
        let beats = pattern_duration_beats(&pattern);
        assert!((beats - 2.0 * BEATS_PER_BAR).abs() < 1e-6);
    }

    #[test]
    fn generate_pattern_only_uses_values_from_the_pool() {
        let config = GeneratorConfig {
            bars: 3,
            note_pool: vec![NoteValue::Quarter, NoteValue::Eighth],
            rest_probability: 0.3,
        };
        let pattern = generate_pattern(&config, deterministic_random(42));
        assert!(pattern
            .iter()
            .all(|n| matches!(n.value, NoteValue::Quarter | NoteValue::Eighth)));
    }

    #[test]
    fn generate_pattern_zero_rest_probability_yields_no_rests() {
        let config = GeneratorConfig {
            rest_probability: 0.0,
            ..GeneratorConfig::default()
        };
        let pattern = generate_pattern(&config, deterministic_random(99));
        assert!(pattern.iter().all(|n| !n.is_rest));
    }

    #[test]
    fn count_in_is_exactly_one_bar_of_plain_clicks() {
        // 60 BPM -> beat = 1000ms, count-in bar = 4000ms.
        let pattern = vec![PatternNote::note(NoteValue::Quarter); 4];
        let schedule = RhythmReaderSchedule::new(60.0, pattern);
        assert_eq!(schedule.count_in_duration(), ms(4000));

        let clicks = schedule.clicks(ms(10_000));
        let times: Vec<u64> = clicks.iter().map(|c| c.at.as_millis() as u64).collect();
        assert_eq!(times, vec![0, 1000, 2000, 3000]);
    }

    #[test]
    fn rests_do_not_produce_expected_taps() {
        // beat = 1000ms (60bpm); pattern: rest(quarter), note(quarter),
        // note(eighth), note(eighth), note(quarter) -> sums to 4 beats.
        let pattern = vec![
            PatternNote::rest(NoteValue::Quarter),
            PatternNote::note(NoteValue::Quarter),
            PatternNote::note(NoteValue::Eighth),
            PatternNote::note(NoteValue::Eighth),
            PatternNote::note(NoteValue::Quarter),
        ];
        let schedule = RhythmReaderSchedule::new(60.0, pattern);
        let taps = schedule.expected_taps(ms(20_000));

        // Count-in is 4000ms. Pattern starts at 4000ms:
        // rest quarter (4000..5000, no tap), note quarter at 5000,
        // note eighth at 6000, note eighth at 6500, note quarter at 7000.
        let times: Vec<u64> = taps.iter().map(|t| t.at.as_millis() as u64).collect();
        assert_eq!(times, vec![5000, 6000, 6500, 7000]);
        assert!(taps.iter().all(|t| !t.informational));
    }

    #[test]
    fn shortest_note_beats_finds_the_minimum_across_the_pattern() {
        // 1 + 0.25*4 + 1 = 3, not a whole bar; use four sixteenths to pad
        // it out to exactly 4 beats instead.
        let pattern = vec![
            PatternNote::note(NoteValue::Quarter),
            PatternNote::note(NoteValue::Sixteenth),
            PatternNote::note(NoteValue::Sixteenth),
            PatternNote::note(NoteValue::Sixteenth),
            PatternNote::note(NoteValue::Sixteenth),
            PatternNote::note(NoteValue::Eighth),
            PatternNote::note(NoteValue::Quarter),
            PatternNote::note(NoteValue::Eighth),
        ];
        let schedule = RhythmReaderSchedule::new(60.0, pattern);
        assert_eq!(schedule.shortest_note_beats(), 0.25);
    }

    #[test]
    #[should_panic(expected = "whole number of 4/4 bars")]
    fn pattern_not_summing_to_whole_bars_panics() {
        let pattern = vec![PatternNote::note(NoteValue::Quarter); 3]; // 3 beats, not a multiple of 4
        RhythmReaderSchedule::new(60.0, pattern);
    }

    #[test]
    #[should_panic(expected = "whole number of 4/4 bars")]
    fn pattern_rounding_down_to_fewer_bars_panics() {
        // 5 beats = 1.25 bars; rounds down to 1 bar, so the naive (non-abs)
        // check `rounded_bars * BEATS_PER_BAR - beats < 1e-6` would wrongly
        // pass since the difference is negative.
        let pattern = vec![PatternNote::note(NoteValue::Quarter); 5];
        RhythmReaderSchedule::new(60.0, pattern);
    }
}
