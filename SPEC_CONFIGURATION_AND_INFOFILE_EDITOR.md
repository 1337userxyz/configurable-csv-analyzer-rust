# Spezifikation: Hauptkonfiguration & InfoFile-Editor (Rust + Slint)

Dieses Dokument beschreibt detailliert die funktionale Architektur, Felder, Validierungen und Workflows der beiden fehlenden Kernmodule:
1. **Hauptkonfiguration (`ConfigurationWindow` / `AppConfig` / `views.json` / `config.json`)**
2. **Tabellen-Konfigurationseditor (`InfoFileEditorWindow` / `TableConfig` / `*.csv.info`)**

Fokus liegt rein auf **Datenmodell, Logik, Workflows und Fähigkeiten** – ohne Einschränkungen für das visuelle Design.

---

# TEIL 1: Hauptkonfiguration (`ConfigurationWindow`)

Die Hauptkonfiguration verwaltet anwendungsweite Einstellungen, globale Farbdefinitionen, Dateiverknüpfungen und insbesondere die logischen Sammlungen von Tabellen („Ansichten“).

Gespeichert wird dies primär in zwei globalen JSON-Dateien:
- `config.json`: Allgemeine App-Einstellungen, Farbkatalog, Speichermodi.
- `views.json`: Definitionen der Sammlungen (Views) und ihrer Dateizuordnungen.

---

### Bereich 1: Ansichten & Pfade ("Views / Collections")
Verwaltet Gruppierungen von Tabellendateien, die zusammengehören (z. B. "Campus Nord", "Netzwerk Dokumentation").

#### Datenmodell & Attribute einer Ansicht (`ViewDefinition`):
- `Name`: Eindeutiger Name der Ansicht.
- `IsDefaultView`: Boolean (wird beim Anwendungsstart automatisch geladen).
- `Files`: Liste der zugeordneten Dateien (`ViewFileItem`):
  - `RelativePath` / `FullPath`: Pfad zur CSV-Datei.
  - `InfoFilePath`: Pfad zur zugehörigen `.csv.info`-Datei (optional; falls leer, gilt der globale Speichermodus).
  - `StorageMode`: Dezentral (`Dateipfad`), Zentral (`Stammverzeichnis`) oder `Temporaer`.
- `SpreadsheetPath`: Optionaler Pfad zu einer verknüpften LibreOffice Calc (`.ods`)-Datei.
- `ColorPattern`: Farbmuster für das DataGrid (`None`, `Original`, `OriginalPlusZebra`, `Zebra`).
- `ExportColorPattern`: Farbmuster beim Exportieren.
- `ExportPaths`: Liste definierter Export-Ziele (Pfad + Format).

#### Operationen & Fähigkeiten:
1. **Sammlungs-Verwaltung:**
   - Neue Ansicht erstellen, bestehende umbenennen oder löschen.
   - Reihenfolge der Ansichten ändern (Up/Down bzw. Drag & Drop).
   - "Als Standard-Ansicht setzen".
2. **Dateizuordnung innerhalb einer Ansicht:**
   - CSV-Dateien per Dateidialog oder Ordnerauswahl hinzufügen.
   - Dateien aus der Ansicht entfernen.
   - Reihenfolge der Dateien ändern (bestimmt die Tab-Reihenfolge im Hauptfenster).
3. **Integritäts- & Pfadprüfung:**
   - Prüft im Hintergrund, ob die referenzierte CSV-Datei auf der Festplatte/Netzlaufwerk existiert.
   - Prüft, ob die zugehörige `.info`-Datei existiert.
   - Status-Anzeige pro Datei: "Gültig", "CSV fehlt", "Info fehlt", "Pfad unerreichbar".
4. **Info-Dateien verschieben / migrieren (`MoveInfoFile`):**
   - Bietet eine Ein-Klick-Aktion, um eine Info-Datei zwischen zentralem Ordner und dezentralem Speicherort (neben der CSV) zu verschieben, falls der Modus geändert wurde.
5. **Vergleichs-Aktion:**
   - Button, um zwei Info-Dateien miteinander zu vergleichen (Diff).
6. **Direktsprung:**
   - Aus der Dateiliste heraus kann direkt der `InfoFileEditor` für eine ausgewählte Datei geöffnet werden.

