//! Mode 2: Quiet Four (see `specs/keyboard-modes.md`).
//!
//! One cycle = 2 audible bars + 1 silent bar. "Four" refers to the standard
//! 4 beats per bar, not the number of bars per cycle (a cycle is 3 bars
//! total). The metronome only produces audio for the 2 audible bars; the
//! silent bar's beat grid still exists as expected-tap timestamps so the
//! player's internal sense of tempo can be evaluated.
//!
//! Only the very first beat *after* the silent bar (the "landing" on the new
//! downbeat) counts towards the session score. All other beats — including
//! the 4 beats during the silent bar itself — are still expected and shown
//! to the player for live feedback, but are marked `informational` so they
//! don't affect [`crate::SessionSummary`].

use std::time::Duration;

use crate::schedule::{ClickEvent, ExpectedTap, Schedule};

#[derive(Debug, Clone, Copy)]
pub struct QuietFourSchedule {
    bpm: f64,
    beats_per_bar: u32,
    cycles: u32,
}

const AUDIBLE_BARS_PER_CYCLE: u32 = 2;
/// Audible bars + 1 silent bar.
const BARS_PER_CYCLE: u32 = AUDIBLE_BARS_PER_CYCLE + 1;

impl QuietFourSchedule {
    pub fn new(bpm: f64, beats_per_bar: u32, cycles: u32) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        assert!(beats_per_bar > 0, "beats_per_bar must be positive");
        assert!(cycles > 0, "cycles must be positive");
        Self {
            bpm,
            beats_per_bar,
            cycles,
        }
    }

    fn beat_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm)
    }

    /// All beat timestamps across the whole session, tagged with whether
    /// their bar is audible and whether the beat is the post-silence
    /// landing beat (the first beat of the bar right after a silent bar).
    ///
    /// Bars are generated as a flat global sequence using
    /// `bar_index % BARS_PER_CYCLE`: 0,1 = audible, 2 = silent. One extra
    /// trailing bar is appended after the last requested cycle so that the
    /// final cycle's silent bar also gets its landing beat evaluated.
    fn all_beats(&self) -> Vec<BeatSlot> {
        let beat = self.beat_duration();
        let mut slots = Vec::new();
        let mut beat_index: u32 = 0;

        // +1 trailing bar to capture the landing beat after the last cycle's
        // silent bar.
        let total_bars = self.cycles * BARS_PER_CYCLE + 1;

        for bar_index in 0..total_bars {
            let position_in_cycle = bar_index % BARS_PER_CYCLE;
            let audible = position_in_cycle < AUDIBLE_BARS_PER_CYCLE;
            // Landing bar = first bar after a silent bar, i.e. every bar
            // whose position_in_cycle == 0, except the very first bar
            // (bar_index == 0), which has no preceding silent bar.
            let is_landing_bar = position_in_cycle == 0 && bar_index > 0;

            for beat_in_bar in 0..self.beats_per_bar {
                let at = beat * beat_index;
                let scored = is_landing_bar && beat_in_bar == 0;
                slots.push(BeatSlot {
                    at,
                    audible,
                    scored,
                    cycle: bar_index / BARS_PER_CYCLE,
                });
                beat_index += 1;
            }
        }
        slots
    }
}

#[derive(Debug, Clone, Copy)]
struct BeatSlot {
    at: Duration,
    audible: bool,
    scored: bool,
    #[allow(dead_code)]
    cycle: u32,
}

impl Schedule for QuietFourSchedule {
    fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent> {
        self.all_beats()
            .into_iter()
            .filter(|slot| slot.audible && slot.at <= session_duration)
            .map(|slot| ClickEvent { at: slot.at })
            .collect()
    }

    fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap> {
        self.all_beats()
            .into_iter()
            .filter(|slot| slot.at <= session_duration)
            .map(|slot| {
                if slot.scored {
                    ExpectedTap::scored(slot.at)
                } else {
                    ExpectedTap::informational(slot.at)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    /// At 60 BPM (beat = 1000ms), 4 beats/bar, one cycle = 3 bars = 12 beats
    /// = 12000ms total. Audible bars are beats 0..7 (bar 0 and 1), silent
    /// bar is beats 8..11 (bar 2). The landing beat is beat 12 — the first
    /// beat of the *next* cycle's first (audible) bar.
    #[test]
    fn one_cycle_has_two_audible_bars_then_one_silent_bar() {
        let schedule = QuietFourSchedule::new(60.0, 4, 2);
        let clicks = schedule.clicks(ms(23_000));
        let click_times: Vec<u64> = clicks.iter().map(|c| c.at.as_millis() as u64).collect();

        // Cycle 1 audible bars: beats 0..7 -> 0,1000,...,7000
        // Cycle 1 silent bar (beats 8..11) must NOT be audible.
        assert!(click_times.contains(&0));
        assert!(click_times.contains(&7000));
        assert!(!click_times.contains(&8000));
        assert!(!click_times.contains(&11000));

        // Landing beat (beat 12, start of cycle 2) is audible again.
        assert!(click_times.contains(&12000));
    }

    #[test]
    fn only_landing_beat_is_scored_rest_is_informational() {
        let schedule = QuietFourSchedule::new(60.0, 4, 1);
        let taps = schedule.expected_taps(ms(20_000));

        let scored: Vec<&ExpectedTap> = taps.iter().filter(|t| !t.informational).collect();
        assert_eq!(scored.len(), 1, "exactly one scored tap per cycle");
        assert_eq!(scored[0].at, ms(12_000), "landing beat is beat index 12");

        let silent_bar_beats: Vec<&ExpectedTap> = taps
            .iter()
            .filter(|t| t.at >= ms(8_000) && t.at < ms(12_000))
            .collect();
        assert_eq!(silent_bar_beats.len(), 4);
        assert!(silent_bar_beats.iter().all(|t| t.informational));
    }

    #[test]
    fn multiple_cycles_each_get_their_own_scored_landing() {
        let schedule = QuietFourSchedule::new(120.0, 4, 3);
        // beat duration at 120bpm = 500ms, 12 beats/cycle = 6000ms/cycle
        let taps = schedule.expected_taps(ms(20_000));
        let scored: Vec<&ExpectedTap> = taps.iter().filter(|t| !t.informational).collect();
        assert_eq!(scored.len(), 3);
    }
}
