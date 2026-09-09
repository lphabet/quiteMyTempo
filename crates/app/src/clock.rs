//! Precise wall-clock timing anchor(s).
//!
//! The keyboard capture thread (`keyboard::spawn`) is started exactly once
//! for the whole process and timestamps every tap against a single,
//! process-wide [`SessionClock`] ("global clock"). Calibration rounds and
//! the main session each have their own *logical* start point in time
//! (e.g. "when this round's bars started converging", "when the metronome
//! started"), which in general do NOT coincide with the global clock's
//! start. [`SessionClock::elapsed_since`] converts a global-clock
//! timestamp into "time since this phase's own start" so each phase can
//! reason about timing in its own local zero-based domain — exactly the
//! domain `quietmytempo_core::TimingEvent`/`ClickEvent` expect.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct SessionClock {
    start: Instant,
}

impl SessionClock {
    pub fn start_now() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Elapsed time since this clock started.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Converts an `elapsed()` value taken from a *different*, earlier-
    /// started `SessionClock` (`earlier`) into "time since `self` started".
    /// Used to rebase a global-clock timestamp (e.g. a keyboard tap
    /// timestamped against a process-wide clock) into a given phase's own
    /// local zero-based time domain.
    ///
    /// Returns `Duration::ZERO` if `global_elapsed` predates `self`'s start
    /// (i.e. the event happened before this phase began — not meaningful
    /// in this domain, so saturates rather than underflowing/panicking).
    pub fn rebase(&self, earlier: SessionClock, global_elapsed: Duration) -> Duration {
        let event_instant = earlier.start + global_elapsed;
        event_instant.saturating_duration_since(self.start)
    }
}