---

### Bereich 2: Info-Dateien & Speicherorte (`InfoFileConfig`)
Steuert, wie die Anwendung standardmäßig mit den Konfigurationsdateien (`.csv.info`) umgeht.

#### Globale Modi (`InfoFileStorageMode`):
1. **`Dateipfad` (Dezentral):** Die Konfigurationsdatei liegt immer direkt neben der CSV-Datei (z. B. `switches.csv` ➔ `switches.csv.info`).
2. **`Stammverzeichnis` (Zentral):** Alle Info-Dateien werden in einem zentralen Verzeichnis gespeichert (z. B. `./Configs/`). Der Dateiname referenziert den Hash oder Namen der Quell-CSV.
3. **`Temporaer`:** Änderungen an Mappings und Regeln gelten nur für die laufende Sitzung und werden nicht auf Platte persistiert.
4. **`BenutzerEntscheidet`:** Beim Öffnen einer neuen, noch unbekannten CSV-Datei fragt das System den Benutzer interaktiv, wo die Konfiguration gespeichert werden soll.

#### Konfigurierbare Felder:
- `SelectedStorageMode`: Einer der vier obigen Modi als Standard.
- `CentralConfigDirectory`: Benutzerdefiniertes zentrales Verzeichnis (mit Fallback auf Standardordner `./Configs/` und "Ordner wählen"-Dialog).

---

### Bereich 3: Farben & Farbschema (`BrushesConfig`)
Verwaltet den globalen Farbkatalog (`BrushDefinitions`). Regeln und Textmarker referenzieren Farben nicht über feste Hex-Werte, sondern über symbolische Schlüssel (z. B. `ErrorBrush`, `DirtyCellBrush`, `MarkerGelb`).

#### Datenmodell (`BrushDefinition`):
- `Key`: Eindeutiger Name (z. B. `WarningBrush`, `MarkerGruen`).
- `HexValue`: Hex-Farbcode (z. B. `#FFD700`, `#FFF59D`).
- `Category`: Logische Kategorie zur Gruppierung:
  - `UI` (Zebra-Muster, Selektion, Filter-Hintergrund).
  - `Status` (Error, Warning, Problem, Positive, EmptyCell, DirtyCell).
  - `Textmarker` (MarkerGelb, MarkerGruen, MarkerBlau, MarkerRot etc.).
  - `Analyse` (CrossTableMismatch, UniquenessViolation).
  - `Logging` (Farben für die Loglevel im Logviewer).

#### Operationen & Fähigkeiten:
- Farbdefinition bearbeiten: Öffnet einen Color-Picker zur Farbauswahl.
- Neue Farbdefinition hinzufügen.
- Bestehende Farbdefinition löschen (sofern kein System-Brush).
- Filter-/Suchfeld: Filtert die Liste nach Name, Kategorie oder Hexwert.
- `MarkerHighlightPriority`: Numerischer Wert (Standard `100`). Bestimmt, ob manuelle Textmarker-Hervorhebungen eine höhere Priorität haben als automatische Regel-Highlights oder umgekehrt.

---

### Bereich 4: Dateiverknüpfungen (`FileAssociationConfig`)
- Option: "Beim Start prüfen, ob CSV-Dateien und Info-Dateien mit dieser Anwendung verknüpft sind".
- Button: "Dateiverknüpfungen jetzt im Betriebssystem registrieren".

---

# TEIL 2: Tabellen-Konfigurationseditor (`InfoFileEditorWindow`)

Der `InfoFileEditor` ist das Herzstück zur Konfiguration einer einzelnen Tabelle. Er wird für das aktuell geöffnete Tab aufgerufen (oder aus der Hauptkonfiguration heraus).

### Lebenszyklus & State Management:
- **Working Copy:** Der Editor arbeitet immer auf einer isolierten Kopie der `TableConfig`. Änderungen schlagen erst beim Klick auf **"Übernehmen & Speichern"** auf das Hauptfenster durch.
- **Dirty Tracking:** Jede Änderung aktiviert den "Ungespeichert"-Status. Schließen bei ungespeicherten Daten erfordert Bestätigung.
- **Validierungs-Engine:** Vor dem Speichern wird die gesamte Konfiguration semantisch validiert. Bei Fehlern (z. B. Syntaxfehler in Formel, fehlende Schlüsselzuordnung) wird das Speichern blockiert und der fehlerhafte Reiter visuell hervorgehoben.

