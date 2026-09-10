# quiteMyTempo – Keyboard Timing Trainer: Modi

Detailspezifikation der Übungsmodi für den Tastaturmodus (siehe `specs/architecture.md`
für die übergeordnete `TimingSource`-Architektur). Alle Modi nutzen dieselbe
`KeyboardSource` + `TimingEvaluator`-Pipeline, unterscheiden sich nur in:
- welche Klicks als "Soll-Zeitpunkte" zur Bewertung herangezogen werden
- wann/wie das Feedback dargestellt wird (live vs. verzögert)

## Gemeinsame Grundlagen

- **Input**: ein Tastendruck = ein `TimingEvent { timestamp }`
- **Referenz**: Metronom-Klick-Zeitpunkte werden vorab deterministisch berechnet
  (BPM-basiert), unabhängig davon ob der Klick hörbar ist oder nicht (relevant für
  "Quiet Four", siehe unten)
- **Abweichung**: `deviation_ms = event.timestamp - nearest_expected_beat.timestamp`
- Alle Modi sind konfigurierbar über **BPM** (Tempo)

---

## Modus 1: Tap Along

**Ziel**: Einfachstes Grundtraining – zum hörbaren Klick dazutappen, mit direktem
Live-Feedback.

### Ablauf
- Metronom läuft durchgehend (jeder Beat hörbar)
- Spieler drückt bei jedem Beat die Taste
- Jeder Tastendruck wird sofort gegen den nächstgelegenen Klick ausgewertet

### Evaluierung
- **Live**, pro Schlag: Abweichung in ms (z.B. "+8ms" = leicht zu spät, "-3ms" = leicht
  zu früh)
- **Grafisch**: Echtzeit-Visualisierung, z.B.
  - Laufender Zeitstrahl/Balkendiagramm mit Abweichung pro Schlag (Achse: früh/spät)
  - Oder ein sich bewegender Zeiger auf einer Skala (wie ein Tuner/VU-Meter), der bei
    jedem Klick zur Mitte zurückspringt
- Aggregiert über die Session: Ø-Abweichung, Konsistenz (Standardabweichung)

### Konfiguration
- BPM
- Optional: Dauer/Anzahl Takte

---

## Modus 2: Quiet Four

**Ziel**: Internes Timing-Gefühl trainieren – ohne die Krücke des durchgehenden
Klicks. Prüft, ob der Spieler den Puls "im Kopf" halten kann.

> Namensklärung: "Four" bezieht sich auf 4 Beats pro Takt (4/4-Takt als Standardfall),
> nicht auf die Anzahl der Takte im Zyklus. Ein Zyklus besteht aus 2 hörbaren + 1
> stillem Takt = 3 Takte total.

### Ablauf
- **2 Takte** mit hörbarem Klick: Spieler tappt normal mit (wie Tap Along)
- **1 Takt** ohne hörbaren Klick (still) – der interne Klick läuft aber unsichtbar
  im Hintergrund weiter (als Referenz-Zeitachse), Spieler tappt weiter, ohne
  akustische Führung
- Danach: nächster hörbarer Klick setzt wieder ein (Takt 4 / "die nächste 1")

### Evaluierung
- Fokus der Bewertung: **wie präzise wurde die 1 des nächsten hörbaren Takts
  getroffen**, nachdem der stille Takt durchgehalten wurde – dieser eine Tap
  zählt für das Session-Ergebnis
- Alle 4 Beats im stillen Takt werden erwartet und getappt; ihre Abweichungen
  werden **informativ** angezeigt (z.B. als Verlauf/Trend während des stillen
  Takts), fließen aber **nicht** in die Kernbewertung ein
- Darstellung: nach Abschluss eines Zyklus (2 Takte hörbar + 1 Takt still + Landung),
  Ergebnis anzeigen (ms-Abweichung auf der Landung, plus informativer Verlauf der
  4 stillen Beats), danach nächster Zyklus

### Konfiguration
- BPM
- Anzahl Beats pro Takt (z.B. 4/4 als Standard)
- Anzahl Wiederholungszyklen pro Session

---

## Modus 3: Tuplets

**Ziel**: Polyrhythmisches Timing – ungerade Unterteilungen (Triolen, Quintolen,
Septolen) präzise gegen einen geraden 4er-Klick tappen.

