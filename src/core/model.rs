use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightItem {
    pub row: usize,
    pub col: Option<usize>,
    pub severity: HighlightSeverity,
    pub message: String,
    pub rule_id: String,
    #[serde(default)]
    pub brush_key: Option<String>,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleGroupSummary {
    pub rule_id: String,
    pub description: String,
    pub hit_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRow {
    pub source_table: String,
    pub target_table: String,
    pub key_value: String,
    pub row_data: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextMarkerColor {
    None,
    Yellow,
    Green,
    Blue,
    Red,
    Orange,
    Purple,
    Custom(String),
}

impl TextMarkerColor {
    pub fn hex(&self) -> &str {
        match self {
            TextMarkerColor::None => "",
            TextMarkerColor::Yellow => "#fff59d",
            TextMarkerColor::Green => "#c8e6c9",
            TextMarkerColor::Blue => "#bbdefb",
            TextMarkerColor::Red => "#ffcdd2",
            TextMarkerColor::Orange => "#ffe0b2",
            TextMarkerColor::Purple => "#e1bee7",
            TextMarkerColor::Custom(ref s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ColumnSetting {
    pub column_name: String,
    pub width: u32,
    pub is_visible: bool,
    pub display_order: u32,
}

impl Default for ColumnSetting {
    fn default() -> Self {
        Self {
            column_name: String::new(),
            width: 150,
            is_visible: true,
            display_order: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FilterCondition {
    pub selected_values: HashSet<String>,
    pub all_selected: bool,
    pub include_empty: bool,
    pub include_non_empty: bool,
    pub text_search: String,
}

impl FilterCondition {
    pub fn new_all() -> Self {
        Self {
            selected_values: HashSet::new(),
            all_selected: true,
            include_empty: true,
            include_non_empty: true,
            text_search: String::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.all_selected || !self.text_search.trim().is_empty()
    }

    pub fn matches(&self, val: &str) -> bool {
        let trimmed = val.trim();
        let is_empty = trimmed.is_empty();

        if !self.text_search.is_empty() {
            if !val.to_lowercase().contains(&self.text_search.to_lowercase()) {
                return false;
            }
        }

        if !self.all_selected {
            if is_empty {
                return self.include_empty || self.selected_values.contains("");
            } else {
                return self.selected_values.contains(val)
                    || self.selected_values.contains(trimmed);
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilterPreset {
    pub name: String,
    pub filters: HashMap<String, FilterCondition>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "Type", rename_all = "camelCase")]
pub enum RuleDefinition {
    BulkField {
        id: String,
        description: String,
        concept: String,
        #[serde(default)]
        check_all_for_empty: bool,
        #[serde(default)]
        check_all_for_placeholder: bool,
        #[serde(default)]
        placeholders: Vec<String>,
    },
    Calculation {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        #[serde(default = "default_true")]
        is_highlighted: bool,
        target_column: String,
        #[serde(default)]
        virtual_column_key: Option<String>,
        formula: String,
    },
    GroupValidation {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        #[serde(default = "default_true")]
        is_highlighted: bool,
        group_by_concept: String,
        #[serde(default)]
        validation_type: Option<String>,
        #[serde(default)]
        is_unique: bool,
        #[serde(default)]
        unique_concept: Option<String>,
        #[serde(default)]
        numeric_range: Option<NumericRangeCheck>,
        #[serde(default)]
        group_consistency: Option<GroupConsistencyCheck>,
        #[serde(default)]
        mismatch_brush_key: Option<String>,
        #[serde(default)]
        mismatch_target: Option<String>,
        #[serde(default)]
        mismatch_priority: Option<u32>,
    },
    VirtualRowJoin {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        #[serde(default = "default_true")]
        is_highlighted: bool,
        #[serde(default)]
        key_concept: String,
        #[serde(default)]
        target_table: String,
        #[serde(default)]
        target_key_concept: String,
        #[serde(default)]
        import_concepts: Vec<String>,
        #[serde(default)]
        injected_column_names: Vec<String>,
        #[serde(default, rename = "LinkSteps", alias = "linkSteps", alias = "link_steps")]
        link_steps: Vec<LinkStep>,
        #[serde(default, rename = "VirtualColumns", alias = "virtualColumns", alias = "virtual_columns")]
        virtual_columns: Vec<VirtualColumnMapping>,
    },
    CrossTableComparison {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        #[serde(default = "default_true")]
        is_highlighted: bool,
        key_concept: String,
        target_table: String,
        target_key_concept: String,
        compare_concepts: Vec<String>,
        #[serde(default)]
        comparison_type: Option<String>,
        #[serde(default)]
        mismatch_brush_key: Option<String>,
        #[serde(default)]
        mismatch_target: Option<String>,
        #[serde(default)]
        mismatch_priority: Option<u32>,
        #[serde(default)]
        show_proposal_dialog: Option<bool>,
    },
    Highlight {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        #[serde(default = "default_true")]
        is_highlighted: bool,
        column_concept: String,
        condition: String,
        #[serde(default)]
        value_to_match: String,
        #[serde(default)]
        multiple_values: Vec<String>,
        #[serde(default)]
        invert_condition: bool,
        #[serde(default)]
        brush_key: String,
        #[serde(default)]
        target: String,
        #[serde(default)]
        priority: u32,
    },
    Apply {
        id: String,
        description: String,
        #[serde(default)]
        action_group: Option<String>,
        #[serde(default = "default_true")]
        is_active: bool,
        source_column_concepts: Vec<String>,
        #[serde(default)]
        requires_confirmation: bool,
        #[serde(default)]
        execute_automatically: bool,
        target_table: String,
        #[serde(default)]
        key_mappings: Vec<(String, String)>,
        #[serde(default)]
        field_assignments: Vec<(String, String)>,
    },
}

impl RuleDefinition {
    pub fn id(&self) -> &str {
        match self {
            RuleDefinition::BulkField { id, .. } => id,
            RuleDefinition::Calculation { id, .. } => id,
            RuleDefinition::GroupValidation { id, .. } => id,
            RuleDefinition::VirtualRowJoin { id, .. } => id,
            RuleDefinition::CrossTableComparison { id, .. } => id,
            RuleDefinition::Highlight { id, .. } => id,
            RuleDefinition::Apply { id, .. } => id,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            RuleDefinition::BulkField { description, .. } => description,
            RuleDefinition::Calculation { description, .. } => description,
            RuleDefinition::GroupValidation { description, .. } => description,
            RuleDefinition::VirtualRowJoin { description, .. } => description,
            RuleDefinition::CrossTableComparison { description, .. } => description,
            RuleDefinition::Highlight { description, .. } => description,
            RuleDefinition::Apply { description, .. } => description,
        }
    }

    pub fn is_active(&self) -> bool {
        match self {
            RuleDefinition::BulkField { .. } => true,
            RuleDefinition::Calculation { is_active, .. } => *is_active,
            RuleDefinition::GroupValidation { is_active, .. } => *is_active,
            RuleDefinition::VirtualRowJoin { is_active, .. } => *is_active,
            RuleDefinition::CrossTableComparison { is_active, .. } => *is_active,
            RuleDefinition::Highlight { is_active, .. } => *is_active,
            RuleDefinition::Apply { is_active, .. } => *is_active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericRangeCheck {
    pub concept: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConsistencyCheck {
    pub concept: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct KeyMapping {
    #[serde(alias = "hostConcept", alias = "host_concept")]
    pub host_concept: String,
    #[serde(alias = "remoteConcept", alias = "remote_concept")]
    pub remote_concept: String,
    #[serde(default, alias = "manualRemoteValue", alias = "manual_remote_value")]
    pub manual_remote_value: Option<String>,
    #[serde(default, alias = "isCaseSensitive", alias = "is_case_sensitive")]
    pub is_case_sensitive: bool,
}

impl KeyMapping {
    pub fn new(host: &str, remote: &str) -> Self {
        Self {
            host_concept: host.to_string(),
            remote_concept: remote.to_string(),
            manual_remote_value: None,
            is_case_sensitive: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LinkStep {
    #[serde(alias = "targetTableKey", alias = "target_table_key")]
    pub target_table_key: String,
    #[serde(default, alias = "keyMappings", alias = "key_mappings")]
    pub key_mappings: Vec<KeyMapping>,
}

impl LinkStep {
    pub fn new(target: &str) -> Self {
        Self {
            target_table_key: target.to_string(),
            key_mappings: Vec::new(),
        }
    }
}

fn default_display_mode_first() -> String {
    "First".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualColumnMapping {
    #[serde(alias = "remoteColumnConcept", alias = "remote_column_concept")]
    pub remote_column_concept: String,
    #[serde(alias = "virtualColumnHeader", alias = "virtual_column_header")]
    pub virtual_column_header: String,
    #[serde(alias = "virtualColumnKey", alias = "virtual_column_key")]
    pub virtual_column_key: String,
    #[serde(default = "default_display_mode_first", alias = "multiResultDisplay", alias = "multi_result_display")]
    pub multi_result_display: String,
    #[serde(default, alias = "brushKey", alias = "brush_key")]
    pub brush_key: Option<String>,
    #[serde(default, alias = "priority")]
    pub priority: Option<u32>,
}

impl VirtualColumnMapping {
    pub fn sanitize_identifier(s: &str) -> String {
        let clean = s
            .replace('ä', "ae").replace('Ä', "ae")
            .replace('ö', "oe").replace('Ö', "oe")
            .replace('ü', "ue").replace('Ü', "ue")
            .replace('ß', "ss");
        
        let mut res = String::new();
        for c in clean.chars() {
            if c.is_ascii_alphanumeric() || c == '_' {
                res.push(c.to_ascii_lowercase());
            } else if c == ' ' || c == '-' || c == '.' {
                res.push('_');
            }
        }
        while res.contains("__") {
            res = res.replace("__", "_");
        }
        let res = res.trim_matches('_').to_string();
        if res.is_empty() {
            "col".to_string()
        } else if res.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            format!("v_{}", res)
        } else {
            res
        }
    }

    pub fn generate_default_header(concept: &str, target_table: &str, host_table: &str, mode: &str) -> String {
        let tgt = target_table.trim_end_matches(".csv").trim_end_matches(".ods");
        let hst = host_table.trim_end_matches(".csv").trim_end_matches(".ods");
        match mode {
            "Sum" => format!("Summe {} ({} -> {})", concept, tgt, hst),
            "Average" => format!("Mittelwert {} ({} -> {})", concept, tgt, hst),
            "CountSeparated" => format!("Anzahl Eindeutig {} ({} -> {})", concept, tgt, hst),
            "CountMerged" => format!("Anzahl {} ({} -> {})", concept, tgt, hst),
            _ => format!("{} ({} -> {})", concept, tgt, hst),
        }
    }

    pub fn generate_default_key(concept: &str, target_table: &str, host_table: &str, mode: &str) -> String {
        let c_clean = Self::sanitize_identifier(concept);
        let t_clean = Self::sanitize_identifier(target_table.trim_end_matches(".csv").trim_end_matches(".ods"));
        let h_clean = Self::sanitize_identifier(host_table.trim_end_matches(".csv").trim_end_matches(".ods"));
        let m_clean = mode.to_lowercase();
        format!("v_{}_{}_from_{}_to_{}", m_clean, c_clean, t_clean, h_clean)
    }

    pub fn generate_autotext_description(
        rule_name: &str,
        is_active: bool,
        action_group: Option<&str>,
        host_table: &str,
        link_steps: &[LinkStep],
        virtual_columns: &[VirtualColumnMapping],
    ) -> String {
        let mut desc = format!("--- Automatisierte Join-Beschreibung ---\nRegel: {} (Aktiv: {})\nGruppe: {}\n\nVerknüpfungskette:\n",
            rule_name,
            if is_active { "True" } else { "False" },
            action_group.unwrap_or("Standard")
        );

        let mut current_source = host_table.to_string();
        for step in link_steps {
            desc.push_str(&format!(" {} -> {} (via {} Bedingung(en))\n", current_source, step.target_table_key, step.key_mappings.len()));
            for m in &step.key_mappings {
                if let Some(ref man) = m.manual_remote_value {
                    desc.push_str(&format!("  - {} == \"{}\" (Manuell)\n", m.remote_concept, man));
                } else {
                    desc.push_str(&format!("  - {} == {}{}\n", m.host_concept, m.remote_concept, if m.is_case_sensitive { " (Aa)" } else { "" }));
                }
            }
            current_source = step.target_table_key.clone();
        }

        if !virtual_columns.is_empty() {
            desc.push_str("\nVirtuelle Ergebnis-Spalten:\n");
            for vc in virtual_columns {
                desc.push_str(&format!(" - \"{}\" [{}] aus {}.{} (Key: {})\n",
                    vc.virtual_column_header,
                    vc.multi_result_display,
                    current_source,
                    vc.remote_column_concept,
                    vc.virtual_column_key
                ));
            }
        }

        desc
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CsvSettings {
    pub file_name: String,
    pub delimiter: String,
    pub has_header: bool,
    pub encoding: String,
}

impl Default for CsvSettings {
    fn default() -> Self {
        Self {
            file_name: String::new(),
            delimiter: ",".to_string(),
            has_header: true,
            encoding: "utf-8".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InfoFile {
    pub info_file_version: u32,
    pub csv_settings: CsvSettings,
    pub column_mappings: HashMap<String, String>,
    pub column_settings: Vec<ColumnSetting>,
    pub saved_filter_presets: Vec<FilterPreset>,
    pub rules: Vec<RuleDefinition>,
    #[serde(default)]
    pub dirty_cell_brush_key: Option<String>,
    #[serde(default)]
    pub storage_mode_override: Option<String>,
}

impl Default for InfoFile {
    fn default() -> Self {
        Self {
            info_file_version: 1,
            csv_settings: CsvSettings::default(),
            column_mappings: HashMap::new(),
            column_settings: Vec::new(),
            saved_filter_presets: Vec::new(),
            rules: Vec::new(),
            dirty_cell_brush_key: None,
            storage_mode_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ViewFileItem {
    pub relative_path: String,
    #[serde(default)]
    pub full_path: Option<String>,
    #[serde(default)]
    pub info_file_path: Option<String>,
    #[serde(default)]
    pub storage_mode: Option<String>,
}

pub type ViewFileEntry = ViewFileItem;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ViewDefinition {
    pub name: String,
    #[serde(default)]
    pub is_default_view: bool,
    #[serde(default)]
    pub files: Vec<ViewFileItem>,
    #[serde(default)]
    pub spreadsheet_path: Option<String>,
    #[serde(default)]
    pub base_path: Option<String>,
    #[serde(default)]
    pub color_pattern: Option<String>,
    #[serde(default)]
    pub export_color_pattern: Option<String>,
    #[serde(default)]
    pub export_paths: Vec<String>,
    #[serde(default)]
    pub column_widths: HashMap<String, u32>,
}

pub type ViewCollection = ViewDefinition;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ViewsFile {
    pub views: Vec<ViewDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum InfoFileStorageMode {
    KollektionsBasispfad,
    Dateipfad,
    Stammverzeichnis,
    Temporaer,
    BenutzerEntscheidet,
}

impl Default for InfoFileStorageMode {
    fn default() -> Self {
        InfoFileStorageMode::KollektionsBasispfad
    }
}

impl InfoFileStorageMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            InfoFileStorageMode::KollektionsBasispfad => "KollektionsBasispfad",
            InfoFileStorageMode::Dateipfad => "Dateipfad",
            InfoFileStorageMode::Stammverzeichnis => "Stammverzeichnis",
            InfoFileStorageMode::Temporaer => "Temporaer",
            InfoFileStorageMode::BenutzerEntscheidet => "BenutzerEntscheidet",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "kollektionsbasispfad" | "basispfad" | "kollektion" => InfoFileStorageMode::KollektionsBasispfad,
            "stammverzeichnis" | "zentral" => InfoFileStorageMode::Stammverzeichnis,
            "temporaer" | "temporär" => InfoFileStorageMode::Temporaer,
            "benutzerentscheidet" => InfoFileStorageMode::BenutzerEntscheidet,
            "dateipfad" => InfoFileStorageMode::Dateipfad,
            _ => InfoFileStorageMode::KollektionsBasispfad,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushCategory {
    UI,
    Status,
    Textmarker,
    Analyse,
    Logging,
}

impl BrushCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            BrushCategory::UI => "UI",
            BrushCategory::Status => "Status",
            BrushCategory::Textmarker => "Textmarker",
            BrushCategory::Analyse => "Analyse",
            BrushCategory::Logging => "Logging",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ui" => BrushCategory::UI,
            "textmarker" => BrushCategory::Textmarker,
            "analyse" => BrushCategory::Analyse,
            "logging" => BrushCategory::Logging,
            _ => BrushCategory::Status,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct BrushDefinition {
    pub key: String,
    pub hex_value: String,
    pub category: BrushCategory,
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExternalAppConfig {
    pub name: String,
    pub command_or_path: String,
    #[serde(default)]
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AppConfig {
    pub selected_storage_mode: InfoFileStorageMode,
    pub central_config_directory: String,
    pub marker_highlight_priority: u32,
    pub check_file_associations_on_startup: bool,
    pub brushes: Vec<BrushDefinition>,
    #[serde(default = "AppConfig::default_external_apps")]
    pub external_apps: Vec<ExternalAppConfig>,
    #[serde(default)]
    pub tabs_multiline: bool,
    #[serde(default)]
    pub theme: kit_theme::ThemeMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_storage_mode: InfoFileStorageMode::KollektionsBasispfad,
            central_config_directory: "./Configs".to_string(),
            marker_highlight_priority: 100,
            check_file_associations_on_startup: false,
            brushes: Self::default_brushes(),
            external_apps: Self::default_external_apps(),
            tabs_multiline: false,
            theme: kit_theme::ThemeMode::default(),
        }
    }
}

impl AppConfig {
    pub fn default_external_apps() -> Vec<ExternalAppConfig> {
        vec![
            ExternalAppConfig {
                name: "LibreOffice".to_string(),
                command_or_path: r"C:\Program Files\LibreOffice\program\soffice.exe".to_string(),
                arguments: "".to_string(),
            },
            ExternalAppConfig {
                name: "Notepad++".to_string(),
                command_or_path: r"C:\Program Files\Notepad++\notepad++.exe".to_string(),
                arguments: "".to_string(),
            },
        ]
    }
    pub fn default_brushes() -> Vec<BrushDefinition> {
        vec![
            // UI
            BrushDefinition { key: "ZebraRowBrush".into(), hex_value: "#222430".into(), category: BrushCategory::UI, is_system: true },
            BrushDefinition { key: "SelectionBrush".into(), hex_value: "#3949ab".into(), category: BrushCategory::UI, is_system: true },
            BrushDefinition { key: "FilterActiveBrush".into(), hex_value: "#ff9800".into(), category: BrushCategory::UI, is_system: true },
            // Status
            BrushDefinition { key: "ErrorBrush".into(), hex_value: "#ef5350".into(), category: BrushCategory::Status, is_system: true },
            BrushDefinition { key: "WarningBrush".into(), hex_value: "#ffb74d".into(), category: BrushCategory::Status, is_system: true },
            BrushDefinition { key: "ProblemBrush".into(), hex_value: "#ab47bc".into(), category: BrushCategory::Status, is_system: true },
            BrushDefinition { key: "PositiveBrush".into(), hex_value: "#66bb6a".into(), category: BrushCategory::Status, is_system: true },
            BrushDefinition { key: "EmptyCellBrush".into(), hex_value: "#424242".into(), category: BrushCategory::Status, is_system: true },
            BrushDefinition { key: "DirtyCellBrush".into(), hex_value: "#26a69a".into(), category: BrushCategory::Status, is_system: true },
            // Textmarker
            BrushDefinition { key: "MarkerGelb".into(), hex_value: "#fff59d".into(), category: BrushCategory::Textmarker, is_system: true },
            BrushDefinition { key: "MarkerGruen".into(), hex_value: "#c8e6c9".into(), category: BrushCategory::Textmarker, is_system: true },
            BrushDefinition { key: "MarkerBlau".into(), hex_value: "#bbdefb".into(), category: BrushCategory::Textmarker, is_system: true },
            BrushDefinition { key: "MarkerRot".into(), hex_value: "#ffcdd2".into(), category: BrushCategory::Textmarker, is_system: true },
            BrushDefinition { key: "MarkerOrange".into(), hex_value: "#ffe0b2".into(), category: BrushCategory::Textmarker, is_system: true },
            BrushDefinition { key: "MarkerLila".into(), hex_value: "#e1bee7".into(), category: BrushCategory::Textmarker, is_system: true },
            // Analyse
            BrushDefinition { key: "CrossTableMismatchBrush".into(), hex_value: "#e57373".into(), category: BrushCategory::Analyse, is_system: true },
            BrushDefinition { key: "UniquenessViolationBrush".into(), hex_value: "#ba68c8".into(), category: BrushCategory::Analyse, is_system: true },
            // Logging
            BrushDefinition { key: "LogErrorBrush".into(), hex_value: "#ef5350".into(), category: BrushCategory::Logging, is_system: true },
            BrushDefinition { key: "LogWarningBrush".into(), hex_value: "#ffb74d".into(), category: BrushCategory::Logging, is_system: true },
            BrushDefinition { key: "LogInfoBrush".into(), hex_value: "#42a5f5".into(), category: BrushCategory::Logging, is_system: true },
        ]
    }

    pub fn find_brush_hex(&self, key: &str) -> Option<&str> {
        self.brushes.iter().find(|b| b.key.eq_ignore_ascii_case(key)).map(|b| b.hex_value.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub tab_index: usize,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct TableData {
    pub id: String,
    pub file_path: Option<PathBuf>,
    pub display_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub is_virtual_col: Vec<bool>,
    pub column_mappings: HashMap<String, String>,
    pub column_settings: Vec<ColumnSetting>,
    pub marked_rows: HashSet<usize>,
    pub row_markers: HashMap<usize, TextMarkerColor>,
    pub cell_markers: HashMap<(usize, usize), TextMarkerColor>,
    pub dirty_cells: HashSet<(usize, usize)>,
    pub rule_highlights: Vec<HighlightItem>,
    pub rule_summaries: Vec<RuleGroupSummary>,
    pub rule_proposals: Vec<ProposalRow>,
    pub active_filters: HashMap<usize, FilterCondition>,
    pub sort_column: Option<(usize, bool)>, // (col_index, ascending)
    pub is_dirty: bool,
    pub delimiter: char,
    pub encoding: String,
    pub is_ods: bool,
    pub sheet_name: Option<String>,
    pub available_sheets: Vec<String>,
    pub saved_filter_presets: Vec<FilterPreset>,
    pub rules: Vec<RuleDefinition>,
}

impl TableData {
    pub fn new(id: String, display_name: String) -> Self {
        Self {
            id,
            file_path: None,
            display_name,
            headers: Vec::new(),
            rows: Vec::new(),
            is_virtual_col: Vec::new(),
            column_mappings: HashMap::new(),
            column_settings: Vec::new(),
            marked_rows: HashSet::new(),
            row_markers: HashMap::new(),
            cell_markers: HashMap::new(),
            dirty_cells: HashSet::new(),
            rule_highlights: Vec::new(),
            rule_summaries: Vec::new(),
            rule_proposals: Vec::new(),
            active_filters: HashMap::new(),
            sort_column: None,
            is_dirty: false,
            delimiter: ',',
            encoding: "utf-8".to_string(),
            is_ods: false,
            sheet_name: None,
            available_sheets: Vec::new(),
            saved_filter_presets: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// Resolve column index by raw name or by abstract concept
    pub fn find_col_index(&self, name_or_concept: &str) -> Option<usize> {
        let trimmed = name_or_concept.trim();
        if trimmed.is_empty() {
            return None;
        }
        // First check if it matches a concept mapping
        if let Some(raw_name) = self.column_mappings.get(trimmed) {
            if let Some(idx) = self.headers.iter().position(|h| h.eq_ignore_ascii_case(raw_name)) {
                return Some(idx);
            }
        }
        for (k, raw_name) in &self.column_mappings {
            if k.eq_ignore_ascii_case(trimmed) {
                if let Some(idx) = self.headers.iter().position(|h| h.eq_ignore_ascii_case(raw_name)) {
                    return Some(idx);
                }
            }
        }
        // Then check raw header names directly
        self.headers.iter().position(|h| h.eq_ignore_ascii_case(trimmed))
    }

    /// Compute row indices matching all active filters EXCEPT `ignore_col_idx`
    pub fn rows_matching_other_filters(
        &self,
        ignore_col_idx: usize,
        only_marked: bool,
        search_query: &str,
    ) -> Vec<usize> {
        let query_lower = search_query.trim().to_lowercase();
        (0..self.rows.len())
            .filter(|&row_idx| {
                if only_marked && !self.marked_rows.contains(&row_idx) {
                    return false;
                }
                let row = &self.rows[row_idx];
                if !query_lower.is_empty()
                    && !row.iter().any(|cell| cell.to_lowercase().contains(&query_lower))
                {
                    return false;
                }
                for (&col_idx, filter) in &self.active_filters {
                    if col_idx == ignore_col_idx || !filter.is_active() {
                        continue;
                    }
                    let val = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                    if !filter.matches(val) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Ensure that the specified row is not hidden by any active column filter
    pub fn ensure_row_visible_in_filter(&mut self, row_idx: usize) {
        let Some(row) = self.rows.get(row_idx) else {
            return;
        };
        for (&col_idx, filter) in self.active_filters.iter_mut() {
            if !filter.is_active() {
                continue;
            }
            let val = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
            if !filter.matches(val) {
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    filter.include_empty = true;
                    filter.selected_values.insert(String::new());
                } else {
                    filter.selected_values.insert(val.to_string());
                    filter.selected_values.insert(trimmed.to_string());
                    filter.include_non_empty = true;
                }
                if !filter.text_search.is_empty()
                    && !val.to_lowercase().contains(&filter.text_search.to_lowercase())
                {
                    filter.text_search.clear();
                }
            }
        }
    }

    /// Compute filtered and sorted row indices
    pub fn visible_row_indices(&self, only_marked: bool) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.rows.len())
            .filter(|&row_idx| {
                if only_marked && !self.marked_rows.contains(&row_idx) {
                    return false;
                }
                let row = &self.rows[row_idx];
                for (&col_idx, filter) in &self.active_filters {
                    if !filter.is_active() {
                        continue;
                    }
                    let val = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                    if !filter.matches(val) {
                        return false;
                    }
                }
                true
            })
            .collect();

        if let Some((col_idx, ascending)) = self.sort_column {
            indices.sort_by(|&a, &b| {
                let val_a = self.rows[a].get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let val_b = self.rows[b].get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let cmp = natord::compare(val_a, val_b);
                if ascending {
                    cmp
                } else {
                    cmp.reverse()
                }
            });
        }

        indices
    }

    pub fn set_cell(&mut self, row: usize, col: usize, new_val: String) {
        if row < self.rows.len() && col < self.headers.len() {
            if col >= self.rows[row].len() {
                self.rows[row].resize(self.headers.len(), String::new());
            }
            if self.rows[row][col] != new_val {
                self.rows[row][col] = new_val;
                self.dirty_cells.insert((row, col));
                self.is_dirty = true;
            }
        }
    }
}
