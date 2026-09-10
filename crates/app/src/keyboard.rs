//! Keyboard input capture — the `KeyboardSource` side of the
//! `TimingSource` abstraction described in `specs/architecture.md`.
//!
//! Polls `crossterm` for key-press events on a dedicated thread and reports
//! each press as a [`quietmytempo_core::TimingEvent`] timestamped against the
//! shared [`crate::clock::SessionClock`], sent over a channel so the main
//! loop can feed them into a `TimingEvaluator` without blocking on input
//! polling itself.
//!
//! Beyond the tap/quit signals needed for the exercise sessions themselves,
//! this also reports menu-navigation signals (see `specs/app-flow.md`) —
//! kept in the same module/channel rather than a separate input source
//! since it's still just "which key got pressed", the meaning is purely a
//! matter of which signal variant it maps to.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use quietmytempo_core::TimingEvent;

use crate::clock::SessionClock;

/// The key the player presses to register a tap. Space bar is the most
/// ergonomic choice for a rhythm-tapping exercise (large target, easy to
/// hit repeatedly without looking at the keyboard).
const TAP_KEY: KeyCode = KeyCode::Char(' ');

/// Signals coming out of the keyboard thread: taps/quit (used during
/// exercise sessions and calibration) plus menu-navigation signals (used by
/// `menu.rs`, see `specs/app-flow.md`). All key events funnel through one
/// channel/enum rather than separate input sources per screen, since
/// they're all just "a key was pressed" — the meaning is assigned by
/// whichever loop is currently reading from the channel.
pub enum KeyboardSignal {
    Tap(TimingEvent),
    /// Up arrow / `k` — move menu focus up.
    Up,
    /// Down arrow / `j` — move menu focus down.
    Down,
    /// Enter — activate the focused menu entry.
    Select,
    /// `m` — return to the main menu from a result screen.
    BackToMenu,
    Quit,
}

/// Spawns a background thread that polls for key events and forwards taps,
/// navigation, and quit requests over the returned channel. Assumes the
/// terminal has already been put into raw mode by the caller (see
/// `crate::terminal`).
pub fn spawn(clock: SessionClock) -> Receiver<KeyboardSignal> {
    let (tx, rx): (Sender<KeyboardSignal>, Receiver<KeyboardSignal>) = std::sync::mpsc::channel();

    std::thread::spawn(move || loop {
        // Short poll timeout keeps tap-timestamp resolution tight: worst
        // case latency added by polling itself is bounded by this value.
        match event::poll(Duration::from_millis(2)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key_event)) if key_event.kind == KeyEventKind::Press => {
                    let signal = match key_event.code {
                        code if code == TAP_KEY => KeyboardSignal::Tap(TimingEvent {
                            at: clock.elapsed(),
                        }),
                        KeyCode::Up | KeyCode::Char('k') => KeyboardSignal::Up,
                        KeyCode::Down | KeyCode::Char('j') => KeyboardSignal::Down,
                        KeyCode::Enter => KeyboardSignal::Select,
                        KeyCode::Char('m') => KeyboardSignal::BackToMenu,
                        KeyCode::Esc | KeyCode::Char('q') => KeyboardSignal::Quit,
                        KeyCode::Char('c')
                            if key_event
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            KeyboardSignal::Quit
                        }
                        _ => continue,
                    };
                    if tx.send(signal).is_err() {
                        return; // receiver dropped, session ended
                    }
                }
                Ok(_) => continue,
                Err(_) => return,
            },
            Ok(false) => continue,
            Err(_) => return,
        }
    });

    rx
}
