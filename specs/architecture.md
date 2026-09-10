# quiteMyTempo – Architecture

## Status
- Phase: **Erster spielbarer Prototyp implementiert** (Tap Along, Terminal/TUI)
- Plattform: Desktop
- Tech-Stack: Rust (Lernprojekt), Cargo-Workspace mit zwei Crates:
  - `crates/core` (`quietmytempo-core`): reine, plattformunabhängige Logik
    (`Schedule`-Trait, `TimingEvaluator`, alle drei Modi). Kein I/O, 17 Unit-Tests.
  - `crates/app` (`quietmytempo-app`): I/O-Schicht (Audio via `cpal`, Tastatur via
    `crossterm`, TUI via `ratatui`). Aktuell nur Tap-Along-Modus verdrahtet.
- Details zur konkreten Implementierung (Module, Bugs, Design-Entscheidungen aus der
  Bauphase) siehe [`specs/implementation-notes.md`](./implementation-notes.md).

## Kernentscheidung: `TimingSource`-Abstraktion

Das größte technische Risiko ist die **Audio-Onset-Detection** (Schlagerkennung aus
Mikrofonsignal) – unvalidiert, potenziell schwierig (Polyphonie, Ghost Notes, Raumhall),
siehe [`specs/risks.md`](./risks.md).

Um dieses Risiko vom kritischen Pfad zu entkoppeln, wird die Timing-Bewertungslogik
("wie weit war der Schlag vom Klick entfernt?") unabhängig von der Input-Quelle gebaut.

```
                     ┌─────────────────────┐
                     │   TimingEvaluator     │
                     │  (Soll- vs. Ist-Zeit, │
                     │   Abweichung in ms)   │
                     └──────────▲───────────┘
                                │ TimingEvent { timestamp }
                     ┌──────────┴───────────┐
                     │     TimingSource      │  (trait/interface)
                     └──────────▲───────────┘
              ┌─────────────────┼─────────────────┐
   ┌──────────┴─────────┐ ┌─────┴──────┐  ┌────────┴─────────┐
   │  KeyboardSource     │ │ AudioSource │  │  MidiSource      │
   │  (Keydown-Events)   │ │ (Onset-     │  │  (später: E-Kit) │
   │  MVP – v1           │ │  Detection) │  │  langfristig      │
   └──────────────────────┘ │  v2, riskant│  └──────────────────┘
                             └─────────────┘
```

### Warum diese Reihenfolge?
1. **KeyboardSource (v1, MVP)**: Trivial zu implementieren (OS-Keydown-Event + Timestamp),
   niedrige und gut kontrollierbare Latenz. Ermöglicht, die komplette
   Metronom- + Bewertungslogik zu bauen und zu testen, **ohne** auf die Audio-Analyse
   angewiesen zu sein. **✅ Implementiert** (`crates/app/src/keyboard.rs`, via `crossterm`).
2. **AudioSource (v2, riskant)**: Erfordert vorab einen technischen Spike
   (siehe `specs/risks.md`), da Machbarkeit unvalidiert ist. **Noch nicht begonnen.**
3. **MidiSource (später)**: E-Drum-Kits liefern MIDI-Events direkt – deutlich
   zuverlässiger als Audio-Onset-Detection, aber setzt entsprechende Hardware
   beim Nutzer voraus. Langfristiges Ziel. **Noch nicht begonnen.**

> **Hinweis zur tatsächlichen Implementierung:** Die reale API in `crates/core`
> unterscheidet sich leicht von der grafischen Skizze oben (die "TimingSource" ist kein
> eigenes Trait, sondern die konkrete Kombination aus `Schedule`-Trait +
> `TimingEvaluator`, siehe `implementation-notes.md`). Das Diagramm bleibt als
> konzeptuelle Übersicht gültig; für die exakte API bitte `crates/core/src/` bzw.
> `implementation-notes.md` konsultieren.
