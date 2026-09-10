# quiteMyTempo – Implementation Notes

Ergänzt `specs/architecture.md` und `specs/keyboard-modes.md` um konkrete
Implementierungsentscheidungen, gefundene Bugs und deren Fixes aus der ersten
Bauphase (Rust-Prototyp, Tap-Along-Modus). Dient als Gedächtnis für "warum ist das so,
wie es ist", nicht als vollständige API-Doku (dafür: Doc-Kommentare im Code selbst).

## Projektstruktur

```
Cargo.toml                 # Workspace-Root
crates/
  core/                     # quietmytempo-core: reine Logik, kein I/O
    src/
      lib.rs
      schedule.rs           # Schedule-Trait, ClickEvent, ExpectedTap
      evaluator.rs           # TimingEvaluator, TapResult, SessionSummary
      schedules/
        tap_along.rs
        quiet_four.rs
        tuplet.rs
  app/                      # quietmytempo-app: I/O-Schicht, Terminal-Binary
    src/
      main.rs
      clock.rs               # SessionClock (+ rebase zwischen Phasen)
      keyboard.rs             # Tastatur-Input via crossterm
      metronome.rs            # Audio-Ausgabe via cpal
      calibration.rs           # Visuelle Latenz-Kalibrierung
      terminal.rs               # Raw-Mode-Guard
      ui.rs                      # ratatui-Rendering (Tap-Along-Live-UI)
```

## `crates/core`: Design-Entscheidung `ClickEvent` vs. `ExpectedTap`

Die ursprüngliche Skizze in `keyboard-modes.md` (`ExpectedBeat { timestamp, audible }`)
wurde beim Implementieren zu zwei separaten Typen verfeinert:

- `ClickEvent { at }` — was tatsächlich als Audio abgespielt wird
- `ExpectedTap { at, informational }` — was zur Bewertung herangezogen wird

Grund: Bei Tuplets klicken die 4 Grounding-Beats hörbar, aber es werden n
(3/5/7) andere Zeitpunkte bewertet — Anzahl und Zeitpunkte von "gehört" und
"bewertet" divergieren komplett. Ein einzelner `ExpectedBeat`-Typ mit nur einem
`audible`-Flag hätte das nicht abbilden können, da er 1:1 zwischen Klick und
Bewertungspunkt hätte gelten müssen.

## Bug: Doppel-Tap hat Bewertung für den Rest der Session verzerrt

**Symptom** (aus manuellem Test): Ein versehentlicher Doppel-Tap führte dazu, dass alle
nachfolgenden Taps für den Rest der Session um mehrere hundert ms danebenlagen.

**Ursache**: `TimingEvaluator::record()` matchte jeden eingehenden Tap gegen den
nächstgelegenen *noch unverbrauchten* Soll-Zeitpunkt — unabhängig davon, wie weit
dieser tatsächlich entfernt war. Ein Doppel-Tap "verbrauchte" einen weit in der Zukunft
liegenden Slot; der eigentliche, korrekte Tap für diesen Beat hatte danach nichts mehr
zum Matchen und wich auf den übernächsten Slot aus — ein Fehler, der sich für den Rest
der Session propagierte.

**Fix**: `TimingEvaluator::new()` nimmt jetzt einen `tolerance: Duration`-Parameter. Ein
Tap matched nur, wenn er innerhalb dieser Toleranz zum nächsten unverbrauchten
Soll-Zeitpunkt liegt; andernfalls wird er verworfen (`record()` gibt `None` zurück)
statt einen fernen Slot zu konsumieren. In der App (`main.rs`) wird die Toleranz als
halber Beat-Abstand gewählt (`beat_duration / 2`).

Regressionstests: `evaluator.rs::tests::double_tap_does_not_derail_subsequent_matches`,
`tap_far_outside_tolerance_is_rejected_even_if_it_is_the_closest`.

## Audio/visuelle Latenz: Kalibrierung statt "Fixen"

**Beobachtung aus manuellem Test**: Audio (Metronom-Klick) und visuelles Feedback
liefen spürbar nicht synchron.

**Einordnung**: Perfekte Synchronität ist physikalisch nicht erreichbar — Audio-Output-
Hardware hat immer Puffer-Latenz (Größenordnung 10–100+ ms, je nach Backend/Treiber).
Jedes Rhythmusspiel (osu!, Guitar Hero, Beat Saber) löst das nicht durch Elimination,
sondern durch **Messen + Kompensieren**.

