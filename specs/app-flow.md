# quiteMyTempo – App-Flow: Startscreen, Menü, Kalibrierungs-Gate

Ergänzt `specs/keyboard-modes.md` (Detailspezifikation der einzelnen Übungsmodi) um
den **Rahmen**, in dem diese Modi ausgewählt und gestartet werden. Bisher (`main.rs`
vor diesem Feature) gab es keinen Menü-Screen: Calibration lief immer einmalig beim
Programmstart, danach direkt in den (einzigen verdrahteten) Tap-Along-Modus.

## Ziele

1. Ein **Startscreen mit Menüauswahl**, über den:
   - die verschiedenen Übungsmodi aufgerufen werden können (Tap Along, Quiet Four,
     Tuplets, Rhythm Reader — siehe `keyboard-modes.md`)
   - die Kalibrierung (erneut) aufgerufen werden kann
   - das Programm beendet werden kann
2. Modi sind **erst auswählbar, nachdem einmal kalibriert wurde** — vorher sind sie
   sichtbar, aber gesperrt (mit Hinweistext), um dem Nutzer zu zeigen, was ihn erwartet,
   ohne dass er versehentlich unkalibriert eine Session startet (siehe
   `specs/risks.md` — Latenzkompensation ist kein optionales Detail, sondern
   Voraussetzung für sinnvolle Bewertung).
3. Die Kalibrierung muss **nicht mehr bei jedem Programmstart** wiederholt werden
   (Persistenz über Prozessstarts hinweg), aber jederzeit über das Menü **manuell
   erneut** ausgeführt werden können (z.B. nach Wechsel der Audio-Hardware).

## Persistenz der Kalibrierung

- Nach erfolgreicher Kalibrierung (`calibration::run` liefert
  `CalibrationOutcome::Measured`) wird das Ergebnis in eine kleine JSON-Datei im
  plattformüblichen Config-Verzeichnis geschrieben (z.B.
  `%APPDATA%/quietmytempo/calibration.json` unter Windows, äquivalent über
  `dirs`-Crate plattformunabhängig aufgelöst).
- Inhalt: `{ "offset_ms": i64 }`. Bewusst minimal — kein Ablaufdatum, keine
  Geräte-Fingerprints (v1-Scope, siehe `implementation-notes.md` für spätere
  Erweiterungsideen).
- Beim Programmstart wird versucht, diese Datei zu laden:
  - **Vorhanden & lesbar** → Menü startet mit entsperrten Modi, gespeicherter
    `offset_ms` wird verwendet, keine Kalibrierung nötig.
  - **Fehlt oder korrupt** → Menü startet mit gesperrten Modi; einzige verfügbare
    Aktion (neben Beenden) ist "Kalibrierung starten".
- Erfolgreiche erneute Kalibrierung über das Menü überschreibt die Datei und den
  In-Memory-Offset für die laufende Prozess-Session.

## Startscreen / Hauptmenü

### Einträge (in dieser Reihenfolge)
1. Tap Along
2. Quiet Four
3. Tuplets
4. Rhythm Reader
5. Kalibrierung (erneut) durchführen
6. Beenden

### Zustände pro Eintrag
- **Kalibrierung durchführen** und **Beenden**: immer aktiv.
- **Modi 1–4**: aktiv nur wenn eine gültige Kalibrierung vorliegt (persistiert
  oder in dieser Prozess-Session neu durchgeführt). Ist keine Kalibrierung
  vorhanden, werden die Einträge **sichtbar, aber ausgegraut** dargestellt, mit
  einem Hinweis (z.B. "Bitte zuerst kalibrieren") am unteren Bildschirmrand,
  wenn ein gesperrter Eintrag fokussiert ist.

### Navigation
- **Pfeiltasten Hoch/Runter** (oder `j`/`k`): Fokus zwischen Einträgen bewegen,
  umlaufend (am letzten Eintrag → Runter springt zurück zum ersten).
- **Enter**: fokussierten Eintrag auswählen (bei gesperrtem Eintrag: keine Aktion,
  ggf. kurzes visuelles Feedback "gesperrt").
- **q / Esc**: Programm beenden (direkt aus dem Menü, kein Bestätigungsdialog —
  Hobbyprojekt, geringe Kosten eines Fehlklicks).

### Nach einer Session (Result Screen bestehender/neuer Modi)
Bisher (`main.rs` vor diesem Feature) führte `q`/`Esc` auf dem Result-Screen zum
Programmende. Das wird geändert, damit das Menü tatsächlich ein Hub ist:
- **SPACE**: Modus mit gleicher Konfiguration neu starten (bestehendes Verhalten).
- **`m`**: zurück ins Hauptmenü (neu).
- **q / Esc**: Programm beenden (bestehendes Verhalten bleibt zusätzlich erhalten).

## Ablaufdiagramm (vereinfacht)

```
Programmstart
   │
   ▼
Kalibrierung von Disk laden ──── vorhanden ───► Hauptmenü (Modi entsperrt)
   │
   └── fehlt/korrupt ─────────────────────────► Hauptmenü (Modi gesperrt,
                                                  nur Kalibrierung/Beenden aktiv)

Hauptmenü
   │  Enter auf "Kalibrierung"
   ▼
Kalibrierungs-Flow (siehe implementation-notes.md, unverändert)
   │  Measured → offset speichern (Disk + Memory), Modi entsperren
   ▼
Hauptmenü (Modi entsperrt)
   │  Enter auf einem entsperrten Modus
   ▼
Session-Loop des gewählten Modus (siehe keyboard-modes.md)
   │
   ▼
Result Screen
   │  SPACE → Session-Loop (neu)   |   m → Hauptmenü   |   q/Esc → Beenden
```

## Architektur-Implikationen

- Neues App-Modul `menu.rs`: reine Render- + Input-Loop-Logik für den
  Startscreen, analog zu `calibration.rs`/`ui.rs` — kennt nichts über
  Metronom/Evaluator, nur über die Liste der Modi und deren Gesperrt-Status.
- Neues App-Modul (in `calibration.rs` ergänzt oder eigenes `settings.rs`) für
  das Laden/Speichern der `calibration.json`.
- `main.rs` wird zu einer Top-Level-Schleife: Menü → (Kalibrierung |
  Modus-Session) → zurück zu Menü, statt der bisherigen linearen
  Kalibrierung-dann-Tap-Along-Pipeline.
- `keyboard.rs` muss um Navigations-Signale erweitert werden (Pfeiltasten/`j`/`k`
  hoch/runter, Enter) — bisher kannte es nur `Tap` (Space) und `Quit`
  (Esc/q/Ctrl+C).

## Offene Punkte für später
- Kein Bestätigungsdialog beim Beenden (bewusst, siehe oben) — falls das im
  echten Gebrauch nervt, später ergänzen.
- Keine Persistenz von Session-Historie/Fortschritt über Modi hinweg (out of
  scope für dieses Feature, ggf. eigenständiges späteres Feature).
