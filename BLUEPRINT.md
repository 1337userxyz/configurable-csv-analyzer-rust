# BLUEPRINT: Configurable CSV & ODS Analyzer (Rust + Slint)

## 1. Vision & Leitplanken
Ziel ist ein performantes Desktop-Tool in **Rust** mit **Slint UI** zur flexiblen Analyse, Validierung, Verknüpfung und Bearbeitung tabellarischer Daten.
- **Fokus:** CSV-Dateien und LibreOffice Calc (`.ods`). Excel wird vorerst nicht unterstützt.
- **Design-Philosophie:** Schlank, reaktionsschnell, tastaturbedienbar, erweiterbare Regel-Engine, saubere Trennung von Kernlogik (`core`) und Benutzeroberfläche (`ui`).
- **Freiheit:** Implementierungsdetails (z. B. Datenpuffer, Concurrency-Modell, exakter Modulschnitt) liegen im Ermessen des Modells – wichtig ist die Erfüllung der funktionalen Spezifikationen.

---

## 2. Kernfunktionen (Feature-Spezifikation)

### A. Datenquellen & I/O
- **CSV Support:**
  - Robustes Parsing mit Delimiter-Sniffer (Komma, Semikolon, Tab).
  - Encodings: UTF-8, UTF-8 mit BOM, ANSI / Windows-1252.
  - CSV-Vorschau-Dialog zur manuellen Wahl von Trennzeichen/Encoding bei Bedarf.
  - Mehrfach-Import aller CSVs eines Ordners.
- **LibreOffice Calc (.ods) Support:**
  - ODS-Reader (ODF XML `content.xml` aus ZIP entpacken/lesen).
  - Blatt-Auswahl-Dialog (`SelectSheet`), falls die Arbeitsmappe mehrere Blätter enthält.
  - Direkte Option: "In LibreOffice öffnen" (Erkennung von `soffice.exe` über Standardpfade/Registry).
- **Ansichten / Sammlungen (`views.json`):**
  - Gruppierung mehrerer Tabellenpfade zu einer logischen Sammlung.
  - Schneller Wechsel zwischen Ansichten und Ordnern per Dropdown im Footer.
  - Warn-Overlay für ungültige oder nicht erreichbare Dateipfade.

### B. Tabellenmodell & Spaltenabstraktion (Concepts)
- **Konzept-basiertes Mapping (`ColumnMappings`):**
  - Regeln adressieren Spalten über abstrakte Konzepte (z. B. Konzept `DeviceIP` -> CSV-Header `ip_address`).
  - Umbenennungen der Roh-Header brechen keine Regeln, solange das Mapping gepflegt ist.
- **Spalten-Metadaten:**
  - Sichtbarkeit (Hidden/Visible), benutzerdefinierte Reihenfolge und Breiten.
  - Layout-Optionen: Standardbreite (150px) oder responsives Einpassen in die Fensterbreite.
- **Virtuelle Spalten (Dynamic Columns):**
  - Dynamisch aus Verknüpfungs- oder Berechnungsregeln in das Grid injizierte Spalten, voll filter- und sortierbar.

### C. Tabellenbedienung & Grid-Behaviors
- **Multi-Tab Interface:**
  - Beliebig viele geöffnete Tabellen in Tabs.
  - Drag & Drop zum Umsortieren von Tabs.
  - Dirty-Tracking (`*` / Disketten-Symbol bei ungespeicherten Änderungen, Revert-Button).
  - Statusanzeige bei Fehlern/Warnungen (z. B. Tab-Hervorhebung/Blinken).
- **Tabellenkalkulations-Bediengefühl:**
  - *Inline-Editing:* F2, Doppelklick oder direktes Lostippen startet Zellenbearbeitung.
  - *Clipboard:* `Ctrl+C` (Kopieren von markierten Zeilen/Zellen als TSV/CSV), `Ctrl+V` (Block-Einfügen), `Entf` (Inhalte leeren).
  - *Zeilenauswahl:* Zeilen-Checkboxen für Massenoperationen, "X Zeilen markiert"-Leiste, "Nur Markierte anzeigen"-Toggle, "Leere Zeilen markieren".
  - *Sortierung:* Natürliche alphanumerische Sortierung (`natord`, z. B. `Port 2` vor `Port 10`).

