//! Raw-mode terminal setup/teardown helper.
//!
//! Wrapped in a guard so raw mode is reliably disabled again on drop, even
//! if the app exits via an error or early return — leaving a user's
//! terminal stuck in raw mode is a bad experience.

use crossterm::terminal;

pub struct RawModeGuard;

impl RawModeGuard {
    pub fn enable() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
