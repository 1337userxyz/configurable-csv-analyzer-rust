# Spezifikation: Virtuelle Spalten & Multi-Step-Joins (`VirtualRowJoin`)

Dieses Dokument beschreibt den detaillierten Aufbau, die Datenstrukturen, die mehrstufige Verknüpfungslogik (`LinkSteps`), die asynchronen Vorberechnungen (Live Hit Counts & Beziehungserkennung) sowie die Spalteninjektion im **InfoFileEditorWindow**.

---

## 1. Konzeptionelle Übersicht

Virtuelle Spalten ermöglichen relationale Verknüpfungen (Lookups & Joins) über mehrere CSVs und LibreOffice ODS-Tabellen hinweg.
- **Ziel:** Spalten fremder Tabellen dynamisch in die Ansicht der aktuellen Tabelle einblenden.
- **Eigenschaften:** Virtuelle Spalten verhalten sich im DataGrid wie reguläre Spalten (sortierbar, filterbar, in weiteren Regeln referenzierbar), werden aber nicht in die Original-CSV zurückgeschrieben.
- **Ketten-Unterstützung (`Multi-Step Joins`):** Es können beliebig viele Tabellen hintereinander verknüpft werden (z. B. *Patchpanel ➔ Switch ➔ Raum ➔ Gebäude*).

---

## 2. Datenmodell (`Models/VirtualRowJoinRule.rs`)

### 2.1. Die Hauptregel (`VirtualRowJoinRule`)
```json
{
  "RuleName": "Switch-Infos an Patchpanel anfügen",
  "ActionGroupName": "Netzwerk",
  "IsActive": true,
  "IsHighlighted": true,
  "Description": "Liest Switch-Name und IP für jeden belegten Port aus",
  "LinkSteps": [ ... ],
  "VirtualColumns": [ ... ]
}
```

### 2.2. Ein Verknüpfungsschritt (`LinkStep`)
Jeder Schritt repräsentiert einen Übergang von einer Quelltabelle zur nächsten Zieltabelle:
```json
{
  "TargetTableKey": "switches.csv",
  "KeyMappings": [
    {
      "HostConcept": "SwitchPortID",
      "RemoteConcept": "PortID",
      "ManualRemoteValue": null,
      "IsCaseSensitive": false
    }
  ]
}
```
* **`TargetTableKey`:** Eindeutiger Schlüssel/Dateiname der Zieltabelle (oder `$self` für Selbstverknüpfungen innerhalb derselben Tabelle).
* **`KeyMappings` (Bedingungen):**
  * `HostConcept`: Konzept/Feld der aktuellen Stufe (beim 1. Schritt: Quelltabelle; ab Schritt 2: Zieltabelle des vorherigen Schritts).
  * `RemoteConcept`: Konzept/Feld der Ziel-Tabelle dieses Schritts.
  * `ManualRemoteValue`: Optionaler statischer Filterwert (z. B. `ManualRemoteValue = "Aktiv"`). Überschreibt den dynamischen Spaltenabgleich für diese Bedingung.
  * `IsCaseSensitive`: Groß-/Kleinschreibung beim Textvergleich beachten.

### 2.3. Virtuelle Ergebnisspalten (`VirtualColumnMapping`)
Definiert, welche Spalten aus der **finalen Zieltabelle** des letzten Schritts in die Host-Tabelle eingeblendet werden:
```json
{
  "RemoteColumnConcept": "ManagementIP",
  "VirtualColumnHeader": "Management IP (switches -> patchpanel)",
  "VirtualColumnKey": "v_managementip_from_switches_to_patchpanel",
  "MultiResultDisplay": "First",
  "BrushKey": "PositiveBrush",
  "Priority": 5
}
```

---

## 3. UI-Aufbau im InfoFileEditor (3-Phasen-Layout)

Der Reiter **"Virtuelle Spalten"** ist wie eine visuelle SQL-Abfrage gegliedert:

