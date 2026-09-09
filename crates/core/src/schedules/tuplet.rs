//! Mode 3: Tuplets (see `specs/keyboard-modes.md`).
//!
//! The metronome keeps clicking a plain 4-beats-per-bar grounding pulse
//! (audible clicks). The player is expected to tap `subdivisions` evenly
//! spaced taps across the *entire bar* (i.e. across the 4 grounding
//! clicks) — e.g. 3 taps spread over 4 clicks for a triplet feel, 5 for a
//! quintuplet feel, 7 for a septuplet feel. This is the one mode where
//! `clicks()` and `expected_taps()` genuinely diverge in count and spacing.

use std::time::Duration;

use crate::schedule::{ClickEvent, ExpectedTap, Schedule};

const GROUNDING_BEATS_PER_BAR: u32 = 4;

#[derive(Debug, Clone, Copy)]
pub struct TupletSchedule {
    bpm: f64,
    subdivisions: u32,
    bars: u32,
}

impl TupletSchedule {
    /// `subdivisions` is the number of evenly spaced taps expected per bar
    /// (e.g. 3, 5, or 7 per `specs/keyboard-modes.md`); the constructor
    /// doesn't hard-restrict the value so callers can experiment, but the
    /// product spec only exposes 3/5/7 in the UI.
    pub fn new(bpm: f64, subdivisions: u32, bars: u32) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        assert!(subdivisions > 0, "subdivisions must be positive");
        assert!(bars > 0, "bars must be positive");
        Self {
            bpm,
            subdivisions,
            bars,
        }
    }

    fn beat_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm)
    }

    fn bar_duration(&self) -> Duration {
        self.beat_duration() * GROUNDING_BEATS_PER_BAR
    }
}

impl Schedule for TupletSchedule {
    fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent> {
        let beat = self.beat_duration();
        let total_beats = self.bars * GROUNDING_BEATS_PER_BAR;
        (0..=total_beats)
            .map(|i| ClickEvent { at: beat * i })
            .filter(|c| c.at <= session_duration)
            .collect()
    }

    fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap> {
        let bar = self.bar_duration();
        let mut taps = Vec::new();
        for bar_index in 0..self.bars {
            let bar_start = bar * bar_index;
            for i in 0..self.subdivisions {
                let offset =
                    Duration::from_secs_f64(bar.as_secs_f64() * (i as f64) / self.subdivisions as f64);
                taps.push(ExpectedTap::scored(bar_start + offset));
            }
        }
        taps.into_iter().filter(|t| t.at <= session_duration).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn grounding_clicks_are_a_plain_four_beat_pulse() {
        // 60 BPM -> beat = 1000ms, bar = 4000ms.
        let schedule = TupletSchedule::new(60.0, 3, 2);
        let clicks = schedule.clicks(ms(8000));
        let times: Vec<u64> = clicks.iter().map(|c| c.at.as_millis() as u64).collect();
        assert_eq!(times, vec![0, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000]);
    }

    #[test]
    fn triplet_spreads_three_taps_evenly_across_one_bar() {
        // bar = 4000ms -> triplet taps at 0, 4000/3, 8000/3
        let schedule = TupletSchedule::new(60.0, 3, 1);
        let taps = schedule.expected_taps(ms(4000));
        assert_eq!(taps.len(), 3);
        assert_eq!(taps[0].at, ms(0));
        assert!((taps[1].at.as_secs_f64() - 4.0 / 3.0).abs() < 1e-6);
        assert!((taps[2].at.as_secs_f64() - 8.0 / 3.0).abs() < 1e-6);
        assert!(taps.iter().all(|t| !t.informational));
    }

    #[test]
    fn quintuplet_and_septuplet_produce_expected_counts() {
        let quintuplet = TupletSchedule::new(60.0, 5, 2);
        assert_eq!(quintuplet.expected_taps(ms(8000)).len(), 10);

        let septuplet = TupletSchedule::new(60.0, 7, 2);
        assert_eq!(septuplet.expected_taps(ms(8000)).len(), 14);
    }

    #[test]
    fn clicks_and_expected_taps_diverge_in_count() {
        let schedule = TupletSchedule::new(60.0, 5, 1);
        assert_eq!(schedule.clicks(ms(4000)).len(), 5); // 4 grounding beats + bar-end click
        assert_eq!(schedule.expected_taps(ms(4000)).len(), 5);
        // Same count here is coincidental (4 grounding intervals + 1 = 5
        // clicks vs. 5 quintuplet taps); verify spacing actually differs.
        let clicks = schedule.clicks(ms(4000));
        let taps = schedule.expected_taps(ms(4000));
        assert_ne!(clicks[1].at, taps[1].at);
    }
}