### D. Filter- & Suchsystem
- **Spaltenfilter-Popups im Header:**
  - Dropdown pro Spalte mit Suchfeld und Multi-Select-Checkboxen.
  - Spezielle Filterpunkte: `(Alle)`, `(Leere)`, `(Nicht leere)`.
- **Filter-Presets:**
  - Aktive Filter-Zusammenstellung als benanntes Preset speichern.
  - Presets anwenden, auflisten, löschen.
  - Ein-Klick-Zurücksetzen aller aktiven Filter.

### E. Analyse- & Regel-Engine
Regeln werden per JSON definiert und auf Tabellen angewendet:
1. **VirtualRowJoin (`virtualRowJoin`):**
   - Lookups über Schlüsselspalten (1:1 oder mehrstufig über Zwischenschritte `LinkSteps`).
   - Injiziert gefundene Spalten als virtuelle Spalten in die Basistabelle.
2. **CrossTableComparison (`crossTableCompare`):**
   - Tabellenvergleich (z. B. "Datensätze in Tabelle A, die in Tabelle B fehlen").
   - Interaktives **Proposal-System:** Dialog zur Prüfung fehlender Zeilen und Übernahme auf Knopfdruck.
3. **GroupValidation (`groupValidation`):**
   - Gruppierung nach Schlüsselspalten.
   - Prüfungen innerhalb der Gruppe: Eindeutigkeit (`isUnique`), numerischer Wertebereich (`numericRange`), Feld-Konsistenz (`groupConsistency`).
4. **Calculation (`calculation`):**
   - Berechnete Spalten via Formelausdruck über andere Spaltenwerte.
5. **BulkField (`bulkField`):**
   - Prüfung auf Pflichtfelder (`CheckAllForEmpty`) oder Platzhalter (`CheckAllForPlaceholder`).
6. **Aktions-Dock (Bottom Panel):**
   - Einklappbares unteres Panel mit Regelgruppen, Trefferzählern (`HitCount`) und Toggles zur visuellen Hervorhebung.

### F. Textmarker- & Highlight-System
- Regelbasierte farbliche Hervorhebungen (Zell-/Zeilenebene) nach Validierungsstatus (Error, Warning, Info).
- Manuelle Textmarker-Funktion (Schnellfarben wie Gelb, Grün, Blau, Rot, Orange, Lila oder Custom Color).
- Animiertes/pulsierendes Overlay bei markierten Zeilen mit aktivem Textmarker.

### G. Datenmanipulation & Workflows
- **Spalte füllen:** Ausgewählte Zeilen mit statischem Wert, Präfix/Suffix oder Inkrement befüllen.
- **Spalte aufteilen:** Zelle nach Trennzeichen in mehrere Spalten aufteilen.
- **Smart Add (Zeilenvorlage):** Neue Zeile basierend auf selektierter Zeile anlegen (mit intelligenter Hochzählung).
- **Letzte Aktion anwenden:** Merkt sich die letzte Editier-Aktion und bietet kontextbezogene Folgeoperationen an.

### H. Synchronisation & Diff (CSV <-> ODS)
- Erkennung externer Dateiänderungen (Timestamp/Größe/Hash) mit Warnhinweis.
- Tabellendaten-Diff-Dialog zum selektiven Abgleich zwischen CSV und LibreOffice ODS.
- Datenübertragung: Richtungsauswahl (Quelle ➔ Ziel) zum direkten Synchronisieren.

### I. Konfigurations-Editor & Logging
- **InfoFile-Editor:** Visuelle GUI zum Erstellen und Bearbeiten der `.info`-Konfigurationen (Konzepte, Regeln, Spaltenlayouts).
- **Log-Viewer:** Integriertes Logging im Bottom-Dock mit Filterung nach Loglevel (Trace, Debug, Info, Warning, Error) und Copy-Funktion.

