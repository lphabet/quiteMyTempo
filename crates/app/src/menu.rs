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

    fn label(&self) -> &'static str {
        match self {
            MenuEntry::TapAlong => "Tap Along",
            MenuEntry::QuietFour => "Quiet Four",
            MenuEntry::Tuplets => "Tuplets",
            MenuEntry::RhythmReader => "Rhythm Reader",
            MenuEntry::Calibrate => "Kalibrierung (erneut) durchfuehren",
            MenuEntry::Quit => "Beenden",
        }
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
                if entry == MenuEntry::Quit {
                    return Ok(MenuOutcome::Quit);
                }
                if !entry.requires_calibration() || is_calibrated {
                    return Ok(MenuOutcome::Selected(entry));
                }
                // Locked entry: ignore selection, stay in the menu (see
                // module docs — no popup, just no-op).
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
        let suffix = if locked { "  (gesperrt)" } else { "" };

        let style = if is_focused {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        lines.push(Line::from(Span::styled(
            format!("{marker}{}{suffix}", entry.label()),
            style,
        )));
    }

    let menu = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(menu, chunks[1]);

    let focused_entry = MenuEntry::ALL[focused];
    let hint = if focused_entry.requires_calibration() && !is_calibrated {
        "Bitte zuerst kalibrieren, um diesen Modus freizuschalten."
    } else {
        "Hoch/Runter (oder j/k) navigieren  |  Enter waehlen  |  q/Esc beenden"
    };
    let help = Paragraph::new(hint)
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(help, chunks[2]);
}
