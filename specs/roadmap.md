# quiteMyTempo – Roadmap

## Nächste Schritte
1. ~~Rust-Projekt-Setup (Cargo-Workspace), Audio-Crate-Wahl für Metronom-Klick-Ausgabe~~ ✅ erledigt
2. ~~`TimingSource`-Trait definieren + `KeyboardSource`-Implementierung~~ ✅ erledigt
3. ~~`TimingEvaluator` (Kernlogik: Abweichungsberechnung, Aggregation)~~ ✅ erledigt
4. ~~Metronom-Engine (präzises Timing, unabhängig vom Event-Loop-Jitter)~~ ✅ erledigt
5. ~~Minimal-UI zur Anzeige von Klick + Tastatur-Feedback + Statistik~~ ✅ erledigt (ratatui-TUI)
6. Quiet Four / Tuplet Modi in die App verdrahten (Logik existiert bereits in `core`,
   `main.rs` fährt bisher nur Tap Along)
7. Onset-Detection-Spike für `AudioSource` (weiterhin offen, größtes verbleibendes Risiko,
   siehe [`specs/risks.md`](./risks.md))

## Später (nicht im kritischen Pfad)
- Skill-Level-Abfrage / Schwierigkeitsgrade (siehe [`specs/vision.md`](./vision.md))
- AudioSource, MidiSource (siehe [`specs/architecture.md`](./architecture.md))
