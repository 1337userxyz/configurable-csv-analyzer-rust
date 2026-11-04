# Configurable CSV & ODS Analyzer (Rust + Slint)

Performantes Desktop-Tool in **Rust** mit **Slint UI** zur flexiblen Analyse, Validierung, Verknüpfung und Bearbeitung tabellarischer Daten (CSV und LibreOffice Calc `.ods`).

Basiert auf den Vorgaben aus [`BLUEPRINT.md`](./BLUEPRINT.md).

---

## 🚀 Schnellstart

### 1. Ausführen
```bash
cargo run
```
Das Hauptfenster öffnet sich und erlaubt das Öffnen beliebiger CSV- und ODS-Dateien sowie kompletter Ordner.

### 2. Tests ausführen
```bash
cargo test
```
Führt die automatisierte Test-Suite für Delimiter-Sniffer, Regel-Engine, Smart Add, Spaltenmanipulation und Joins aus.

---

## 🌟 Umgesetzte Funktionen

### A. Datenquellen & I/O
- **CSV Parsing:**
  - Automatischer Delimiter-Sniffer (Komma `,`, Semikolon `;`, Tab `\t`, Pipe `|`).
  - Encoding-Unterstützung (UTF-8, UTF-8 mit BOM, ANSI / Windows-1252 via `encoding_rs`).
  - Mehrfach-Import aller CSVs eines Ordners in Tabs.
- **LibreOffice Calc (.ods) Support:**
  - ODS-Reader via `calamine`.
  - Blatt-Auswahl-Dialog (`SelectSheet`) bei Multi-Sheet-Arbeitsmappen.
  - Ein-Klick-Aktion: "In LibreOffice öffnen" (automatische Erkennung von `soffice.exe` in Standardpfaden & PATH).
- **Views & Sammlungen:**
  - Struktur für `views.json` zum Gruppieren zusammengehöriger Tabellen.

### B. Tabellenmodell & Konzepte
- **Konzept-basiertes Mapping (`ColumnMappings`):**
  - Abstrakte Konzepte (z. B. `IPAddress` -> `management_ip`), sodass Umbenennungen von Spalten keine Regeln brechen.
  - Spalten-Metadaten (Breiten, Sichtbarkeit, Reihenfolge).
  - Virtuelle Spalten (dynamisch aus Berechnungen und Joins injiziert).

### C. Tabellenbedienung & Grid
- **Multi-Tab Interface:**
  - Paralleles Arbeiten mit beliebig vielen Tabellen.
  - Dirty-Tracking (`*` bei ungespeicherten Änderungen, Revert-Funktion).
  - Fehler-Badges auf Tabs mit Regelverletzungen.
- **Kalkulationsbedienung:**
  - *Inline-Editing:* Doppelklick auf Zellen zum Bearbeiten mit Eingabefeld.
  - *Clipboard:* Kopieren als TSV / CSV (kompatibel mit Excel / Calc) und Block-Einfügen via `arboard`.
  - *Zeilenauswahl:* Checkboxen, "X Zeilen markiert"-Zähler, "Nur Markierte anzeigen"-Filter, "Leere Zeilen markieren".
  - *Natürliche Sortierung:* Header-Klick sortiert alphanumerisch nach `natord` (z. B. `Port 2` vor `Port 10`).

### D. Filter- & Suchsystem
- **Spaltenfilter-Popups:**
  - Dropdown pro Spalte mit Live-Suche und Checkboxen der eindeutigen Werte.
  - Schnellsuche über die gesamte Tabelle in der Toolbar.
  - Ein-Klick "Filter leeren".

### E. Analyse- & Regel-Engine
- **BulkField:** Prüfung auf Pflichtfelder (`check_all_for_empty`) und Platzhalter (`check_all_for_placeholder`, z. B. `TBD`, `-`, `???`).
- **Calculation:** Formelausdrücke über Spaltenwerte via `evalexpr` (z. B. `Price * Qty`).
- **GroupValidation:** Gruppierung nach Konzept und Prüfung auf Eindeutigkeit (`is_unique`), numerische Wertebereiche (`numeric_range`) und Gruppen-Konsistenz.
- **VirtualRowJoin:** Dynamisches Nachschlagen und Injizieren von Spalten aus verknüpften Tabellen.
- **CrossTableComparison:** Erkennung von Zeilen, die in Zieltabellen fehlen, mit interaktivem Vorschlagssystem (`Proposals`).