```
┌────────────────────────────────────────────────────────────────────────┐
│ 1. Basis-Einstellungen                                                 │
│    Regelname, Aktionsgruppe, Beschreibung (mit "Autotext"-Button),     │
│    [x] Regel aktiv    [x] Im Grid hervorheben                          │
├────────────────────────────────────────────────────────────────────────┤
│ 2. Join-Kette (Datenfluss) - LinkSteps                                 │
│    ┌──────────────────────────────────────────────────────────────┐    │
│    │ Schritt 1: Verknüpfung von 'patchpanel' nach:                │    │
│    │ Ziel-Tabelle: [ switches.csv ▾ ]  (38/42 Quellzeilen gematcht)│   │
│    │ ┌──────────────────────┬──────────────────────┬────┬───────┐ │    │
│    │ │ Host (Quell-Konzept) │ Remote (Ziel-Konzept)│ Aa │ Löschen││   │
│    │ │ SwitchPortID       ▾ │ PortID             ▾ │ [ ]│   🗑   ││   │
│    │ └──────────────────────┴──────────────────────┴────┴───────┘ │    │
│    │ [+ Bedingung hinzufügen]                                     │    │
│    └──────────────────────────────────────────────────────────────┘    │
│    [+ Verknüpfungsschritt hinzufügen]   [Letzten Schritt entfernen]     │
├────────────────────────────────────────────────────────────────────────┤
│ 3. SELECT (Ergebnis-Spalten)                  [ Badge: 1:1 Beziehung ] │
│    ┌──────────────────┬──────────────────┬─────────────┬─────────────┐ │
│    │ Remote-Konzept   │ Header im Grid   │ Bindung-Key │ Modus (1:N) │ │
│    │ ManagementIP   ▾ │ Management IP... │ v_mgmt_ip...│ Erster... ▾ │ │
│    └──────────────────┴──────────────────┴─────────────┴─────────────┘ │
│    [+ Virtuelle Spalte hinzufügen]                                     │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Vorberechnungen & Asynchrone Analyse-Engine

Damit der Benutzer sofortiges Feedback über die Gültigkeit seiner Join-Bedingungen erhält, führt der Editor im Hintergrund reaktive Vorberechnungen durch:

### 4.1. Live Hit Count pro Step (Trefferquote)
Sobald eine Zieltabelle oder ein Schlüsselpaar geändert wird, startet eine asynchrone Trefferzählung:
1. **Debouncing & Task-Cancellation:**
   - Ein atomarer Zähler (`calculation_id`) stellt sicher, dass veraltete Berechnungen sofort abgebrochen und deren Ergebnisse verworfen werden.
   - Status während der Berechnung: *"Berechne Treffer..."*.
2. **Echzeit-Rückmeldung:**
   - Format: `"{GematchteQuellzeilen} / {GesamteQuellzeilen} Quellzeilen fanden Treffer."`
   - Bei unvollständigen Bedingungen: *"Warte auf vollständige Bedingung..."*.
3. **Vergleichsregeln:**
   - **SQL-Null-Semantik:** Zwei leere oder reine Whitespace-Zellen verbinden sich **niemals**.
   - **Numerische Normalisierung:** Strings, die gültige Dezimalzahlen darstellen, werden numerisch verglichen (`10` matcht `10.0`).
   - Textvergleiche nutzen je nach `IsCaseSensitive` ordinale oder case-insensitive Gleichheit.

### 4.2. Automatische Erkennung des Beziehungstyps (`JoinRelationshipType`)
Der Editor analysiert die Multiplizität der Beziehung über alle Stufen hinweg:
- **`OneToOne` (1:1):** Jede Quellzeile matcht höchstens eine Zielzeile in der Kette.
- **`OneToMany` (1:N):** Mindestens eine Quellzeile matcht mehrere Zielzeilen.
- **Visuelles Badge:**
  - `1:1`: Grünes Badge *"1:1 Beziehung (Eindeutig)"*.
  - `1:N`: Gelbes Badge *"1:n Beziehung (Mehrere Treffer möglich)"*.
  - Bei `1:N` wird der Dropdown für `MultiResultDisplay` optisch hervorgehoben und empfohlen.

### 4.3. Multi-Result-Displaymodi bei 1:N-Joins (`MultiResultDisplayMode`)
Liefert der Join mehrere Treffer für eine einzige Zeile, steuert diese Option die Darstellung im Grid:

| Modus | Verhalten | Beispiel |
|---|---|---|
| **`First`** | Zeigt nur den ersten gefundenen Wert (Standard). | `"GigabitEthernet0/1"` |
| **`CountMerged`** | Zählt alle gefundenen Treffer (inklusive Duplikaten). | `"5"` |
| **`CountSeparated`** | Zählt nur unterschiedliche (distinct) Werte. | `"3"` |
| **`CommaSeparated`** | Kommagetrennte Liste der eindeutigen Werte. | `"VLAN 10, VLAN 20"` |
| **`CommaSeparatedMerged`** | Kommagetrennte Liste aller Werte (inkl. Duplikate). | `"VLAN 10, VLAN 10, VLAN 20"` |
| **`Sum`** | Summiert alle numerischen Werte. | `"1500.50"` |
| **`Average`** | Berechnet den arithmetischen Mittelwert. | `"750.25"` |

### 4.4. Automatische Namensgenerierung (`GenerateDefaultNames`)
Sobald der Anwender ein Remote-Konzept oder einen `MultiResultDisplayMode` wählt, generiert der Editor automatisch sprechende Namen:
- **Header:**
  - Standard: `"{Konzept} ({Zieltabelle} -> {Quelltabelle})"`
  - Bei `Sum`: `"Summe {Konzept} ({Zieltabelle} -> {Quelltabelle})"`
  - Bei `CountSeparated`: `"Anzahl Eindeutig {Konzept} ({Zieltabelle} -> {Quelltabelle})"`
- **Key (Technischer Bindungsschlüssel):**
  - Schema: `v_{modus}_{konzept}_from_{ziel}_to_{quelle}` (in Kleinbuchstaben, bereinigt von Umlauten und Sonderzeichen).
- **Benutzerschutz:** Hat der Benutzer manuell einen eigenen Namen eingetippt, wird dieser **nicht** durch Automatismen überschrieben.

---

## 5. Autotext-Generator (`GenerateDescription`)
Der Button **"Autotext"** neben dem Beschreibungsfeld analysiert die gesamte Kette und generiert automatisch eine zusammenfassende Dokumentation:
```text
--- Automatisierte Join-Beschreibung ---
Regel: Switch-Infos an Patchpanel anfügen (Aktiv: True)
Gruppe: Netzwerk

Verknüpfungskette:
 patchpanel.csv -> switches.csv (via 1 Bedingung(en))
  - SwitchPortID == PortID
```

---

## 6. Validierungs-Regeln für Virtuelle Spalten

Vor dem Speichern der `.info`-Datei prüft die Validierungs-Engine folgende Punkte:
1. **Schritte:** Mindestens 1 `LinkStep` muss definiert sein.
2. **Zieltabelle:** Jeder Step muss eine gültige, in der Session geladene `TargetTableKey` referenzieren.
3. **Bedingungen:** Jeder Step benötigt mindestens ein `KeyMapping` mit ausgefülltem `HostConcept` und (`RemoteConcept` ODER `ManualRemoteValue`).
4. **Schlüssel-Syntax:** Jeder `VirtualColumnKey` muss ein valider Identifier sein: `^[a-zA-Z_][a-zA-Z0-9_]*$`.
5. **Eindeutigkeit:** `VirtualColumnKey` muss innerhalb der Host-Tabelle absolut eindeutig sein.
6. **Farbpalette:** Der angegebene `BrushKey` muss in `AppConfig.BrushDefinitions` existieren.
