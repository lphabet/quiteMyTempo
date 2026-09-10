//! Main menu / start screen (see `specs/app-flow.md`).
//!
//! Pure render + input-loop logic, analogous to `calibration.rs`/`ui.rs` —
//! knows nothing about the metronome/evaluator, only about the list of
//! modes and their locked/unlocked state. `main.rs` interprets the
//! returned [`MenuOutcome`] to decide what to actually run next.

use std::io::Stdout;
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Terminal;

use crate::keyboard::KeyboardSignal;

type Term = Terminal<CrosstermBackend<Stdout>>;

const FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS

/// One selectable entry in the main menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEntry {
    TapAlong,
    QuietFour,
    Tuplets,
    RhythmReader,
    Calibrate,
    Quit,
}

impl MenuEntry {
    const ALL: [MenuEntry; 6] = [
        MenuEntry::TapAlong,
        MenuEntry::QuietFour,
        MenuEntry::Tuplets,
        MenuEntry::RhythmReader,
        MenuEntry::Calibrate,
        MenuEntry::Quit,
    ];

    /// The entry's display label. For [`MenuEntry::Calibrate`], the
    /// wording depends on whether a calibration has already been done:
    /// "Run calibration" the first time, "Redo calibration" afterwards
    /// (no parentheses either way — see `specs/app-flow.md`).
    fn label(&self, is_calibrated: bool) -> String {
        match self {
            MenuEntry::TapAlong => "Tap Along".to_string(),
            MenuEntry::QuietFour => "Quiet Four".to_string(),
            MenuEntry::Tuplets => "Tuplets".to_string(),
            MenuEntry::RhythmReader => "Rhythm Reader".to_string(),
            MenuEntry::Calibrate => {
                if is_calibrated {
                    "Redo calibration".to_string()
                } else {
                    "Run calibration".to_string()
                }
            }
            MenuEntry::Quit => "Quit".to_string(),
        }
    }

    /// The single-letter direct shortcut for this entry (see
    /// `specs/app-flow.md`) — lets the player jump straight to an entry
    /// without navigating with Up/Down first. Deliberately **not** just
    /// the entry's first letter in every case: `q` is already wired
    /// globally to `KeyboardSignal::Quit` (see `keyboard.rs`), so
    /// "Quiet Four" can't use `q` for itself (would never even reach here
    /// as a `Shortcut` — the keyboard thread intercepts it first) and
    /// instead uses `f` ("Four"). `j`/`k`/`m`/space are similarly reserved
    /// (navigation/back-to-menu/tap), so entries whose natural first
    /// letter collides pick the next-most-obvious mnemonic letter
    /// instead.
    fn shortcut(&self) -> char {
        match self {
            MenuEntry::TapAlong => 't',
            MenuEntry::QuietFour => 'f',
            MenuEntry::Tuplets => 'u',
            MenuEntry::RhythmReader => 'r',
            MenuEntry::Calibrate => 'c',
            MenuEntry::Quit => 'q',
        }
    }

    /// Looks up the entry (if any) whose [`Self::shortcut`] matches `c`
    /// (already lower-cased by the caller, see `keyboard.rs`).
    fn by_shortcut(c: char) -> Option<MenuEntry> {
        Self::ALL.into_iter().find(|entry| entry.shortcut() == c)
    }

    /// Entries that require a valid calibration before they can be
    /// selected (see `specs/app-flow.md`: modes are visible-but-locked
    /// until calibrated at least once).
    fn requires_calibration(&self) -> bool {
        matches!(
            self,
            MenuEntry::TapAlong
                | MenuEntry::QuietFour
                | MenuEntry::Tuplets
                | MenuEntry::RhythmReader
        )
    }
}

/// What the player chose from the main menu.
pub enum MenuOutcome {
    Selected(MenuEntry),
    Quit,
}