### F. Textmarker- & Highlight-System
- Regelbasierte farbliche Hervorhebungen (Rot = Fehler, Gelb = Warnung, Blau = Info).
- Manuelle Textmarker-Palette mit 6 Schnellfarben (Gelb, Grün, Blau, Rot, Orange, Lila) für markierte Zeilen.

### G. Datenmanipulation & Workflows
- **Spalte füllen:** Statischer Wert, Präfix/Suffix oder Inkrement (z. B. `TAG-0100`, `TAG-0110`).
- **Spalte aufteilen:** Aufteilen nach Delimiter in neue Spalten.
- **Smart Add:** Neue Zeile basierend auf selektierter Vorlage mit automatischer Hochzählung von IP-Adressen und Zahlenfolgen (z. B. `192.168.1.1` ➔ `192.168.1.2`).
- **Diff & Sync:** Vergleich zwischen zwei Tabellen (CSV <-> ODS) mit Diff-Report und Synchronisationsoption.

### H. Bottom-Dock & Logging
- Einklappbares unteres Panel mit Tabs für:
  - **Regeln & Analysen** (Trefferzähler, Fehler, Warnungen)
  - **Vorschläge (Proposals)** (Übernahme fehlender Zeilen auf Knopfdruck)
  - **Log-Viewer** (Live In-Memory Logging mit Kopierfunktion)

---

## 📂 Beispiel-Dateien (`examples/`)
- [`examples/switches.csv`](./examples/switches.csv): Netzwerk-Switches Tabelle.
- [`examples/switches.csv.info`](./examples/switches.csv.info): `.info`-Konfiguration mit Mappings und aktiven Validierungsregeln.
- [`examples/patchpanel.csv`](./examples/patchpanel.csv): Verknüpfungstabelle.
- [`examples/views.json`](./examples/views.json): Beispiel für Ansichten / Sammlungen.


---

## 📌 Nächster Meilenstein: Konfigurations-UIs

Die vollständige funktionale Spezifikation für die beiden noch fehlenden Dialoge befindet sich in:
👉 **[`SPEC_CONFIGURATION_AND_INFOFILE_EDITOR.md`](./SPEC_CONFIGURATION_AND_INFOFILE_EDITOR.md)**

1. **Hauptkonfiguration (`ConfigurationWindow`):**
   - Sammlungen / Ansichten verwalten (`views.json`)
   - InfoFile-Speichermodi (`config.json`)
   - Farbkatalog & Marker-Prioritäten (`BrushDefinitions`)
   - Dateiverknüpfungen
2. **Tabellen-Konfigurationseditor (`InfoFileEditorWindow`):**
   - Visuelle Konfiguration pro Tabelle (`*.csv.info`)
   - Spalten & Sichtbarkeiten
   - Konzept-Mappings (`ColumnMappings`)
   - Berechnete Spalten (`CalculationRules`)
   - Virtuelle Spalten & Multi-Step-Joins (`VirtualRowJoinRules`)
   - Tabellenvergleiche & Proposals (`CrossTableComparisonRules`)
   - Gruppenvalidierungen (`GroupValidationRules`)
   - Highlight- & Folgeregeln (`HighlightRules`, `ApplyRules`)
   - Semantische Validierung vor dem Speichern


Zusätzliche Detail-Spezifikation für komplexe relationale Joins:
👉 **[`SPEC_VIRTUAL_COLUMNS_JOIN_STEPS.md`](./SPEC_VIRTUAL_COLUMNS_JOIN_STEPS.md)** (Multi-Step-Joins, Live Hit Counts, Beziehungs-Erkennung 1:1/1:N und Aggregationen).
