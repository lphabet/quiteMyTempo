use std::time::Duration;

/// A metronome click that is actually rendered as audio ("audible" grounding
/// pulse). Whether or not it doubles as an [`ExpectedTap`] depends on the
/// mode (see module docs in `lib.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickEvent {
    /// Time offset from the start of the session/exercise.
    pub at: Duration,
}

/// A point in time at which the player is expected to press the key, used as
/// the reference for timing evaluation.
///
/// `informational` marks taps that are still expected and shown to the
/// player, but that must NOT count towards the core session score (e.g. the
/// four beats tapped during the silent bar in Quiet Four: they get live
/// feedback, but only the tap on the following downbeat counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedTap {
    pub at: Duration,
    pub informational: bool,
}

impl ExpectedTap {
    pub fn scored(at: Duration) -> Self {
        Self {
            at,
            informational: false,
        }
    }

    pub fn informational(at: Duration) -> Self {
        Self {
            at,
            informational: true,
        }
    }
}

/// Produces the audible click grid and the expected-tap grid for a given
/// exercise mode. Implementations are pure/deterministic given a
/// `session_duration`, which keeps them trivially unit-testable without any
/// audio or keyboard I/O.
pub trait Schedule {
    /// All metronome clicks that should be audibly played during the
    /// session.
    fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent>;

    /// All points in time the player is expected to tap, in chronological
    /// order.
    fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap>;
}