---

### Tab 1: Allgemein (`GeneralTab`)
- **CSV-Parsing-Parameter:**
  - `Separator`: Trennzeichen (genau 1 Zeichen: Komma, Semikolon, Tabulator etc.).
  - `Encoding`: Zeichencodierung (UTF-8, UTF-8-BOM, Windows-1252/ANSI).
  - `HasHeader`: Boolean (erste Zeile enthält Spaltennamen).
- **Zellen-Status:**
  - `DirtyCellBrushKey`: Farbschlüssel aus dem Katalog für geänderte Zellen (Default: `DirtyCellBrush`).
- **Speicherort-Override:**
  - Eigener Speichermodus für diese spezifische Datei (`Dateipfad`, `Stammverzeichnis`, `Temporaer`).

---

### Tab 2: Spalten & Layout (`ColumnsTab`)
Verwaltet Darstellung und Reihenfolge aller sichtbaren Spalten der Tabelle (inkl. virtueller Spalten).

#### Pro Spalte (`ColumnDisplaySetting`):
- `ColumnName`: Name der Spalte (CSV-Header oder virtueller Spaltenname).
- `IsVisible`: Boolean (Checkbox). Ausgeblendete Spalten werden im Grid nicht angezeigt, bleiben aber in Regeln referenzierbar.
- `Width`: Spaltenbreite in Pixeln.
- `DisplayIndex`: Numerische Reihenfolge von links nach rechts.

#### Werkzeuge:
- Suchfeld zum schnellen Auffinden von Spalten in breiten Tabellen.
- Buttons: "Alle anzeigen", "Alle ausblenden", "Breiten auf Standard (150px) zurücksetzen".

---

### Tab 3: Spalten-Mappings & Konzepte (`ColumnMappingsTab`)
Ermöglicht die Abstraktion zwischen technischen CSV-Headern und fachlichen Konzepten.

#### Datenmodell:
- Key-Value-Paare: `ConceptName` ➔ `CsvColumnName`.
- Beispiel:
  - Konzept `DeviceName` ➔ CSV-Header `switch_hostname`
  - Konzept `IPAddress` ➔ CSV-Header `management_ip`

#### Fähigkeiten:
- Hinzufügen neuer Mappings.
- Dropdown für `CsvColumnName` mit allen echten Spalten der geladenen CSV.
- Löschen und Umbenennen.
- Suchfilter über Konzepte und CSV-Spaltennamen.
- Validierung: Warnt, wenn ein gemappter CSV-Header in der Datei gar nicht existiert.

---

### Tab 4: Berechnete Spalten (`CalculationRulesTab`)
Erzeugt dynamisch berechnete Spalten im DataGrid basierend auf Ausdrücken.

#### Datenmodell (`CalculationRule`):
- `RuleName`: Name der Regel.
- `ActionGroupName`: Name der Kategorie im Aktions-Dock.
- `IsActive`: Boolean (Regel ausführen).
- `IsHighlighted`: Boolean (Ergebnis im Grid standardmäßig hervorheben).
- `VirtualColumnHeader`: Anzeigename der erzeugten Spalte im Tabellenkopf.
- `VirtualColumnKey`: Eindeutiger interner Spaltenschlüssel.
- `Formula`: Formel-Ausdruck (z. B. `{Port} * 2` oder `{Prefix} + "-" + {Index}`).

#### Fähigkeiten im Editor:
- Spalten-Platzhalter einfügen: Klick auf ein Konzept/Spalte fügt `{KonzeptName}` in die Formel ein.
- Operator-Buttons: Klickbare Buttons für `+`, `-`, `*`, `/`, `(`, `)` zum schnellen Zusammenklicken.
- Formel-Syntaxvalidierung in Echtzeit.

---

### Tab 5: Virtuelle Spalten & Multi-Step-Joins (`VirtualRowJoinRulesTab`)
Ermöglicht relationale Lookups über Tabellengrenzen hinweg und blendet Felder fremder Tabellen in die aktuelle Ansicht ein.

