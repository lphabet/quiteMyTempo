# quiteMyTempo – Architecture

## Vision
Übungs-App für Schlagzeuger (alle Skill-Level, Zielgruppe für den ersten Release: Fortgeschrittene).
Kernfunktionen: Metronom, Übungen aufnehmen & evaluieren, **Tightness Trainer**
(Timing-Abweichung zum Klick). Hobby-/Lernprojekt, kein Business-Ziel.

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
Mikrofonsignal) – unvalidiert, potenziell schwierig (Polyphonie, Ghost Notes, Raumhall).

Um dieses Risiko vom kritischen Pfad zu entkoppeln, wird die Timing-Bewertungslogik
(„wie weit war der Schlag vom Klick entfernt?") unabhängig von der Input-Quelle gebaut.

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
   (siehe unten), da Machbarkeit unvalidiert ist. **Noch nicht begonnen.**
3. **MidiSource (später)**: E-Drum-Kits liefern MIDI-Events direkt – deutlich
   zuverlässiger als Audio-Onset-Detection, aber setzt entsprechende Hardware
   beim Nutzer voraus. Langfristiges Ziel. **Noch nicht begonnen.**

> **Hinweis zur tatsächlichen Implementierung:** Die reale API in `crates/core`
> unterscheidet sich leicht von der grafischen Skizze oben (die "TimingSource" ist kein
> eigenes Trait, sondern die konkrete Kombination aus `Schedule`-Trait +
> `TimingEvaluator`, siehe `implementation-notes.md`). Das Diagramm bleibt als
> konzeptuelle Übersicht gültig; für die exakte API bitte `crates/core/src/` bzw.
> `implementation-notes.md` konsultieren.

## Tastaturmodus (Keyboard Timing Trainer)

Permanentes User-Feature, **kein** Ersatz für das Drum-Practice mit Audio, sondern
eine eigenständige Übungsform: Timing-Grundgefühl schärfen, wenn kein Kit verfügbar ist
(z.B. unterwegs). Muss in der UI klar von "echtem" Drum-Practice-Modus abgegrenzt werden,
da der motorische Transfer (Tastendruck vs. Stick-Schlag) nicht belegt ist.

Umfasst mehrere Übungsmodi (Tap Along, Quiet Four, Tuplets) – Detailspezifikation siehe
[`specs/keyboard-modes.md`](./keyboard-modes.md).

Optionale Erweiterung: Mapping mehrerer Tasten auf unterschiedliche Gliedmaßen/Drums
(z.B. Leertaste = Kick, Buchstabe = Snare), um einfache Grooves/Patterns zu simulieren.

## Tightness Trainer – Definition (v1)
- Misst ausschließlich **Timing-Abweichung zum Klick** (nicht Dynamik/Lautstärke)
- Bewertungslogik: `deviation_ms = event.timestamp - nearest_click.timestamp`
- Aggregation über eine Übungssession: z.B. Ø-Abweichung, Standardabweichung, Trend

## Skill-Level-Abfrage
- Aktuell **nur Vorbereitung** für spätere Ausbaustufen (Schwierigkeitsgrade,
  Übungsauswahl). Für den ersten Release (Zielgruppe: Fortgeschrittene) nicht
  im kritischen Pfad – nicht vorzeitig bauen.

## Offene Risiken

| Risiko | Status | Nächster Schritt |
|---|---|---|
| Audio-Onset-Detection technisch unvalidiert | offen | 1-Tages-Spike: Snare-Aufnahmen (versch. Lautstärken/Räume) durch Onset-Detection-Lib (z.B. Rust: `aubio`-Bindings oder eigene Onset-Erkennung via FFT-Energie) laufen lassen, Ziel: <10-15ms Abweichung, keine False Positives |
| Audio-Output-Latenz / Tastatur-Jitter verzerrt Timing-Bewertung | **✅ gelöst (v1)** | Visuelle Latenz-Kalibrierung vor jeder Session (konvergierende Balken, siehe `implementation-notes.md`) misst Offset empirisch, wird auf alle Taps angewendet |
| Motorischer Transfer Tastatur → Drumstick unbelegt | akzeptiertes Risiko | Klar als eigenständige Übungsform kommunizieren, keine Transfer-Behauptung |
| Kein Vergleich zu bestehenden Lösungen (Melodics, Drumeo, etc.) | für Hobbyprojekt unkritisch | optional später nachholen |
| Doppel-Tap konnte Bewertung für den Rest der Session verzerren | **✅ gefixt** | Toleranzfenster in `TimingEvaluator` (siehe `implementation-notes.md`) |

## Nächste Schritte
1. ~~Rust-Projekt-Setup (Cargo-Workspace), Audio-Crate-Wahl für Metronom-Klick-Ausgabe~~ ✅ erledigt
2. ~~`TimingSource`-Trait definieren + `KeyboardSource`-Implementierung~~ ✅ erledigt
3. ~~`TimingEvaluator` (Kernlogik: Abweichungsberechnung, Aggregation)~~ ✅ erledigt
4. ~~Metronom-Engine (präzises Timing, unabhängig vom Event-Loop-Jitter)~~ ✅ erledigt
5. ~~Minimal-UI zur Anzeige von Klick + Tastatur-Feedback + Statistik~~ ✅ erledigt (ratatui-TUI)
6. Quiet Four / Tuplet Modi in die App verdrahten (Logik existiert bereits in `core`,
   `main.rs` fährt bisher nur Tap Along)
7. Onset-Detection-Spike für `AudioSource` (weiterhin offen, größtes verbleibendes Risiko)