---

## 3. Dateistrukturen (Referenz)

### `.info`-Dateiformat (`<name>.csv.info` als JSON)
```json
{
  "InfoFileVersion": 1,
  "CsvSettings": {
    "FileName": "switches.csv",
    "Delimiter": ",",
    "HasHeader": true,
    "Encoding": "utf-8"
  },
  "ColumnMappings": {
    "DeviceName": "sw_name",
    "IPAddress": "management_ip"
  },
  "ColumnSettings": [
    { "ColumnName": "sw_name", "Width": 180, "IsVisible": true, "DisplayOrder": 0 }
  ],
  "SavedFilterPresets": [],
  "Rules": []
}
```

### `views.json` (Sammlungen)
```json
{
  "Views": [
    {
      "Name": "Netzwerk Köln",
      "Files": [
        { "RelativePath": "switches.csv", "InfoFilePath": "switches.csv.info" },
        { "RelativePath": "patchpanel.csv", "InfoFilePath": "patchpanel.csv.info" }
      ]
    }
  ]
}
```

---

## 4. Empfohlene Rust-Crates

| Bereich | Crate-Vorschlag |
|---|---|
| **UI** | `slint` |
| **CSV & Encoding** | `csv`, `encoding_rs` |
| **ODS / Tabellen** | `calamine` oder `zip` + `quick-xml` |
| **Clipboard** | `arboard` |
| **Sortierung** | `natord` (Natural Sort) |
| **Formel-Parser** | `evalexpr` |
| **JSON** | `serde`, `serde_json` |
| **File-Watcher** | `notify` |
| **Parallelisierung** | `rayon` (Hintergrund-Prüfungen der Rules) |

---

## 5. Schrittweise Vorgehensweise (Entwicklungsplan)

1. **Phase 1: Datenkern (`core`)**
   - Tabellenpuffer-Datenstruktur (`TableData`, `Column`, `Row`, Dirty-Tracking).
   - CSV-Parser & Writer mit Delimiter-Sniffer und Encoding-Support.
   - ODS-Reader (Einlesen von Tabellenblättern).
   - JSON-Serialisierung der `.info`- und `views.json`-Dateien.
2. **Phase 2: UI-Skelett (`ui` mit Slint)**
   - Hauptfenster: Menü-/Footerleiste, Multi-Tab-Leiste, Grid-Bereich, einklappbares Bottom-Dock.
   - Tabellen-Darstellung via Slint `Model`.
3. **Phase 3: Tabellen-Interaktion & Editing**
   - Inline-Editing von Zellen, Tastatur-Shortcuts (F2, Enter, Esc, Del).
   - Clipboard-Operationen (Copy/Paste tabellarischer Daten via `arboard`).
   - Zeilenauswahl (Checkboxen) und Selektions-Logik.
4. **Phase 4: Sortierung & Filterung**
   - Natural Sort (`natord`) bei Header-Klick.
   - Spaltenfilter-Popup (Checkboxen, Suche, Leer/Nicht-leer).
   - Filter-Presets speichern und laden.
5. **Phase 5: Regel-Engine & Analysen**
   - Konzept-Mapping-Resolver (`Concept ➔ Spaltenname`).
   - Implementierung der Checkers: `BulkField`, `Calculation`, `GroupValidation`, `VirtualRowJoin`, `CrossTableComparison`.
   - Hintergrund-Ausführung von Prüfungen und Rückgabe von Highlight- und Fehlermengen.
6. **Phase 6: Dialoge & Datenmanipulation**
   - Spalte füllen, Spalte trennen, Smart Add.
   - Proposal-Dialog für `CrossTableComparison`.
   - Diff-Viewer für extern geänderte Tabellen / ODS-Sync.
7. **Phase 7: Visueller Info-Editor & Feinschliff**
   - Konfigurationsdialog zum Bearbeiten von Mappings und Regeln.
   - Integriertes In-Memory Logging.
   - Performance-Feinschliff.