#### Datenmodell (`VirtualRowJoinRule`):
- `RuleName`, `ActionGroupName`, `IsActive`, `IsHighlighted`.
- `LinkSteps`: Liste von Join-Schritten (unterstützt 1:1-Lookups und mehrstufige Ketten, z. B. *Switch ➔ Patchpanel ➔ Raum*):
  - Pro Schritt:
    - `TargetTableKey`: Schlüssel/Name der Zieltabelle.
    - `KeyMappings`: Liste von Schlüsselpaaren (`HostConcept` = `RemoteConcept`).
- `VirtualColumns`: Liste der einzubindenden Zielspalten:
  - `SourceConcept`: Welches Konzept/Feld aus der Zieltabelle gelesen wird.
  - `InjectedColumnName`: Wie die Spalte in der aktuellen Tabelle heißen soll.
  - `Width`, `IsVisible`.

#### Fähigkeiten im Editor:
- Schrittweises Hinzufügen und Entfernen von `LinkSteps`.
- Auswahl der Zieltabellen aus allen aktuell geladenen Tabellen/Dateien.
- Schlüsselpaare konfigurieren mit Dropdown-Unterstützung aller Konzepte von Quelle und Ziel.
- **Live-Treffervorschau:** Berechnet direkt im Dialog, wie viele Zeilen der Quelltabelle durch den Join erfolgreich gematcht werden (z. B. "98 von 100 Zeilen verknüpft (98%)").

---

### Tab 6: Tabellenvergleiche (`CrossTableComparisonRulesTab`)
Vergleicht Datensätze zweier Tabellen auf Übereinstimmung oder Differenz (Mengenlehre).

#### Datenmodell (`CrossTableComparisonRule`):
- `RuleName`, `ActionGroupName`, `IsActive`, `IsHighlighted`.
- `SelectedRemoteTableKey`: Zieltabelle, gegen die verglichen wird.
- `KeyMappings`: Liste von Schlüsselpaaren (`HostConcept` = `RemoteConcept`), die die Identität eines Datensatzes definieren.
- `ComparisonType`:
  - `MissingInRemote`: Zeilen in Tabelle A, die in Tabelle B fehlen.
  - `ExistsInRemote`: Zeilen in Tabelle A, die auch in Tabelle B existieren.
- `MismatchBrushKey`: Farbschlüssel zur Hervorhebung von Abweichungen.
- `MismatchTarget`: `Row` (gesamte Zeile) oder `Cell` (nur die Schlüsselzelle).
- `MismatchPriority`: Prioritäts-Index für die Farbhervorhebung.
- `MismatchHighlightedConcepts`: Bestimmte Spalten, die bei Abweichung markiert werden sollen.
- `ShowProposalDialog`: Boolean. Wenn aktiv, erhält die Regel im unteren Aktions-Dock einen interaktiven **"Start"**-Button. Dieser öffnet den `ProposeEntriesDialog`, mit dem fehlende Zeilen zeilenweise oder gesammelt in die Zieltabelle übertragen werden können.

---

### Tab 7: Gruppenvalidierungen (`GroupValidationRulesTab`)
Prüft logische Bedingungen über Gruppen von Zeilen hinweg.

#### Datenmodell (`GroupValidationRule`):
- `RuleName`, `ActionGroupName`, `IsActive`, `IsHighlighted`.
- `GroupingConcepts`: Eine oder mehrere Spalten/Konzepte, die die Gruppe bilden (z. B. gruppieren nach `SwitchName` und `VlanID`).
- `ValidationType`:
  1. **`isUnique`:** Prüft, ob ein bestimmtes Feld (`UniquenessValueConcept`) innerhalb der Gruppe eindeutig ist (z. B. jeder Port an einem Switch darf nur 1x vorkommen).
  2. **`numericRange`:** Prüft, ob ein Zahlenwert (`RangeValueConcept`) lückenlos oder innerhalb von Grenzwerten (`RangeStartConcept` bis `RangeEndConcept`) liegt.
  3. **`groupConsistency`:** Prüft, ob alle Zeilen innerhalb derselben Gruppe in definierten Feldern identische Werte haben (z. B. alle Datensätze desselben Gebäudes müssen dieselbe Postleitzahl haben).
