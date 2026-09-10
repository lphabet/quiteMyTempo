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
- Während des stillen Takts wird zusätzlich zum Klick auch die "Position im
  Takt"-Anzeige (Beat-Zähler, siehe `implementation-notes.md` → TUI) ausgeblendet
  — sonst hätte der Spieler weiterhin eine visuelle Pulsreferenz und der Sinn des
  Modus (rein internes Timing-Gefühl ohne jede externe Krücke) wäre unterlaufen.

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

## Modus 4: Rhythm Reader

**Ziel**: Notenlesen mit sofortiger Timing-Rückmeldung verbinden — der Spieler sieht
einen kurzen Rhythmus als Notenwerte (Viertel, Achtel, Sechzehntel, Pausen) und muss
ihn auf der Leertaste **nachspielen**. Anders als Tap Along/Quiet Four/Tuplets ist der
Soll-Rhythmus hier **nicht gleichmäßig** (kein einfacher fester Puls), sondern ein
konkretes, vorab generiertes oder festes Pattern — der Spieler übt dadurch parallel
Notation-Verständnis und Timing.

### Ablauf
1. **Anzeige-Phase**: Ein Notenpattern (z.B. 1 Takt, 4/4) wird als Notenwerte in der
   TUI dargestellt (siehe Darstellung unten). Ein hörbarer **Count-in** (z.B. 1 Takt
   im gleichen Tempo, reiner Klick ohne Noten) gibt dem Spieler die Tempo-Referenz,
   bevor der eigentliche Rhythmus "dran" ist — ohne Count-in wüsste der Spieler nicht,
   wo Beat 1 des Patterns liegt.
2. **Spiel-Phase**: Ab dem Ende des Count-ins muss der Spieler jede Note im
   angezeigten Pattern zur richtigen Zeit per Leertaste treffen. Pausen (Rests)
   erfordern **keinen** Tastendruck — ein Tastendruck während einer Pause (statt
   während einer erwarteten Note) zählt als Fehltreffer (siehe Evaluierung).
3. **Pro-Note-Feedback**: Sobald eine Note "dran" ist (ihr Soll-Zeitpunkt erreicht/
   vorbei ist) und eine Bewertung feststeht, wird die Note in der Anzeige einfärbt:
   - **Grün**: Tap innerhalb der Toleranz getroffen
   - **Rot**: kein Tap in der Toleranz erfolgt (verpasst) ODER ein Tap fiel auf eine
     Pause/eine bereits verbrauchte Note (Fehltreffer)
   - Ungespielte, noch kommende Noten bleiben neutral (Standardfarbe), bereits
     korrekt gespielte bleiben dauerhaft grün sichtbar, bis das Pattern komplett
     durchlaufen ist
4. Nach Abschluss eines Patterndurchlaufs: kurze Pause, danach automatisch der
   **nächste Durchlauf** (gleiches oder neu generiertes Pattern, je Konfiguration —
   siehe unten), bis die konfigurierte Anzahl an Durchläufen erreicht ist.
5. **Result Screen** nach dem letzten Durchlauf (siehe Darstellung unten).

### Notenwerte (v1)
- Viertel (♩), Achtel (♪), Sechzehntel (𝅘𝅥𝅯), punktierte Viertel, sowie die
  entsprechenden Pausen (Viertel-, Achtel-Pause). Triolen/ungerade Unterteilungen
  sind explizit **nicht** Teil dieses Modus (dafür existiert bereits Modus 3:
  Tuplets) — Rhythm Reader bleibt auf gerade Unterteilungen von 4/4 fokussiert, um
  Notation und Timing nicht gleichzeitig mit polyrhythmischer Komplexität zu
  überfrachten.
- Ein Pattern ist eine Sequenz von `(NoteValue, is_rest: bool)`-Einträgen, deren
  Dauern sich exakt zu ganzen Takten (Vielfachen von 4 Vierteln) aufsummieren.

