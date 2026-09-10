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
    /// a content/curation invariant (see `curated_patterns`), not
    /// something derived at runtime, so it is checked with an assertion
    /// rather than a recoverable error.
    pub fn new(bpm: f64, pattern: Pattern) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        assert!(!pattern.is_empty(), "pattern must not be empty");
        let beats = pattern_duration_beats(&pattern);
        assert!(
            (beats / BEATS_PER_BAR).round() * BEATS_PER_BAR - beats < 1e-6,
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

/// A small, fixed library of curated one-bar (4/4) patterns spanning a
/// range of difficulty, per `specs/keyboard-modes.md` Mode 4 v1 scope
/// (algorithmic generation is explicitly deferred). The app layer draws
/// randomly from this per repetition.
pub fn curated_patterns() -> Vec<Pattern> {
    use NoteValue::*;

    vec![
        // 1. Four quarters — simplest possible pattern.
        vec![
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
        ],
        // 2. All eighths.
        vec![PatternNote::note(Eighth); 8],
        // 3. Quarter, two eighths, quarter, two eighths.
        vec![
            PatternNote::note(Quarter),
            PatternNote::note(Eighth),
            PatternNote::note(Eighth),
            PatternNote::note(Quarter),
            PatternNote::note(Eighth),
            PatternNote::note(Eighth),
        ],
        // 4. Quarter rest on beat 1, syncopated eighths.
        vec![
            PatternNote::rest(Quarter),
            PatternNote::note(Eighth),
            PatternNote::note(Eighth),
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
        ],
        // 5. Dotted quarter + eighth, twice (classic syncopation cell).
        vec![
            PatternNote::note(DottedQuarter),
            PatternNote::note(Eighth),
            PatternNote::note(DottedQuarter),
            PatternNote::note(Eighth),
        ],
        // 6. Sixteenth-note run on beat 1, quarters after.
        vec![
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
            PatternNote::note(Quarter),
        ],
        // 7. Eighth rests interleaved with eighth notes ("off-beat" feel).
        vec![
            PatternNote::rest(Eighth),
            PatternNote::note(Eighth),
            PatternNote::rest(Eighth),
            PatternNote::note(Eighth),
            PatternNote::rest(Eighth),
            PatternNote::note(Eighth),
            PatternNote::rest(Eighth),
            PatternNote::note(Eighth),
        ],
        // 8. Mixed: quarter, sixteenth run, two eighths, quarter.
        vec![
            PatternNote::note(Quarter),
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Sixteenth),
            PatternNote::note(Eighth),
            PatternNote::note(Eighth),
            PatternNote::note(Quarter),
        ],
    ]
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

    #[test]
    fn all_curated_patterns_sum_to_a_whole_number_of_bars() {
        for pattern in curated_patterns() {
            let beats = pattern_duration_beats(&pattern);
            assert!(
                (beats / BEATS_PER_BAR).round() * BEATS_PER_BAR - beats < 1e-6,
                "pattern beats {beats} is not a whole number of bars"
            );
        }
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
}
