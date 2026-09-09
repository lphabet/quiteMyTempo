//! Mode 1: Tap Along (see `specs/keyboard-modes.md`).
//!
//! Simplest mode: the metronome clicks continuously and every click is also
//! the expected tap. Live per-tap feedback plus session-aggregate stats are
//! handled by [`crate::TimingEvaluator`]; this type only generates the
//! click/expected-tap grid.

use std::time::Duration;

use crate::schedule::{ClickEvent, ExpectedTap, Schedule};

/// Continuous metronome pulse at a fixed tempo. Every audible click is also
/// an expected (scored) tap.
#[derive(Debug, Clone, Copy)]
pub struct TapAlongSchedule {
    bpm: f64,
}

impl TapAlongSchedule {
    pub fn new(bpm: f64) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        Self { bpm }
    }

    fn beat_duration(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.bpm)
    }

    fn beat_times(&self, session_duration: Duration) -> Vec<Duration> {
        let beat = self.beat_duration();
        let mut times = Vec::new();
        let mut t = Duration::ZERO;
        while t <= session_duration {
            times.push(t);
            t += beat;
        }
        times
    }
}

impl Schedule for TapAlongSchedule {
    fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent> {
        self.beat_times(session_duration)
            .into_iter()
            .map(|at| ClickEvent { at })
            .collect()
    }

    fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap> {
        self.beat_times(session_duration)
            .into_iter()
            .map(ExpectedTap::scored)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_beat_every_500ms_at_120bpm() {
        let schedule = TapAlongSchedule::new(120.0);
        let clicks = schedule.clicks(Duration::from_millis(1500));
        let expected: Vec<Duration> = vec![0, 500, 1000, 1500]
            .into_iter()
            .map(Duration::from_millis)
            .collect();
        let actual: Vec<Duration> = clicks.into_iter().map(|c| c.at).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_click_is_also_a_scored_expected_tap() {
        let schedule = TapAlongSchedule::new(60.0);
        let clicks = schedule.clicks(Duration::from_secs(2));
        let taps = schedule.expected_taps(Duration::from_secs(2));
        assert_eq!(clicks.len(), taps.len());
        for tap in taps {
            assert!(!tap.informational);
        }
    }
}
