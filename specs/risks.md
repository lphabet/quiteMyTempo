# quiteMyTempo – Offene Risiken

| Risiko | Status | Nächster Schritt |
|---|---|---|
| Audio-Onset-Detection technisch unvalidiert | offen | 1-Tages-Spike: Snare-Aufnahmen (versch. Lautstärken/Räume) durch Onset-Detection-Lib (z.B. Rust: `aubio`-Bindings oder eigene Onset-Erkennung via FFT-Energie) laufen lassen, Ziel: <10-15ms Abweichung, keine False Positives |
| Audio-Output-Latenz / Tastatur-Jitter verzerrt Timing-Bewertung | **✅ gelöst (v1)** | Visuelle Latenz-Kalibrierung vor jeder Session (konvergierende Balken, siehe `implementation-notes.md`) misst Offset empirisch, wird auf alle Taps angewendet |
| Motorischer Transfer Tastatur → Drumstick unbelegt | akzeptiertes Risiko | Klar als eigenständige Übungsform kommunizieren, keine Transfer-Behauptung |
| Kein Vergleich zu bestehenden Lösungen (Melodics, Drumeo, etc.) | für Hobbyprojekt unkritisch | optional später nachholen |
| Doppel-Tap konnte Bewertung für den Rest der Session verzerren | **✅ gefixt** | Toleranzfenster in `TimingEvaluator` (siehe `implementation-notes.md`) |

Das größte technische Risiko ist die **Audio-Onset-Detection** (Schlagerkennung aus
Mikrofonsignal) – unvalidiert, potenziell schwierig (Polyphonie, Ghost Notes, Raumhall).
Diese Unsicherheit ist der Grund für die `TimingSource`-Abstraktion in
[`specs/architecture.md`](./architecture.md), die dieses Risiko vom kritischen Pfad
entkoppelt.