- `MismatchBrushKey`: Farbe bei Regelverletzung.
- `MismatchTarget`: `Row` oder `Cell`.
- `MismatchPriority`: Priorität der Hervorhebung.
- `MismatchHighlightedConcepts`: Liste der Spalten, die bei einem Fehler markiert werden sollen.

---

### Tab 8: Highlight-Regeln (`HighlightRulesTab`)
Wert- und musterbasierte Hervorhebung von Zellen oder Zeilen basierend auf Kriterien.

#### Datenmodell (`HighlightRuleDefinition`):
- `RuleName`, `ActionGroupName`, `IsActive`, `IsHighlighted`.
- `ColumnConcept`: Zu prüfende Spalte / Konzept.
- `MatchCondition`: Kriterium:
  - `Equals` (Gleichheit)
  - `Contains` (Enthält Teilstring)
  - `StartsWith` (Beginnt mit)
  - `EndsWith` (Endet mit)
  - `RegEx` (Regulärer Ausdruck)
  - `IsEmpty` (Leer oder Whitespace)
  - `IsNotEmpty` (Nicht leer)
  - `InList` (Wert ist in einer Liste von Werten enthalten)
- `ValueToMatch`: Der Vergleichswert bzw. RegEx-Muster.
- `MultipleValuesToMatch`: Kommagetrennte Liste bei `InList`.
- `InvertCondition`: Boolean (Invertiert das Kriterium: NOT).
- `BrushKey`: Farbschlüssel aus dem globalen Katalog.
- `Target`: `Row` (ganze Zeile einfärben) oder `Cell` (nur die geprüfte Zelle).
- `Priority`: Numerische Priorität (höhere Zahl gewinnt bei Farbkonflikten).

---

### Tab 9: Übertragungs- & Folgeregeln (`ApplyRulesTab`)
Definiert automatisierte Folgeoperationen, wenn Zeilen manuell bearbeitet werden.

#### Datenmodell (`ApplyRule`):
- `RuleName`: Name der Regel.
- `SourceColumnConcepts`: Liste der Spalten, deren Bearbeitung die Regel auslöst.
- `RequiresConfirmation`: Boolean (vor Ausführung Bestätigungsdialog anzeigen).
- `ExecuteAutomatically`: Boolean (bei jeder Zellenänderung sofort im Hintergrund ausführen).
- `Targets`: Liste von Zieloperationen (`RuleTarget`):
  - `TargetTableKey`: Zieltabelle oder `$self` für dieselbe Tabelle.
  - `KeyMappings`: Verknüpfung zwischen Quell- und Zielzeilen.
  - `Conditions`: Bedingungen, wann die Übertragung greift (z. B. nur wenn Spalte X den Wert Y hat).
  - `FieldAssignments`: Welche Quellwerte in welche Zielspalten geschrieben werden.

---

# TEIL 3: Validierungslogik im Editor (Semantic Validation)

Bevor der InfoFile-Editor eine Konfiguration speichert, führt er folgende Validierungsprüfungen durch:
1. **Regel-Eindeutigkeit:** Keine doppelten Regelnamen innerhalb desselben Regeltyps.
2. **Konzept-Integrität:**
   - Jedes in Regeln genutzte Konzept muss in den `ColumnMappings` definiert sein.
   - Jedes Mapping muss auf eine existierende Spalte der aktuellen CSV verweisen.
3. **Join-Konsistenz:**
   - Zieltabellen in `VirtualRowJoin` und `CrossTableComparison` müssen existieren.
   - Alle Schlüsselpaare (`KeyMappings`) müssen auf beiden Seiten gültige Konzepte referenzieren.
4. **Formel-Validierung:**
   - Berechnete Spalten dürfen keine ungültigen Tokens oder Syntaxfehler enthalten.
   - Platzhalter `{X}` müssen auflösbaren Konzepten entsprechen.
5. **Farb-Referenzen:**
   - Alle angegebenen `BrushKey`s müssen im globalen Farbkatalog existieren.

**Verhalten bei Validierungsfehlern:**
- Der "Übernehmen & Speichern"-Button ist deaktiviert, solange Fehler mit Schweregrad `Error` vorliegen.
- Betroffene Tabs und Formularfelder zeigen Fehlertexte an.
