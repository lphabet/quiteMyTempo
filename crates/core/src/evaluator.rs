use std::time::Duration;

use crate::schedule::ExpectedTap;

/// A single key-press captured from a [`crate::Schedule`]-independent input
/// source (keyboard now, audio/MIDI onset detection later — see
/// `specs/architecture.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingEvent {
    pub at: Duration,
}

/// Result of matching one player tap against the nearest expected tap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapResult {
    pub expected: ExpectedTap,
    pub actual: TimingEvent,
    /// Signed deviation in milliseconds. Positive = late, negative = early.
    pub deviation_ms: f64,
}

impl TapResult {
    pub fn is_scored(&self) -> bool {
        !self.expected.informational
    }
}

/// Aggregate statistics over a set of [`TapResult`]s, split into scored vs.
/// informational-only results (see `ExpectedTap::informational`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SessionSummary {
    pub scored_count: usize,
    pub mean_deviation_ms: f64,
    pub std_dev_ms: f64,
}

impl SessionSummary {
    fn from_deviations(deviations: &[f64]) -> Self {
        let scored_count = deviations.len();
        if scored_count == 0 {
            return Self::default();
        }
        let mean = deviations.iter().sum::<f64>() / scored_count as f64;
        let variance = deviations
            .iter()
            .map(|d| (d - mean).powi(2))
            .sum::<f64>()
            / scored_count as f64;
        Self {
            scored_count,
            mean_deviation_ms: mean,
            std_dev_ms: variance.sqrt(),
        }
    }
}

/// Matches incoming [`TimingEvent`]s against a fixed list of
/// [`ExpectedTap`]s and produces per-tap results plus an aggregate summary.
///
/// This is intentionally decoupled from any particular [`crate::Schedule`]
/// implementation: callers compute `expected_taps` once (e.g. via
/// `Schedule::expected_taps`) and feed events in as they occur, which keeps
/// this type usable both for live (streaming) evaluation and for
/// after-the-fact batch evaluation of a whole recorded session.
///
/// A tap only matches an expected tap if it falls within `tolerance` of it.
/// This matters for the streaming/live use case in particular: without a
/// tolerance window, an accidental double-tap would consume a *future*
/// expected slot far away in time. The next legitimate tap for the beat
/// that slot was "supposed" to represent would then have nothing left to
/// match against nearby and would itself match some other distant slot —
/// an error that, once introduced, propagates and corrupts every
/// subsequent match for the rest of the session. Rejecting out-of-tolerance
/// matches (returning `None` instead) keeps a single mis-tap a self-
/// contained, one-off error instead of a session-wide derailment.
#[derive(Debug, Clone)]
pub struct TimingEvaluator {
    expected: Vec<ExpectedTap>,
    matched: Vec<bool>,
    tolerance: Duration,
}

impl TimingEvaluator {
    /// `tolerance` is the maximum allowed distance between a tap and its
    /// nearest expected tap for them to be considered a match. Callers
    /// should generally derive this from the schedule's beat spacing (e.g.
    /// half a beat) rather than hardcoding a fixed millisecond value, since
    /// tolerance-in-proportion-to-tempo is what actually reflects "closest
    /// plausible beat".
    pub fn new(mut expected: Vec<ExpectedTap>, tolerance: Duration) -> Self {
        expected.sort_by_key(|t| t.at);
        let matched = vec![false; expected.len()];
        Self {
            expected,
            matched,
            tolerance,
        }
    }

    /// Matches a single incoming tap against the closest not-yet-matched
    /// expected tap, provided it falls within `tolerance`. Each expected
    /// tap can only be consumed once, so bursts of duplicate key presses
    /// don't inflate the score.
    ///
    /// Returns `None` if there is no unmatched expected tap within
    /// tolerance (including the case where none are left at all).
    pub fn record(&mut self, event: TimingEvent) -> Option<TapResult> {
        let mut best_idx: Option<usize> = None;
        let mut best_distance = Duration::MAX;

        for (idx, expected) in self.expected.iter().enumerate() {
            if self.matched[idx] {
                continue;
            }
            let distance = abs_duration_diff(expected.at, event.at);
            if distance < best_distance {
                best_distance = distance;
                best_idx = Some(idx);
            }
        }

        let idx = best_idx?;
        if best_distance > self.tolerance {
            return None;
        }
        self.matched[idx] = true;
        let expected = self.expected[idx];
        let deviation_ms = signed_diff_ms(event.at, expected.at);
        Some(TapResult {
            expected,
            actual: event,
            deviation_ms,
        })
    }

    /// Builds an aggregate [`SessionSummary`] from a batch of results
    /// (typically everything returned by [`Self::record`] over a session).
    /// Only scored (non-informational) results contribute to the summary.
    pub fn summarize(results: &[TapResult]) -> SessionSummary {
        let deviations: Vec<f64> = results
            .iter()
            .filter(|r| r.is_scored())
            .map(|r| r.deviation_ms)
            .collect();
        SessionSummary::from_deviations(&deviations)
    }
}