**Umgesetzte Lösung** (`crates/app/src/calibration.rs`):
- Vor jeder Session läuft eine **rein visuelle** Kalibrierungsphase (bewusst **ohne
  Audio** — isoliert visuelle Wahrnehmungs-/Eingabe-Latenz von der Frage der
  Audio-Output-Latenz, die separat und nur informativ angezeigt wird)
- Zwei Balken laufen vom linken/rechten Bildschirmrand nach innen zusammen
  (`CONVERGE_DURATION` = 1.8s, bewusst **langsamer** als die Session-BPM — Kalibrierung
  soll ruhige Präzision messen, nicht Tempo)
- 5 Runden (`CALIBRATION_ROUNDS`), Nutzer drückt Space beim Treffen der Balken
- Durchschnittliche Abweichung wird als `offset_ms` gespeichert
- Danach: "Ready"-Screen mit gemessenem Offset, Bestätigung per Space startet die
  eigentliche Session
- Der gemessene Offset wird auf alle Live-Tap-Timestamps angewendet
  (`calibration::apply_offset`), bevor sie an den `TimingEvaluator` gehen
- Zusätzlich (informativ, fließt NICHT in den Offset ein): `cpal`-Callback-Timestamps
  (`OutputCallbackInfo::timestamp()`) liefern eine Geräte-Latenz-Schätzung
  (`metronome::LatencyEstimate`), wird im UI-Header separat angezeigt

**Wichtige Nebenkonsequenz — mehrere Zeit-Anker im Prozess**: Kalibrierungsrunden und
die Hauptsession starten jeweils ihre eigene lokale `SessionClock` (unterschiedliche
Nullpunkte). Der Tastatur-Thread läuft aber nur einmal für den ganzen Prozess und
timestampt gegen einen einzigen globalen `SessionClock`. Ohne Umrechnung hätten
Tap-Zeitstempel sich auf falsche Nullpunkte bezogen. Lösung: `SessionClock::rebase()`
rechnet einen Zeitstempel aus dem globalen Clock in den lokalen Zeitraum einer
beliebigen Phase (Kalibrierungsrunde, Hauptsession) um.

## TUI (`ratatui`)

- Terminal läuft im Alternate-Screen + Raw-Mode für die komplette Programmlaufzeit
  (Kalibrierung UND Hauptsession), nicht nur für die Hauptsession — nötig, weil die
  Kalibrierung selbst eine Render-Loop mit Live-Grafik braucht
- Metronom-Panel "leuchtet" kurz auf (`FLASH_DURATION` = 90ms) bei jedem tatsächlichen
  Klick — Zeitpunkt wird aus der vorab berechneten Klick-Liste abgeleitet, nicht über
  einen Audio-Callback-Channel (Schedules sind pure/deterministisch, siehe `core`)
- Tap-Historie zeigt die letzten 10 Taps mit farbcodiertem Abweichungs-Meter
  (grün ≤15ms, gelb ≤40ms, rot darüber)
- Render-Loop läuft unabhängig vom Audio-/Input-Timing bei ~30 FPS
  (`FRAME_INTERVAL`); Präzision der Bewertung hängt nur an den Event-Timestamps,
  nicht am Render-Takt

## Dependencies (`crates/app`)

- `cpal` — Low-Latency-Audio-Output, liefert auch Callback-Timestamps für
  Latenz-Schätzung
- `crossterm` — Tastatur-Input (raw mode, Key-Events) + Terminal-Steuerung
- `ratatui` (mit `default-features = false, features = ["crossterm"]` — bewusst
  reduziert, da `all-widgets` unnötige Abhängigkeiten wie Farb-/Kalender-Libs zieht)
- `anyhow` — Fehlerbehandlung

## Offene Punkte (Stand Ende dieser Bauphase)

- Quiet Four / Tuplets: Core-Logik fertig + getestet, aber nicht in `main.rs`
  verdrahtet (kein Moduswahl-Screen in der App)
- Kein Retry/Redo für einzelne Kalibrierungsrunden (bei komplett verpasstem Tap greift
  ein Fallback-Strafwert, siehe `calibration::run_round`)
- Onset-Detection-Spike (`AudioSource`) weiterhin nicht begonnen — größtes
  verbleibendes technisches Risiko laut `risks.md`
