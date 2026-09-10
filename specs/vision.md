# quiteMyTempo – Vision

Übungs-App für Schlagzeuger (alle Skill-Level, Zielgruppe für den ersten Release: Fortgeschrittene).
Kernfunktionen: Metronom, Übungen aufnehmen & evaluieren, **Tightness Trainer**
(Timing-Abweichung zum Klick). Hobby-/Lernprojekt, kein Business-Ziel.

## Tightness Trainer – Definition (v1)
- Misst ausschließlich **Timing-Abweichung zum Klick** (nicht Dynamik/Lautstärke)
- Bewertungslogik: `deviation_ms = event.timestamp - nearest_click.timestamp`
- Aggregation über eine Übungssession: z.B. Ø-Abweichung, Standardabweichung, Trend

## Tastaturmodus (Keyboard Timing Trainer)

Permanentes User-Feature, **kein** Ersatz für das Drum-Practice mit Audio, sondern
eine eigenständige Übungsform: Timing-Grundgefühl schärfen, wenn kein Kit verfügbar ist
(z.B. unterwegs). Muss in der UI klar von "echtem" Drum-Practice-Modus abgegrenzt werden,
da der motorische Transfer (Tastendruck vs. Stick-Schlag) nicht belegt ist.

Umfasst mehrere Übungsmodi (Tap Along, Quiet Four, Tuplets) – Detailspezifikation siehe
[`specs/keyboard-modes.md`](./keyboard-modes.md).

Optionale Erweiterung: Mapping mehrerer Tasten auf unterschiedliche Gliedmaßen/Drums
(z.B. Leertaste = Kick, Buchstabe = Snare), um einfache Grooves/Patterns zu simulieren.

## Skill-Level-Abfrage
Aktuell **nur Vorbereitung** für spätere Ausbaustufen (Schwierigkeitsgrade,
Übungsauswahl). Für den ersten Release (Zielgruppe: Fortgeschrittene) nicht
im kritischen Pfad – nicht vorzeitig bauen. Siehe [`specs/roadmap.md`](./roadmap.md).
