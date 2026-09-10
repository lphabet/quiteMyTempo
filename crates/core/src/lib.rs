//! Core timing/beat-schedule primitives shared by all keyboard-mode exercises
//! (Tap Along, Quiet Four, Tuplets). See `specs/keyboard-modes.md` for the
//! product-level description of each mode.
//!
//! Design note (refines the rough sketch in `specs/architecture.md`):
//! We separate two concerns that the architecture doc's `BeatSchedule`
//! conflated: the metronome pulse that is actually played as audio (see
//! [`ClickEvent`], the "grounding" click, e.g. a plain 4/4 pulse) versus the
//! point in time at which the player is expected to tap (see
//! [`ExpectedTap`], used purely for evaluation).
//!
//! For Tap Along and Quiet Four these coincide (every audible click is also
//! an expected tap). For Tuplets they diverge: the metronome keeps clicking
//! a plain 4-beat grounding pulse while the player taps n evenly spaced
//! subdivisions across the bar.

pub mod evaluator;
pub mod schedule;
pub mod schedules;

pub use evaluator::{SessionSummary, TapResult, TimingEvaluator, TimingEvent};
pub use schedule::{ClickEvent, ExpectedTap, Schedule};
pub use schedules::quiet_four::QuietFourSchedule;
pub use schedules::rhythm_reader::{
    generate_pattern, pattern_duration_beats, GeneratorConfig, NoteValue, Pattern, PatternNote,
    RhythmReaderSchedule,
};
pub use schedules::tap_along::TapAlongSchedule;
pub use schedules::tuplet::TupletSchedule;