/// Runs the main menu loop: renders the entry list (locked entries greyed
/// out with a hint if calibration is missing), lets the player navigate
/// with Up/Down (or j/k) and select with Enter, and returns once a
/// selectable entry is chosen or the player quits.
pub fn run(
    term: &mut Term,
    rx: &std::sync::mpsc::Receiver<KeyboardSignal>,
    is_calibrated: bool,
) -> anyhow::Result<MenuOutcome> {
    let mut focused: usize = 0;

    loop {
        match rx.try_recv() {
            Ok(KeyboardSignal::Up) => {
                focused = focused.checked_sub(1).unwrap_or(MenuEntry::ALL.len() - 1);
            }
            Ok(KeyboardSignal::Down) => {
                focused = (focused + 1) % MenuEntry::ALL.len();
            }
            Ok(KeyboardSignal::Select) | Ok(KeyboardSignal::Tap(_)) => {
                let entry = MenuEntry::ALL[focused];
                if let Some(outcome) = try_select(entry, is_calibrated) {
                    return Ok(outcome);
                }
                // Locked entry: ignore selection, stay in the menu (see
                // module docs — no popup, just no-op).
            }
            // Direct one-letter shortcut (see `MenuEntry::shortcut`):
            // jumps straight to that entry's selection/lock behavior,
            // without requiring the player to navigate focus there first.
            // Unknown letters (no entry uses them) are simply ignored.
            Ok(KeyboardSignal::Shortcut(c)) => {
                if let Some(entry) = MenuEntry::by_shortcut(c) {
                    focused = MenuEntry::ALL
                        .iter()
                        .position(|e| *e == entry)
                        .unwrap_or(focused);
                    if let Some(outcome) = try_select(entry, is_calibrated) {
                        return Ok(outcome);
                    }
                }
            }
            Ok(KeyboardSignal::Quit) => return Ok(MenuOutcome::Quit),
            Ok(KeyboardSignal::BackToMenu) => {}
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(MenuOutcome::Quit),
        }

        term.draw(|frame| draw(frame, focused, is_calibrated))?;
        std::thread::sleep(FRAME_INTERVAL);
    }
}

/// Shared selection logic for both Enter-on-focused-entry and direct
/// one-letter shortcuts: quit always succeeds, locked mode entries are a
/// no-op (`None`), everything else resolves to [`MenuOutcome::Selected`].
fn try_select(entry: MenuEntry, is_calibrated: bool) -> Option<MenuOutcome> {
    if entry == MenuEntry::Quit {
        return Some(MenuOutcome::Quit);
    }
    if !entry.requires_calibration() || is_calibrated {
        return Some(MenuOutcome::Selected(entry));
    }
    None
}

fn draw(frame: &mut ratatui::Frame, focused: usize, is_calibrated: bool) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new(Span::styled(
        "quietMyTempo",
        Style::default()
            .fg(Color::Rgb(220, 224, 235))
            .add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );
    frame.render_widget(title, chunks[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (idx, entry) in MenuEntry::ALL.iter().enumerate() {
        let locked = entry.requires_calibration() && !is_calibrated;
        let is_focused = idx == focused;

        let color = if locked {
            Color::Rgb(90, 96, 112)
        } else if is_focused {
            Color::Rgb(120, 190, 255)
        } else {
            Color::Rgb(220, 224, 235)
        };

        let marker = if is_focused { "> " } else { "  " };
        let suffix = if locked { "  (locked)" } else { "" };

        let style = if is_focused {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        lines.push(Line::from(Span::styled(
            format!(
                "{marker}[{}] {}{suffix}",
                entry.shortcut().to_ascii_uppercase(),
                entry.label(is_calibrated)
            ),
            style,
        )));
    }

    let menu = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(menu, chunks[1]);

    let focused_entry = MenuEntry::ALL[focused];
    let hint = if focused_entry.requires_calibration() && !is_calibrated {
        "Please calibrate first to unlock this mode."
    } else {
        "Up/Down (or j/k) navigate  |  Enter select  |  Letter = jump directly  |  q/Esc quit"
    };
    let help = Paragraph::new(hint)
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(help, chunks[2]);
}