### Ablauf
- Metronom klickt einen geraden 4er-Grundpuls (z.B. 4 Klicks pro Takt)
- Spieler tappt eine gewählte Unterteilung **über den gesamten Takt** (4 Klicks),
  nicht nur über ein einzelnes Klick-Intervall:
  - **3-olen (Triolen)**: 3 gleich verteilte Taps über 4 Klicks (Grundfall: die
    klassische "Triole über 4 Viertel", d.h. 3 Taps in der Zeit von 4 Grundpuls-
    Schlägen)
  - **5-olen (Quintolen)**: 5 gleich verteilte Taps über 4 Klicks
  - **7-olen (Septolen)**: 7 gleich verteilte Taps über 4 Klicks
- Die erwarteten Tap-Zeitpunkte werden mathematisch berechnet:
  `expected_tap[i] = bar_start + i * (bar_duration / n)` für `i = 0..n-1`,
  wobei `bar_duration` die Dauer der 4 Grundpuls-Klicks (ein Takt) ist

### Evaluierung
- Pro Tap: Abweichung vom nächstgelegenen berechneten Unterteilungspunkt (ms)
- Zusätzlich relevant: **Gleichmäßigkeit der Unterteilung** (nicht nur Treffergenauigkeit
  auf den ersten Tap, sondern ob alle n Taps gleich weit verteilt sind) – d.h. neben
  Abweichung zum Soll auch Varianz der tatsächlichen Intervalle zwischen den eigenen Taps
- Grafisch: z.B. Kreis-/Kreissegment-Darstellung (n gleich große Segmente pro Klick-
  Intervall) mit Markierung, wo der tatsächliche Tap gelandet ist

### Konfiguration
- BPM (Grundpuls)
- Unterteilung: 3, 5 oder 7 (später ggf. erweiterbar/frei wählbar)
- Bezugsrahmen: ein ganzer Takt (4 Grundpuls-Klicks) als Standard für v1

---

## Architektur-Implikationen

Alle drei Modi benötigen eine **Referenz-Zeitachse**, die unabhängig vom hörbaren
Klick existiert (relevant besonders für Quiet Four und Tuplets). Das bedeutet:

- Der Metronom-Kern muss Klick-Zeitpunkte generieren können, **ob sie hörbar
  gemacht werden oder nicht** – Trennung von "Beat-Zeitplan" (Datenmodell) und
  "Audio-Ausgabe" (ob dieser Beat tatsächlich einen Sound triggert)
- `TimingEvaluator` muss pro Modus konfigurierbar sein, welche Zeitpunkte als Soll-Raster
  gelten (einfacher 4er-Grundpuls bei Tap Along/Quiet Four, vs. berechnetes n-tolen-Raster
  bei Tuplets)
- Gemeinsame Kernabstraktion (**tatsächlich implementiert** in `crates/core/src/schedule.rs`,
  leicht abweichend von der ursprünglichen Skizze):
  ```rust
  struct ClickEvent { at: Duration }               // tatsächlich hörbarer Klick
  struct ExpectedTap { at: Duration, informational: bool } // Soll-Zeitpunkt zur Bewertung
  trait Schedule {
      fn clicks(&self, session_duration: Duration) -> Vec<ClickEvent>;
      fn expected_taps(&self, session_duration: Duration) -> Vec<ExpectedTap>;
  }
  ```
  Mit modusspezifischen Implementierungen (alle implementiert + getestet):
  `TapAlongSchedule`, `QuietFourSchedule`, `TupletSchedule`.

## Implementierungsstatus
- **Core-Logik (`crates/core`)**: Alle drei Modi implementiert und getestet (17 Unit-Tests).
- **App-Verdrahtung (`crates/app`)**: Bisher nur **Tap Along** ist in `main.rs` an
  Metronom-Audio + Tastatur-Input + TUI angeschlossen. Quiet Four und Tuplets existieren
  als fertige `Schedule`-Implementierungen, aber ohne App-seitige Anbindung/Moduswahl.
- Details zu konkreten Implementierungsentscheidungen (Toleranzfenster im Evaluator,
  Latenz-Kalibrierung, TUI-Aufbau) siehe [`specs/implementation-notes.md`](./implementation-notes.md).

## Offene Punkte für später
- UI/UX-Feinschliff der grafischen Live-Darstellung (siehe Tap Along)
- Schwierigkeitsgrade/Skill-Level-Anbindung (später, siehe `vision.md`/`roadmap.md`)