fn abs_duration_diff(a: Duration, b: Duration) -> Duration {
    a.abs_diff(b)
}

fn signed_diff_ms(actual: Duration, expected: Duration) -> f64 {
    let sign = if actual >= expected { 1.0 } else { -1.0 };
    sign * abs_duration_diff(actual, expected).as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn matches_closest_expected_tap() {
        let mut evaluator = TimingEvaluator::new(
            vec![
                ExpectedTap::scored(ms(0)),
                ExpectedTap::scored(ms(500)),
                ExpectedTap::scored(ms(1000)),
            ],
            ms(250),
        );

        let result = evaluator.record(TimingEvent { at: ms(510) }).unwrap();
        assert_eq!(result.expected.at, ms(500));
        assert!((result.deviation_ms - 10.0).abs() < 1e-6);
        assert!(result.deviation_ms > 0.0, "late tap should be positive");
    }

    #[test]
    fn early_tap_is_negative_deviation() {
        let mut evaluator =
            TimingEvaluator::new(vec![ExpectedTap::scored(ms(500))], ms(250));
        let result = evaluator.record(TimingEvent { at: ms(480) }).unwrap();
        assert!((result.deviation_ms + 20.0).abs() < 1e-6);
        assert!(result.deviation_ms < 0.0, "early tap should be negative");
    }

    #[test]
    fn each_expected_tap_is_consumed_only_once() {
        let mut evaluator = TimingEvaluator::new(
            vec![ExpectedTap::scored(ms(0)), ExpectedTap::scored(ms(500))],
            ms(250),
        );

        let first = evaluator.record(TimingEvent { at: ms(10) }).unwrap();
        assert_eq!(first.expected.at, ms(0));

        // A second tap very close to t=0 must NOT be matched against the
        // already-consumed t=0 slot again. It's also too far from t=500 to
        // be within tolerance, so it should be rejected rather than
        // incorrectly matched to a distant slot (see doc comment on
        // `TimingEvaluator` for why that matters).
        let second = evaluator.record(TimingEvent { at: ms(20) });
        assert!(second.is_none());
    }

    #[test]
    fn returns_none_when_all_expected_taps_are_consumed() {
        let mut evaluator = TimingEvaluator::new(vec![ExpectedTap::scored(ms(0))], ms(250));
        assert!(evaluator.record(TimingEvent { at: ms(0) }).is_some());
        assert!(evaluator.record(TimingEvent { at: ms(0) }).is_none());
    }

    #[test]
    fn double_tap_does_not_derail_subsequent_matches() {
        // Regression test for the exact bug observed in manual testing: an
        // accidental double-tap must not consume a far-away future slot and
        // corrupt every match after it.
        let mut evaluator = TimingEvaluator::new(
            vec![
                ExpectedTap::scored(ms(0)),
                ExpectedTap::scored(ms(500)),
                ExpectedTap::scored(ms(1000)),
            ],
            ms(250),
        );

        // Legit tap on beat 0.
        assert!(evaluator.record(TimingEvent { at: ms(5) }).is_some());
        // Accidental double-tap milliseconds later: nearest unmatched slot
        // is now 500ms away (beat at t=500), which is outside tolerance ->
        // must be rejected, not matched.
        assert!(evaluator.record(TimingEvent { at: ms(15) }).is_none());
        // The next legitimate tap on beat 500 must still match beat 500,
        // not be pushed to beat 1000.
        let result = evaluator.record(TimingEvent { at: ms(505) }).unwrap();
        assert_eq!(result.expected.at, ms(500));
    }

    #[test]
    fn tap_far_outside_tolerance_is_rejected_even_if_it_is_the_closest() {
        let mut evaluator = TimingEvaluator::new(vec![ExpectedTap::scored(ms(1000))], ms(100));
        // Only expected tap is 1000ms away from t=0 -> it IS the closest
        // (only) candidate, but still outside the 100ms tolerance.
        assert!(evaluator.record(TimingEvent { at: ms(0) }).is_none());
    }


    #[test]
    fn informational_taps_are_excluded_from_summary() {
        let results = vec![
            TapResult {
                expected: ExpectedTap::informational(ms(0)),
                actual: TimingEvent { at: ms(50) },
                deviation_ms: 50.0,
            },
            TapResult {
                expected: ExpectedTap::scored(ms(1000)),
                actual: TimingEvent { at: ms(1010) },
                deviation_ms: 10.0,
            },
        ];

        let summary = TimingEvaluator::summarize(&results);
        assert_eq!(summary.scored_count, 1);
        assert!((summary.mean_deviation_ms - 10.0).abs() < 1e-6);
    }

    #[test]
    fn summary_of_empty_results_is_default() {
        let summary = TimingEvaluator::summarize(&[]);
        assert_eq!(summary.scored_count, 0);
        assert_eq!(summary.mean_deviation_ms, 0.0);
    }
}