### Pattern-Erzeugung
- **v1**: Eine kleine, feste Bibliothek kuratierter 1-Takt-Patterns (z.B. 8–12
  Patterns unterschiedlicher Schwierigkeit: von "4 Viertel" bis Mischungen aus
  Achteln/Sechzehnteln/Pausen), zufällig gezogen für jeden Durchlauf einer Session
  (ohne unmittelbare Wiederholung des direkten Vorgängers, damit es nicht "das
  gleiche Pattern zweimal in Folge" gibt).
- **Später** (nicht v1): algorithmische Zufallsgenerierung von Patterns mit
  einstellbarem Schwierigkeitsgrad (Notenwert-Pool, max. Pausen-Anteil) — siehe
  `specs/roadmap.md`.

### Darstellung (TUI)
- Da klassische Notenlinien im Terminal keinen sinnvollen Mehrwert gegenüber einer
  einfachen symbolischen Reihe bieten, wird das Pattern als **horizontale Reihe von
  Notensymbolen** dargestellt (kein Notenlinien-/Notenschlüssel-Rendering in v1):
  - Jedes Symbol repräsentiert eine Note/Pause, proportional zur Dauer breiter
    dargestellt (z.B. eine Sechzehntel schmaler als eine Viertel), analog zum
    horizontalen Zeitstrahl-Konzept aus Tap Along
  - Ein vertikaler "Playhead"-Marker läuft während der Spiel-Phase in Echtzeit durch
    diese Reihe (gleiche Zeitachse wie die erwarteten Tap-Zeitpunkte), damit der
    Spieler auch ohne Notenlesen-Erfahrung rein visuell die Position im Takt sehen
    kann
  - Farbe pro Symbol: neutral (noch nicht dran) → grün/rot (siehe Evaluierung) nach
    Bewertung

### Evaluierung
- Jede Nicht-Pause-Note ist ein `ExpectedTap` (scored, keine informational-Taps in
  diesem Modus — anders als Quiet Four gibt es hier keine "nur informative"
  Zwischenbewertung)
- Toleranzfenster analog zu den anderen Modi: proportional zur kürzesten im Pattern
  vorkommenden Notendauer (z.B. halbe kürzeste Notendauer), nicht zum Takt/Beat wie
  bei Tap Along — sonst wären Sechzehntel-Läufe nicht sinnvoll unterscheidbar
  bewertbar
- Ein Tastendruck, der keiner offenen erwarteten Note zugeordnet werden kann
  (außerhalb jeder Toleranz, z.B. während einer Pause), wird als **Fehltreffer**
  gezählt und fließt separat (nicht als Timing-Abweichung, sondern als eigener
  Zähler "falsche Anschläge") in die Session-Statistik ein
- Eine erwartete Note, die nicht innerhalb ihres Toleranzfensters getappt wurde, gilt
  als **verpasst** (separater Zähler "verpasste Noten") — anders als bei Tap Along
  gibt es hier kein "einfach den nächsten Beat nehmen", weil das Pattern selbst nicht
  gleichmäßig ist
- Session-Aggregation: Ø-Abweichung + Konsistenz (wie bei den anderen Modi, nur über
  tatsächlich getroffene Noten), plus Trefferquote (getroffen / (getroffen + verpasst))
  und Anzahl Fehltreffer

### Result Screen
Zusätzlich zu den bereits etablierten Result-Screen-Elementen (Grade, Ø-Abweichung,
Konsistenz-Gauge, siehe `implementation-notes.md` → TUI) zeigt der Rhythm-Reader-
Result-Screen:
- Trefferquote (getroffene / gesamt erwartete Noten, über alle Durchläufe)
- Anzahl Fehltreffer (Taps während Pausen / ohne zugehörige Note)
- Anzahl verpasster Noten

### Konfiguration
- BPM
- Anzahl Durchläufe pro Session
- (Später, nicht v1) Schwierigkeitsgrad-Auswahl, sobald algorithmische
  Pattern-Generierung existiert

---

## Architektur-Implikationen

Alle vier Modi benötigen eine **Referenz-Zeitachse**, die unabhängig vom hörbaren
Klick existiert (relevant besonders für Quiet Four, Tuplets und Rhythm Reader). Das
bedeutet:

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
  `TapAlongSchedule`, `QuietFourSchedule`, `TupletSchedule`. Rhythm Reader benötigt
  zusätzlich eine `RhythmReaderSchedule`, die aus einem `Pattern` (Sequenz aus
  `(NoteValue, is_rest)`) die `ExpectedTap`-Liste ableitet, sowie einen kleinen
  `NoteValue`-Typ (Dauer als Bruchteil eines Viertels) — siehe
  `implementation-notes.md` für die konkrete API, sobald umgesetzt.

## Implementierungsstatus
- **Core-Logik (`crates/core`)**: Alle vier Modi implementiert und getestet
  (Tap Along, Quiet Four, Tuplets, Rhythm Reader).
- **App-Verdrahtung (`crates/app`)**: Alle vier Modi sind über den Startscreen
  (siehe [`specs/app-flow.md`](./app-flow.md)) auswählbar und an Metronom-Audio +
  Tastatur-Input + TUI angeschlossen.
- Details zu konkreten Implementierungsentscheidungen (Toleranzfenster im Evaluator,
  Latenz-Kalibrierung, TUI-Aufbau) siehe [`specs/implementation-notes.md`](./implementation-notes.md).

## Offene Punkte für später
- UI/UX-Feinschliff der grafischen Live-Darstellung (siehe Tap Along)
- Schwierigkeitsgrade/Skill-Level-Anbindung (später, siehe `vision.md`/`roadmap.md`)
- Rhythm Reader: algorithmische Pattern-Generierung mit Schwierigkeitsgraden (bisher
  feste kuratierte Pattern-Bibliothek, siehe Modus 4 oben)
