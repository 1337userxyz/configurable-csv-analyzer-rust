#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use arboard::Clipboard;
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};

use configurable_csv_analyzer::core::io::csv::{find_all_csvs_in_dir, load_csv, save_csv};
use configurable_csv_analyzer::core::io::info::{
    apply_info_to_table, build_info_from_table, default_config_path, default_info_path_for_table,
    default_views_path, load_app_config, load_info_file, load_views_file, save_app_config,
    save_info_file, save_views_file,
};
use configurable_csv_analyzer::core::io::ods::{
    create_ods_file, list_ods_sheets, load_ods_sheet, open_in_external_app, open_in_libreoffice,
};
use configurable_csv_analyzer::core::color_cache::{cached_color, get_global_color_cache};
use configurable_csv_analyzer::core::logging::{get_global_logger, log_error, log_info};
use configurable_csv_analyzer::core::model::{
    AppConfig, BrushCategory, FilterCondition, HighlightItem, TableData,
    TextMarkerColor, ViewDefinition, ViewFileItem, ViewsFile,
};
use configurable_csv_analyzer::core::rules::RuleEngine;
use configurable_csv_analyzer::core::transforms::{
    build_smart_add_dialog_state, compare_tables, execute_smart_add_rows,
    export_to_clipboard_string, fill_column, load_all_suggestions_for_column,
    paste_clipboard_data, split_column, FillMode,
};

mod dialogs;

slint::include_modules!();

pub struct AppState {
    pub tables: HashMap<String, TableData>,
    pub tab_order: Vec<String>,
    pub active_tab_id: Option<String>,
    pub active_view_name: Option<String>,
    pub selected_collections: HashSet<String>,
    pub search_query: String,
    pub only_marked: bool,
    pub pending_ods_path: Option<PathBuf>,
    pub app_config: AppConfig,
    pub views_config: ViewsFile,
    pub active_view_mode: usize, // 0 = Alle, 1 = CSV, 2 = ODS
    pub temp_collection: Option<ViewDefinition>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            tab_order: Vec::new(),
            active_tab_id: None,
            active_view_name: None,
            selected_collections: HashSet::new(),
            search_query: String::new(),
            only_marked: false,
            pending_ods_path: None,
            app_config: AppConfig::default(),
            views_config: ViewsFile::default(),
            active_view_mode: 0,
            temp_collection: None,
        }
    }

    pub fn handle_files_opened_as_temp_collection(&mut self, opened_file_paths: &[PathBuf]) {
        let temp_name = "Temporäre Kollektion".to_string();
        let mut temp_view = self.temp_collection.clone().unwrap_or_else(|| ViewDefinition {
            name: temp_name.clone(),
            is_default_view: false,
            files: Vec::new(),
            spreadsheet_path: None,
            color_pattern: None,
            export_color_pattern: None,
            export_paths: Vec::new(),
        });

        for p in opened_file_paths {
            let p_str = p.to_string_lossy().to_string();
            let is_ods = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("ods"))
                .unwrap_or(false);

            if is_ods && temp_view.spreadsheet_path.is_none() {
                temp_view.spreadsheet_path = Some(p_str.clone());
            }

            if !temp_view.files.iter().any(|f| f.relative_path == p_str || f.full_path.as_deref() == Some(&p_str)) {
                temp_view.files.push(ViewFileItem {
                    relative_path: p_str.clone(),
                    full_path: Some(p_str),
                    info_file_path: None,
                    storage_mode: Some("Dateipfad".to_string()),
                });
            }
        }

        self.temp_collection = Some(temp_view);
        self.active_view_name = Some(temp_name.clone());
        self.selected_collections.clear();
        self.selected_collections.insert(temp_name);
    }

    pub fn is_table_in_selected_collections(&self, tbl: &TableData) -> bool {
        if self.selected_collections.is_empty() {
            return true;
        }
        for view_name in &self.selected_collections {
            if view_name == "Temporäre Kollektion" {
                if let Some(ref view) = self.temp_collection {
                    if self.table_matches_view(tbl, view) {
                        return true;
                    }
                }
            } else if let Some(view) = self.views_config.views.iter().find(|v| &v.name == view_name) {
                if self.table_matches_view(tbl, view) {
                    return true;
                }
            }
        }
        false
    }

    pub fn table_matches_view(&self, tbl: &TableData, view: &ViewDefinition) -> bool {
        if let Some(ref p) = tbl.file_path {
            let p_norm = p.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
            let p_fname = p.file_name().map(|f| f.to_string_lossy().to_ascii_lowercase());

            for f in &view.files {
                let f_norm = f.relative_path.to_ascii_lowercase().replace('/', "\\");
                if p_norm == f_norm || p_norm.ends_with(&format!("\\{}", f_norm)) || f_norm.ends_with(&format!("\\{}", p_norm)) {
                    return true;
                }
                if let Some(ref full) = f.full_path {
                    let full_norm = full.to_ascii_lowercase().replace('/', "\\");
                    if p_norm == full_norm {
                        return true;
                    }
                }
                if let Some(ref pf) = p_fname {
                    let f_path = std::path::Path::new(&f.relative_path);
                    if let Some(ff) = f_path.file_name().map(|f| f.to_string_lossy().to_ascii_lowercase()) {
                        if pf == &ff {
                            return true;
                        }
                    }
                }
            }

            if let Some(ref sp) = view.spreadsheet_path {
                let sp_norm = sp.to_ascii_lowercase().replace('/', "\\");
                if p_norm == sp_norm || p_norm.ends_with(&format!("\\{}", sp_norm)) || sp_norm.ends_with(&format!("\\{}", p_norm)) {
                    return true;
                }
                if let Some(ref pf) = p_fname {
                    let sp_path = std::path::Path::new(sp);
                    if let Some(sf) = sp_path.file_name().map(|f| f.to_string_lossy().to_ascii_lowercase()) {
                        if pf == &sf {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn add_table(&mut self, mut table: TableData) -> String {
        // Try auto-loading .info file if exists
        if let Some(ref p) = table.file_path {
            let mut info_p = default_info_path_for_table(p);
            if !info_p.exists() {
                // Also check central config directory if configured
                let central = PathBuf::from(&self.app_config.central_config_directory);
                if let Some(fname) = p.file_name() {
                    let candidate = central.join(format!("{}.info", fname.to_string_lossy()));
                    if candidate.exists() {
                        info_p = candidate;
                    }
                }
            }
            if info_p.exists() {
                match load_info_file(&info_p) {
                    Ok(info) => {
                        let rule_count = info.rules.len();
                        apply_info_to_table(&mut table, &info);
                        log_info(
                            "InfoFile",
                            &format!(
                                "Konfiguration automatisch geladen für '{}': {:?} ({} Regeln)",
                                table.display_name, info_p, rule_count
                            ),
                        );
                    }
                    Err(e) => {
                        log_error(
                            "InfoFile",
                            &format!("Fehler beim Laden der Info-Datei {:?}: {}", info_p, e),
                        );
                    }
                }
            }
        }

        let id = table.id.clone();
        if !self.tab_order.contains(&id) {
            self.tab_order.push(id.clone());
        }
        self.tables.insert(id.clone(), table);
        self.active_tab_id = Some(id.clone());

        // Execute rules across all loaded tables so joins and highlights are immediately active
        self.execute_all_rules();

        id
    }

    pub fn activate_and_load_view(&mut self, view_idx: usize) {
        let Some(view) = self.views_config.views.get(view_idx).cloned() else {
            return;
        };
        self.active_view_name = Some(view.name.clone());
        self.selected_collections.insert(view.name.clone());
        log_info(
            "Collection",
            &format!(
                "Aktiviere Kollektion '{}' ({} Dateien)",
                view.name,
                view.files.len()
            ),
        );

        let mut ordered_tab_ids: Vec<String> = Vec::new();

        for file_item in &view.files {
            let p = PathBuf::from(&file_item.relative_path);
            let existing_id = self
                .tables
                .values()
                .find(|t| t.file_path.as_ref() == Some(&p))
                .map(|t| t.id.clone());

            if let Some(id) = existing_id {
                if !ordered_tab_ids.contains(&id) {
                    ordered_tab_ids.push(id);
                }
            } else if p.exists() {
                let is_ods = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("ods"))
                    .unwrap_or(false);
                if is_ods {
                    match list_ods_sheets(&p) {
                        Ok(sheets) => {
                            for sheet in sheets {
                                let sheet_id = format!("{}::{}", p.to_string_lossy(), sheet);
                                let existing_tbl_id = self
                                    .tables
                                    .values()
                                    .find(|t| t.id == sheet_id || (t.file_path.as_ref() == Some(&p) && t.sheet_name.as_deref() == Some(&sheet)))
                                    .map(|t| t.id.clone());

                                if let Some(id) = existing_tbl_id {
                                    if !ordered_tab_ids.contains(&id) {
                                        ordered_tab_ids.push(id);
                                    }
                                } else if let Ok(tbl) = load_ods_sheet(&p, Some(&sheet)) {
                                    let id = self.add_table(tbl);
                                    if !ordered_tab_ids.contains(&id) {
                                        ordered_tab_ids.push(id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log_error(
                                "Collection",
                                &format!("Fehler beim Lesen der ODS-Blätter von {:?}: {}", p, e),
                            );
                        }
                    }
                } else {
                    match load_csv(&p, None, None, true) {
                        Ok(tbl) => {
                            let id = self.add_table(tbl);
                            if !ordered_tab_ids.contains(&id) {
                                ordered_tab_ids.push(id);
                            }
                        }
                        Err(e) => {
                            log_error(
                                "Collection",
                                &format!("Fehler beim Laden von {:?}: {}", p, e),
                            );
                        }
                    }
                }
            }
        }

        // Also load linked spreadsheet_path if configured and not yet open
        if let Some(ref sp) = view.spreadsheet_path {
            if !sp.trim().is_empty() {
                let ods_p = PathBuf::from(sp.trim());
                if ods_p.exists() {
                    match list_ods_sheets(&ods_p) {
                        Ok(sheets) => {
                            for sheet in sheets {
                                let sheet_id = format!("{}::{}", ods_p.to_string_lossy(), sheet);
                                let existing_tbl_id = self
                                    .tables
                                    .values()
                                    .find(|t| t.id == sheet_id || (t.file_path.as_ref() == Some(&ods_p) && t.sheet_name.as_deref() == Some(&sheet)))
                                    .map(|t| t.id.clone());

                                if let Some(id) = existing_tbl_id {
                                    if !ordered_tab_ids.contains(&id) {
                                        ordered_tab_ids.push(id);
                                    }
                                } else if let Ok(tbl) = load_ods_sheet(&ods_p, Some(&sheet)) {
                                    let id = self.add_table(tbl);
                                    if !ordered_tab_ids.contains(&id) {
                                        ordered_tab_ids.push(id);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log_error(
                                "Collection",
                                &format!("Fehler beim Lesen der Blätter von {:?}: {}", ods_p, e),
                            );
                        }
                    }
                }
            }
        }

        // Append any other currently open tabs that were not part of ordered_tab_ids
        for id in &self.tab_order {
            if !ordered_tab_ids.contains(id) {
                ordered_tab_ids.push(id.clone());
            }
        }

        self.tab_order = ordered_tab_ids;
        if self.active_tab_id.is_none()
            || !self
                .active_tab_id
                .as_ref()
                .map(|id| self.tab_order.contains(id))
                .unwrap_or(false)
        {
            self.active_tab_id = self.tab_order.first().cloned();
        }

        self.execute_all_rules();
    }

    pub fn save_tab_order_to_collection(&mut self) {
        let view_idx = if let Some(ref active_name) = self.active_view_name {
            if active_name == "Temporäre Kollektion" {
                return;
            }
            self.views_config
                .views
                .iter()
                .position(|v| &v.name == active_name)
        } else {
            self.views_config
                .views
                .iter()
                .position(|v| v.is_default_view)
                .or_else(|| {
                    if !self.views_config.views.is_empty() {
                        Some(0)
                    } else {
                        None
                    }
                })
        };

        let Some(idx) = view_idx else {
            return;
        };

        let view_name = self.views_config.views[idx].name.clone();
        self.active_view_name = Some(view_name.clone());

        let existing_files = self.views_config.views[idx].files.clone();
        let mut new_files: Vec<ViewFileItem> = Vec::new();
        let mut used_paths: HashSet<String> = HashSet::new();

        for tab_id in &self.tab_order {
            if let Some(tbl) = self.tables.get(tab_id) {
                if tbl.is_ods {
                    // ODS tables from linked spreadsheet_path should not be saved as individual view files
                    let is_in_existing = existing_files.iter().any(|f| {
                        tbl.file_path.as_ref().map(|p| p.to_string_lossy() == f.relative_path).unwrap_or(false)
                    });
                    if !is_in_existing {
                        continue;
                    }
                }
                if let Some(ref p) = tbl.file_path {
                    let p_str = p.to_string_lossy().to_string();
                    if used_paths.contains(&p_str) {
                        continue;
                    }
                    if let Some(existing) = existing_files.iter().find(|f| {
                        f.relative_path == p_str
                            || PathBuf::from(&f.relative_path).file_name() == p.file_name()
                    }) {
                        new_files.push(existing.clone());
                        used_paths.insert(existing.relative_path.clone());
                        used_paths.insert(p_str);
                    } else {
                        let storage_mode_str =
                            self.app_config.selected_storage_mode.as_str().to_string();
                        new_files.push(ViewFileItem {
                            relative_path: p_str.clone(),
                            full_path: Some(p_str.clone()),
                            info_file_path: None,
                            storage_mode: Some(storage_mode_str),
                        });
                        used_paths.insert(p_str);
                    }
                }
            }
        }

        // Keep any unopened files from the collection at the end so they are not lost
        for f in existing_files {
            if !used_paths.contains(&f.relative_path) {
                new_files.push(f);
            }
        }

        self.views_config.views[idx].files = new_files;
        if let Err(e) = save_views_file(&default_views_path(), &self.views_config) {
            log_error(
                "Collection",
                &format!(
                    "Fehler beim Speichern der Tab-Reihenfolge in Kollektion '{}': {}",
                    view_name, e
                ),
            );
        } else {
            log_info(
                "Collection",
                &format!(
                    "Tab-Reihenfolge in Kollektion '{}' gespeichert ({} Dateien).",
                    view_name,
                    self.views_config.views[idx].files.len()
                ),
            );
        }
    }

    pub fn execute_all_rules(&mut self) {
        let ids = self.tab_order.clone();
        for id in ids {
            let all_tbls = self.tables.clone();
            if let Some(tbl) = self.tables.get_mut(&id) {
                if !tbl.rules.is_empty() || tbl.is_virtual_col.iter().any(|&v| v) {
                    RuleEngine::execute(tbl, &all_tbls);
                }
            }
        }
    }

    pub fn active_table(&self) -> Option<&TableData> {
        self.active_tab_id.as_ref().and_then(|id| self.tables.get(id))
    }

    pub fn active_table_mut(&mut self) -> Option<&mut TableData> {
        self.active_tab_id.as_ref().and_then(|id| self.tables.get_mut(id))
    }
}

fn resolve_brush_color(key_or_hex: &str, state: &AppState) -> Color {
    if let Some(hex) = state.app_config.find_brush_hex(key_or_hex) {
        cached_color(hex, 180)
    } else {
        cached_color(key_or_hex, 180)
    }
}

pub fn sync_ui(ui: &MainWindow, state: &AppState) {
    // 0. Sync available views / active collection & marker palette & external apps
    let view_names: Vec<slint::SharedString> = state
        .views_config
        .views
        .iter()
        .map(|v| slint::SharedString::from(&v.name))
        .collect();
    ui.set_available_views(ModelRc::new(VecModel::from(view_names)));
    ui.set_active_view_name(slint::SharedString::from(
        state.active_view_name.as_deref().unwrap_or(""),
    ));

    let mut collection_items: Vec<SlintCollectionFilterItem> = state
        .views_config
        .views
        .iter()
        .map(|v| SlintCollectionFilterItem {
            name: slint::SharedString::from(&v.name),
            file_count: v.files.len() as i32,
            is_selected: state.selected_collections.contains(&v.name),
            is_temporary: false,
        })
        .collect();

    if let Some(ref tc) = state.temp_collection {
        collection_items.push(SlintCollectionFilterItem {
            name: slint::SharedString::from(&tc.name),
            file_count: tc.files.len() as i32,
            is_selected: state.selected_collections.contains(&tc.name),
            is_temporary: true,
        });
    }

    ui.set_has_temporary_collection(state.temp_collection.is_some());
    ui.set_collection_items(ModelRc::new(VecModel::from(collection_items)));

    let summary = if state.selected_collections.is_empty() {
        "Alle".to_string()
    } else if state.selected_collections.len() == 1 {
        state
            .selected_collections
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "Alle".to_string())
    } else {
        format!("{} aktiv", state.selected_collections.len())
    };
    ui.set_collection_filter_summary(slint::SharedString::from(summary));

    let external_apps: Vec<SlintExternalAppItem> = state
        .app_config
        .external_apps
        .iter()
        .enumerate()
        .map(|(idx, a)| SlintExternalAppItem {
            id: idx as i32,
            name: slint::SharedString::from(&a.name),
            command_or_path: slint::SharedString::from(&a.command_or_path),
            arguments: slint::SharedString::from(&a.arguments),
        })
        .collect();
    ui.set_external_apps(ModelRc::new(VecModel::from(external_apps)));

    let marker_choices: Vec<ColorChoiceItem> = state
        .app_config
        .brushes
        .iter()
        .filter(|b| b.category == BrushCategory::Textmarker)
        .map(|b| {
            let label = match b.key.as_str() {
                "MarkerGelb" => "Gelb",
                "MarkerGruen" => "Grün",
                "MarkerBlau" => "Blau",
                "MarkerRot" => "Rot",
                "MarkerOrange" => "Orange",
                "MarkerLila" => "Lila",
                other => other.strip_prefix("Marker").unwrap_or(other),
            };
            ColorChoiceItem {
                key: slint::SharedString::from(label),
                hex_value: slint::SharedString::from(&b.hex_value),
                color: cached_color(&b.hex_value, 255),
            }
        })
        .collect();
    if !marker_choices.is_empty() {
        ui.set_marker_choices(ModelRc::new(VecModel::from(marker_choices)));
    }

    // 1. Sync tabs and view mode (filtered by selected collections)
    let visible_tables: Vec<(&String, &TableData)> = state
        .tab_order
        .iter()
        .filter_map(|id| state.tables.get(id).map(|tbl| (id, tbl)))
        .filter(|(_, tbl)| state.is_table_in_selected_collections(tbl))
        .collect();

    let csv_count = visible_tables.iter().filter(|(_, t)| !t.is_ods).count();
    let ods_count = visible_tables.iter().filter(|(_, t)| t.is_ods).count();
    ui.set_csv_tab_count(csv_count as i32);
    ui.set_ods_tab_count(ods_count as i32);
    ui.set_active_view_mode(state.active_view_mode as i32);
    let mode_label = match state.active_view_mode {
        1 => format!("CSV-Ebene ({} Tabellen)", csv_count),
        2 => format!("ODS-Ebene ({} Blätter)", ods_count),
        _ => format!("Alle Ebenen ({} Tabellen)", csv_count + ods_count),
    };
    ui.set_active_mode_label(slint::SharedString::from(mode_label));

    // Determine effective active tab: if current active tab is not visible, select first visible
    let is_active_visible = state
        .active_tab_id
        .as_ref()
        .map(|act| visible_tables.iter().any(|(id, _)| *id == act))
        .unwrap_or(false);

    let effective_active_id = if is_active_visible {
        state.active_tab_id.as_deref()
    } else {
        visible_tables.first().map(|(id, _)| id.as_str())
    };

    let is_ods_active = effective_active_id
        .and_then(|id| state.tables.get(id))
        .map(|t| t.is_ods)
        .unwrap_or(false);
    ui.set_active_table_is_ods(is_ods_active);

    let mut tab_items = Vec::new();
    let mut csv_tab_items = Vec::new();
    let mut ods_tab_items = Vec::new();

    let total_csv = visible_tables.iter().filter(|(_, t)| !t.is_ods).count() as i32;
    let total_ods = visible_tables.iter().filter(|(_, t)| t.is_ods).count() as i32;
    let mut cur_csv_idx = 0;
    let mut cur_ods_idx = 0;

    for (id, tbl) in &visible_tables {
        let is_tab_active = effective_active_id == Some(id.as_str());
        let error_count = tbl
            .rule_highlights
            .iter()
            .filter(|h| h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error)
            .count();

        let (tab_index, total_tabs) = if tbl.is_ods {
            let idx = cur_ods_idx;
            cur_ods_idx += 1;
            (idx, total_ods)
        } else {
            let idx = cur_csv_idx;
            cur_csv_idx += 1;
            (idx, total_csv)
        };

        let item = TabItem {
            id: slint::SharedString::from(*id),
            title: slint::SharedString::from(&tbl.display_name),
            is_dirty: tbl.is_dirty,
            error_count: error_count as i32,
            is_active: is_tab_active,
            is_ods: tbl.is_ods,
            file_type: slint::SharedString::from(if tbl.is_ods { "ODS" } else { "CSV" }),
            tab_index,
            total_tabs,
        };

        if tbl.is_ods {
            ods_tab_items.push(item.clone());
        } else {
            csv_tab_items.push(item.clone());
        }

        let matches_mode = match state.active_view_mode {
            1 => !tbl.is_ods,
            2 => tbl.is_ods,
            _ => true,
        };
        if matches_mode {
            tab_items.push(item);
        }
    }

    let chunk_tabs = |tabs: &[TabItem]| -> Vec<TabRowItem> {
        let mut rows = Vec::new();
        let mut cur_row = Vec::new();
        let mut cur_chars = 0;
        for tab in tabs {
            let tab_chars = tab.title.chars().count() + 16;
            if !cur_row.is_empty() && (cur_row.len() >= 5 || (cur_chars + tab_chars > 80 && cur_row.len() >= 2)) {
                rows.push(TabRowItem { tabs: ModelRc::new(VecModel::from(cur_row)) });
                cur_row = Vec::new();
                cur_chars = 0;
            }
            cur_chars += tab_chars;
            cur_row.push(tab.clone());
        }
        if !cur_row.is_empty() {
            rows.push(TabRowItem { tabs: ModelRc::new(VecModel::from(cur_row)) });
        }
        rows
    };

    let csv_rows = chunk_tabs(&csv_tab_items);
    let ods_rows = chunk_tabs(&ods_tab_items);

    ui.set_tabs(ModelRc::new(VecModel::from(tab_items)));
    ui.set_csv_tabs(ModelRc::new(VecModel::from(csv_tab_items)));
    ui.set_ods_tabs(ModelRc::new(VecModel::from(ods_tab_items)));
    ui.set_csv_tab_rows(ModelRc::new(VecModel::from(csv_rows)));
    ui.set_ods_tab_rows(ModelRc::new(VecModel::from(ods_rows)));
    ui.set_tabs_multiline(state.app_config.tabs_multiline);
    ui.set_active_tab_id(slint::SharedString::from(effective_active_id.unwrap_or("")));

    // 2. Active table sync
    let active_tbl = effective_active_id.and_then(|id| state.tables.get(id));
    if let Some(tbl) = active_tbl {
        // Headers (respecting visibility from column_settings)
        let mut header_items = Vec::new();
        for (idx, h) in tbl.headers.iter().enumerate() {
            let concept = tbl
                .column_mappings
                .iter()
                .find(|(_, target_col)| target_col.eq_ignore_ascii_case(h))
                .map(|(concept, _)| concept.as_str())
                .unwrap_or("");

            let sort_dir = match tbl.sort_column {
                Some((c, true)) if c == idx => 1,
                Some((c, false)) if c == idx => 2,
                _ => 0,
            };

            let is_filtered = tbl.active_filters.get(&idx).map(|f| f.is_active()).unwrap_or(false);
            let setting = tbl
                .column_settings
                .iter()
                .find(|s| s.column_name.eq_ignore_ascii_case(h))
                .or_else(|| tbl.column_settings.get(idx));
            let width = setting.map(|s| s.width).unwrap_or(150) as f32;

            header_items.push(HeaderItem {
                id: idx as i32,
                name: slint::SharedString::from(h),
                concept: slint::SharedString::from(concept),
                sort_dir,
                is_filtered,
                width,
            });
        }
        ui.set_headers(ModelRc::new(VecModel::from(header_items)));

        // Compute visible row indices
        let mut visible_indices = tbl.visible_row_indices(state.only_marked);

        // Apply quick search query if set
        if !state.search_query.is_empty() {
            let query = state.search_query.to_lowercase();
            visible_indices.retain(|&r_idx| {
                if let Some(row) = tbl.rows.get(r_idx) {
                    row.iter().any(|cell| cell.to_lowercase().contains(&query))
                } else {
                    false
                }
            });
        }

        // Pre-index rule highlights by row for fast lookup
        let mut row_hl_map: HashMap<usize, Vec<&HighlightItem>> = HashMap::new();
        for h in &tbl.rule_highlights {
            row_hl_map.entry(h.row).or_default().push(h);
        }

        // Map visible rows to Slint RowItems
        let mut row_items = Vec::new();
        for (view_row_idx, &orig_row_idx) in visible_indices.iter().enumerate() {
            if let Some(row) = tbl.rows.get(orig_row_idx) {
                let is_marked = tbl.marked_rows.contains(&orig_row_idx);
                let row_marker = tbl.row_markers.get(&orig_row_idx);
                let row_highlights = row_hl_map.get(&orig_row_idx);

                // Highest priority row-level highlight (col == None)
                let best_row_hl = row_highlights.and_then(|list| {
                    list.iter()
                        .filter(|h| h.col.is_none() && h.brush_key.is_some())
                        .max_by_key(|h| h.priority)
                        .copied()
                });

                let row_rule_color = best_row_hl
                    .and_then(|h| h.brush_key.as_deref())
                    .map(|bk| resolve_brush_color(bk, state))
                    .unwrap_or_else(|| Color::from_argb_u8(0, 0, 0, 0));

                let mut cell_items = Vec::new();
                for (col_idx, cell_text) in row.iter().enumerate() {
                    let has_error = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error
                        })
                    }).unwrap_or(false);

                    let has_warning = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Warning
                        })
                    }).unwrap_or(false);

                    let has_info = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Info
                        })
                    }).unwrap_or(false);

                    let best_cell_hl = row_highlights.and_then(|list| {
                        list.iter()
                            .filter(|h| h.col == Some(col_idx) && h.brush_key.is_some())
                            .max_by_key(|h| h.priority)
                            .copied()
                    });

                    let cell_marker = tbl.cell_markers.get(&(orig_row_idx, col_idx));
                    let is_cell_dirty = tbl.dirty_cells.contains(&(orig_row_idx, col_idx));
                    let bg_color = if let Some(m) = cell_marker {
                        parse_hex_color(m.hex())
                    } else if let Some(m) = row_marker {
                        parse_hex_color(m.hex())
                    } else if let Some(ch) = best_cell_hl {
                        if let Some(ref bk) = ch.brush_key {
                            resolve_brush_color(bk, state)
                        } else {
                            row_rule_color
                        }
                    } else if row_rule_color.alpha() > 0 {
                        row_rule_color
                    } else if is_cell_dirty {
                        resolve_brush_color("DirtyCellBrush", state)
                    } else {
                        row_rule_color
                    };

                    cell_items.push(CellItem {
                        text: slint::SharedString::from(cell_text),
                        bg_color,
                        has_error,
                        has_warning,
                        has_info,
                    });
                }

                let row_bg = if let Some(m) = row_marker {
                    parse_hex_color(m.hex())
                } else {
                    row_rule_color
                };

                row_items.push(RowItem {
                    row_idx: view_row_idx as i32,
                    original_idx: orig_row_idx as i32,
                    is_marked,
                    bg_color: row_bg,
                    cells: ModelRc::new(VecModel::from(cell_items)),
                });
            }
        }
        ui.set_rows(ModelRc::new(VecModel::from(row_items)));
        ui.set_total_row_count(tbl.rows.len() as i32);
        ui.set_marked_row_count(tbl.marked_rows.len() as i32);
        ui.set_only_marked_active(state.only_marked);

        // Sync Rule Summaries & Proposals for active table
        let summaries: Vec<RuleSummaryItem> = tbl
            .rule_summaries
            .iter()
            .map(|s| RuleSummaryItem {
                id: slint::SharedString::from(&s.rule_id),
                description: slint::SharedString::from(&s.description),
                hits: s.hit_count as i32,
                errors: s.error_count as i32,
                warnings: s.warning_count as i32,
            })
            .collect();
        ui.set_rule_summaries(ModelRc::new(VecModel::from(summaries)));

        let proposals: Vec<ProposalItem> = tbl
            .rule_proposals
            .iter()
            .enumerate()
            .map(|(i, p)| ProposalItem {
                source_table: slint::SharedString::from(&p.source_table),
                key_val: slint::SharedString::from(&p.key_value),
                preview: slint::SharedString::from(p.row_data.join(", ")),
                row_idx: i as i32,
            })
            .collect();
        ui.set_proposals(ModelRc::new(VecModel::from(proposals)));
    } else {
        ui.set_headers(ModelRc::new(VecModel::default()));
        ui.set_rows(ModelRc::new(VecModel::default()));
        ui.set_total_row_count(0);
        ui.set_marked_row_count(0);
        ui.set_rule_summaries(ModelRc::new(VecModel::default()));
        ui.set_proposals(ModelRc::new(VecModel::default()));
    }

    // 3. Sync Logs
    let log_entries = get_global_logger().get_entries(None);
    let log_items: Vec<LogViewItem> = log_entries
        .into_iter()
        .rev()
        .take(100)
        .map(|e| LogViewItem {
            timestamp: slint::SharedString::from(&e.timestamp),
            level: slint::SharedString::from(e.level.as_str()),
            message: slint::SharedString::from(&format!("{}: {}", e.target, e.message)),
        })
        .collect();
    ui.set_logs(ModelRc::new(VecModel::from(log_items)));
}

fn parse_hex_color(hex: &str) -> Color {
    cached_color(hex, 180)
}

fn main() -> Result<(), slint::PlatformError> {
    let app_config = load_app_config(&default_config_path()).unwrap_or_default();
    if let Ok(mut cache) = get_global_color_cache().lock() {
        cache.update_from_brushes(&app_config.brushes);
    }
    let views_config = load_views_file(&default_views_path()).unwrap_or_default();

    let mut initial_state = AppState::new();
    initial_state.app_config = app_config;
    initial_state.views_config = views_config;

    log_info("App", "Configurable CSV & ODS Analyzer gestartet.");

    // Check if there is a default view (or first view) and auto-load its files on startup
    if let Some(default_idx) = initial_state
        .views_config
        .views
        .iter()
        .position(|v| v.is_default_view)
        .or_else(|| {
            if !initial_state.views_config.views.is_empty() {
                Some(0)
            } else {
                None
            }
        })
    {
        initial_state.activate_and_load_view(default_idx);
    }

    let main_window = MainWindow::new()?;
    let config_window = Rc::new(ConfigurationWindow::new()?);
    let info_editor_window = Rc::new(InfoFileEditorWindow::new()?);
    let state = Rc::new(RefCell::new(initial_state));

    // Setup dialogs
    dialogs::configuration::setup_configuration_window(
        config_window.clone(),
        info_editor_window.clone(),
        main_window.as_weak(),
        state.clone(),
    );

    dialogs::info_editor::setup_info_editor_window(
        info_editor_window.clone(),
        main_window.as_weak(),
        state.clone(),
    );

    // --- SHOW CONFIGURATION WINDOW ---
    {
        let config_win = config_window.clone();
        let state = state.clone();
        main_window.on_show_config_clicked(move || {
            {
                let mut st = state.borrow_mut();
                dialogs::configuration::ensure_selected_view(&config_win, &mut st);
                dialogs::configuration::sync_configuration_ui(&config_win, &st);
            }
            let _ = config_win.show();
        });
    }

    // --- SHOW INFO FILE EDITOR WINDOW ---
    {
        let info_win = info_editor_window.clone();
        let state = state.clone();
        let main_ui = main_window.as_weak();
        main_window.on_show_info_editor_clicked(move || {
            dialogs::info_editor::open_info_editor_for_table_or_path(&info_win, None, &state, main_ui.clone());
            let _ = info_win.show();
        });
    }

    // --- CSV ÖFFNEN (Mehrfachauswahl) ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_csv_clicked(move || {
            let files = rfd::FileDialog::new()
                .set_title("CSV-Dateien öffnen (Mehrfachauswahl möglich)")
                .add_filter("CSV Dateien", &["csv", "tsv", "txt"])
                .add_filter("Alle Dateien", &["*"])
                .pick_files();

            if let Some(paths) = files {
                let mut loaded_count = 0usize;
                let mut last_name = String::new();
                let mut loaded_paths = Vec::new();
                let mut st = state.borrow_mut();
                for p in paths {
                    match load_csv(&p, None, None, true) {
                        Ok(table) => {
                            log_info("CSV", &format!("CSV erfolgreich geladen: {:?}", p));
                            last_name = p
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            st.add_table(table);
                            loaded_paths.push(p);
                            loaded_count += 1;
                        }
                        Err(e) => {
                            log_error("CSV", &format!("Fehler beim Laden von {:?}: {}", p, e));
                            if let Some(ui) = ui_handle.upgrade() {
                                ui.set_status_message(slint::SharedString::from(format!(
                                    "Fehler: {}",
                                    e
                                )));
                            }
                        }
                    }
                }
                if loaded_count > 0 {
                    st.handle_files_opened_as_temp_collection(&loaded_paths);
                    if let Some(ui) = ui_handle.upgrade() {
                        let msg = if loaded_count == 1 {
                            format!("Geladen als temporäre Kollektion: {}", last_name)
                        } else {
                            format!("{} CSV-Dateien als temporäre Kollektion geladen", loaded_count)
                        };
                        ui.set_status_message(slint::SharedString::from(msg));
                        sync_ui(&ui, &st);
                    }
                }
            }
        });
    }

    // --- ODS ÖFFNEN ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_ods_clicked(move || {
            let file = rfd::FileDialog::new()
                .set_title("ODS-Datei öffnen")
                .add_filter("LibreOffice Calc (.ods)", &["ods"])
                .add_filter("Alle Dateien", &["*"])
                .pick_file();

            if let Some(p) = file {
                match list_ods_sheets(&p) {
                    Ok(sheets) => {
                        if sheets.len() > 1 {
                            let mut st = state.borrow_mut();
                            st.pending_ods_path = Some(p.clone());
                            if let Some(ui) = ui_handle.upgrade() {
                                let mut sheet_model: Vec<slint::SharedString> = Vec::new();
                                sheet_model.push(slint::SharedString::from("⭐ Alle Blätter importieren"));
                                for s in &sheets {
                                    sheet_model.push(slint::SharedString::from(s));
                                }
                                ui.set_available_sheets(ModelRc::new(VecModel::from(sheet_model)));
                                ui.set_sheet_dialog_visible(true);
                            }
                        } else {
                            match load_ods_sheet(&p, None) {
                                Ok(table) => {
                                    log_info("ODS", &format!("ODS erfolgreich geladen: {:?}", p));
                                    let mut st = state.borrow_mut();
                                    st.add_table(table);
                                    st.handle_files_opened_as_temp_collection(&[p.clone()]);
                                    if let Some(ui) = ui_handle.upgrade() {
                                        ui.set_status_message(slint::SharedString::from(format!(
                                            "ODS geladen als temporäre Kollektion: {:?}",
                                            p.file_name().unwrap_or_default()
                                        )));
                                        sync_ui(&ui, &st);
                                    }
                                }
                                Err(e) => {
                                    log_error(
                                        "ODS",
                                        &format!("Fehler beim Laden von {:?}: {}", p, e),
                                    );
                                    if let Some(ui) = ui_handle.upgrade() {
                                        ui.set_status_message(slint::SharedString::from(format!(
                                            "Fehler: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log_error(
                            "ODS",
                            &format!("Fehler beim Lesen der Blätter von {:?}: {}", p, e),
                        );
                        if let Some(ui) = ui_handle.upgrade() {
                            ui.set_status_message(slint::SharedString::from(format!(
                                "Fehler: {}",
                                e
                            )));
                        }
                    }
                }
            }
        });
    }

    // --- ODS BLATT AUSWÄHLEN ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_select_ods_sheet_clicked(move |sheet_name| {
            let mut st = state.borrow_mut();
            if let Some(p) = st.pending_ods_path.take() {
                if sheet_name == "⭐ Alle Blätter importieren" {
                    if let Ok(sheets) = list_ods_sheets(&p) {
                        let mut loaded_cnt = 0;
                        for s in sheets {
                            if let Ok(table) = load_ods_sheet(&p, Some(&s)) {
                                st.add_table(table);
                                loaded_cnt += 1;
                            }
                        }
                        log_info("ODS", &format!("{} Blätter aus {:?} geladen", loaded_cnt, p));
                        st.handle_files_opened_as_temp_collection(&[p.clone()]);
                        if let Some(ui) = ui_handle.upgrade() {
                            ui.set_status_message(slint::SharedString::from(format!("{} ODS-Blätter als temporäre Kollektion importiert", loaded_cnt)));
                            sync_ui(&ui, &st);
                        }
                    }
                } else {
                    match load_ods_sheet(&p, Some(sheet_name.as_str())) {
                        Ok(table) => {
                            log_info("ODS", &format!("ODS Blatt '{}' geladen aus {:?}", sheet_name, p));
                            st.add_table(table);
                            st.handle_files_opened_as_temp_collection(&[p.clone()]);
                            if let Some(ui) = ui_handle.upgrade() {
                                ui.set_status_message(slint::SharedString::from(format!("ODS Blatt '{}' als temporäre Kollektion geladen", sheet_name)));
                                sync_ui(&ui, &st);
                            }
                        }
                        Err(e) => {
                            log_error("ODS", &format!("Fehler beim Laden des Blattes: {}", e));
                        }
                    }
                }
            }
        });
    }

    // --- ORDNER IMPORTIEREN ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_folder_clicked(move || {
            let folder = rfd::FileDialog::new()
                .set_title("Ordner mit CSV-Dateien auswählen")
                .pick_folder();

            if let Some(f) = folder {
                let csvs = find_all_csvs_in_dir(&f);
                log_info("Import", &format!("{} CSV-Dateien in {:?} gefunden.", csvs.len(), f));
                let mut loaded_paths = Vec::new();
                let mut st = state.borrow_mut();
                for p in csvs {
                    if let Ok(tbl) = load_csv(&p, None, None, true) {
                        st.add_table(tbl);
                        loaded_paths.push(p);
                    }
                }
                if !loaded_paths.is_empty() {
                    st.handle_files_opened_as_temp_collection(&loaded_paths);
                }
                st.execute_all_rules();
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_status_message(slint::SharedString::from(format!("{} CSV-Dateien als temporäre Kollektion geladen", loaded_paths.len())));
                    sync_ui(&ui, &st);
                }
            }
        });
    }

    // --- VIEW MODE CHANGED (0: Alle, 1: CSV, 2: ODS) ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_view_mode_changed(move |mode_idx| {
            let mut st = state.borrow_mut();
            st.active_view_mode = mode_idx as usize;

            let cur_matches = if let Some(ref cur_id) = st.active_tab_id {
                if let Some(tbl) = st.tables.get(cur_id) {
                    match st.active_view_mode {
                        1 => !tbl.is_ods,
                        2 => tbl.is_ods,
                        _ => true,
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if !cur_matches {
                let first_matching = st
                    .tab_order
                    .iter()
                    .find(|id| {
                        if let Some(tbl) = st.tables.get(*id) {
                            match st.active_view_mode {
                                1 => !tbl.is_ods,
                                2 => tbl.is_ods,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    })
                    .cloned();
                if first_matching.is_some() {
                    st.active_tab_id = first_matching;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- EXPORT: CSV -> ODS ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_export_csv_to_ods_clicked(move || {
            let st = state.borrow();
            let csv_tables: Vec<TableData> = st
                .tab_order
                .iter()
                .filter_map(|id| st.tables.get(id))
                .filter(|t| !t.is_ods)
                .cloned()
                .collect();

            if csv_tables.is_empty() {
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_status_message(slint::SharedString::from(
                        "Keine CSV-Tabellen zum Exportieren vorhanden.",
                    ));
                }
                return;
            }

            let linked_ods = if let Some(ref view_name) = st.active_view_name {
                st.views_config
                    .views
                    .iter()
                    .find(|v| &v.name == view_name)
                    .and_then(|v| v.spreadsheet_path.clone())
            } else {
                None
            };

            let target_path = if let Some(ref p_str) = linked_ods {
                if !p_str.trim().is_empty() {
                    Some(PathBuf::from(p_str.trim()))
                } else {
                    None
                }
            } else {
                None
            };

            let final_path = match target_path {
                Some(p) => p,
                None => {
                    let chosen = rfd::FileDialog::new()
                        .set_title("Ziel-ODS-Datei für CSV-Export auswählen")
                        .add_filter("LibreOffice Calc (.ods)", &["ods"])
                        .save_file();
                    match chosen {
                        Some(mut p) => {
                            if p.extension().is_none() {
                                p.set_extension("ods");
                            }
                            p
                        }
                        None => return,
                    }
                }
            };

            match create_ods_file(&final_path, &csv_tables) {
                Ok(_) => {
                    let msg = format!(
                        "✓ {} CSV-Tabelle(n) erfolgreich in ODS-Arbeitsmappe '{:?}' exportiert.",
                        csv_tables.len(),
                        final_path.file_name().unwrap_or_default()
                    );
                    log_info("Export", &msg);
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_status_message(slint::SharedString::from(msg));
                        sync_ui(&ui, &st);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Fehler beim CSV ➔ ODS Export: {}", e);
                    log_error("Export", &err_msg);
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_status_message(slint::SharedString::from(err_msg));
                    }
                }
            }
        });
    }

    // --- EXPORT: ODS -> CSV ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_export_ods_to_csv_clicked(move || {
            let st = state.borrow();
            let ods_tables: Vec<TableData> = st
                .tab_order
                .iter()
                .filter_map(|id| st.tables.get(id))
                .filter(|t| t.is_ods)
                .cloned()
                .collect();

            if ods_tables.is_empty() {
                drop(st);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_status_message(slint::SharedString::from(
                        "Keine ODS-Tabellenblätter zum Exportieren geöffnet.",
                    ));
                }
                return;
            }
            drop(st);

            let folder = rfd::FileDialog::new()
                .set_title("Zielordner für CSV-Export auswählen")
                .pick_folder();

            if let Some(dir) = folder {
                let mut exported_count = 0;
                for tbl in &ods_tables {
                    let sheet_name = tbl
                        .sheet_name
                        .clone()
                        .unwrap_or_else(|| tbl.display_name.clone());
                    let clean_name = sheet_name.replace(
                        ['/', '\\', ':', '*', '?', '"', '<', '>', '|', ' '],
                        "_",
                    );
                    let out_path = dir.join(format!("{}.csv", clean_name));
                    if let Err(e) = save_csv(&out_path, tbl, ';', "utf-8") {
                        log_error(
                            "Export",
                            &format!("Fehler beim Exportieren von Blatt '{}': {}", sheet_name, e),
                        );
                    } else {
                        exported_count += 1;
                    }
                }
                let msg = format!(
                    "✓ {} Tabellenblatt/-blätter erfolgreich als CSV nach '{:?}' exportiert.",
                    exported_count,
                    dir.file_name().unwrap_or_default()
                );
                log_info("Export", &msg);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_status_message(slint::SharedString::from(msg));
                }
            }
        });
    }

    // --- SPEICHERN ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_save_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                if let Some(ref p) = tbl.file_path.clone() {
                    match save_csv(p, tbl, tbl.delimiter, &tbl.encoding) {
                        Ok(_) => {
                            tbl.is_dirty = false;
                            // Also save .info file
                            let info = build_info_from_table(tbl);
                            let info_p = default_info_path_for_table(p);
                            let _ = save_info_file(&info_p, &info);

                            log_info("Save", &format!("Erfolgreich gespeichert: {:?}", p));
                            if let Some(ui) = ui_handle.upgrade() {
                                ui.set_status_message(slint::SharedString::from(format!("Gespeichert: {:?}", p.file_name().unwrap_or_default())));
                                sync_ui(&ui, &st);
                            }
                        }
                        Err(e) => {
                            log_error("Save", &format!("Fehler beim Speichern: {}", e));
                        }
                    }
                }
            }
        });
    }

    // --- REVERT ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_revert_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table() {
                if let Some(p) = tbl.file_path.clone() {
                    let is_ods = tbl.is_ods;
                    let sheet_name = tbl.sheet_name.clone();
                    let reloaded = if is_ods {
                        load_ods_sheet(&p, sheet_name.as_deref())
                    } else {
                        load_csv(&p, Some(tbl.delimiter), Some(&tbl.encoding), true)
                    };

                    if let Ok(new_tbl) = reloaded {
                        log_info("Revert", &format!("Tabelle zurückgesetzt auf Stand der Datei {:?}", p));
                        st.add_table(new_tbl);
                        if let Some(ui) = ui_handle.upgrade() {
                            sync_ui(&ui, &st);
                        }
                    }
                }
            }
        });
    }

    // --- LIBREOFFICE & ÖFFNEN MIT ---
    {
        let state = state.clone();
        main_window.on_open_libreoffice_clicked(move || {
            let st = state.borrow();
            if let Some(tbl) = st.active_table() {
                if let Some(ref p) = tbl.file_path {
                    match open_in_libreoffice(p) {
                        Ok(_) => log_info("LibreOffice", &format!("In LibreOffice geöffnet: {:?}", p)),
                        Err(e) => log_error("LibreOffice", &format!("Fehler: {}", e)),
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        main_window.on_open_with_app_clicked(move |app_idx| {
            let st = state.borrow();
            let i = app_idx as usize;
            if let Some(app) = st.app_config.external_apps.get(i) {
                if let Some(tbl) = st.active_table() {
                    if let Some(ref p) = tbl.file_path {
                        match open_in_external_app(app, p) {
                            Ok(_) => log_info("ExternalApp", &format!("Datei {:?} in '{}' geöffnet.", p, app.name)),
                            Err(e) => log_error("ExternalApp", &format!("Fehler beim Öffnen in '{}': {}", app.name, e)),
                        }
                    }
                }
            }
        });
    }

    // --- TAB SELECTION, REORDERING, VIEW SWITCHING & CLOSE ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_tab_selected(move |tab_id| {
            let mut st = state.borrow_mut();
            st.active_tab_id = Some(tab_id.to_string());
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_tab_move_left(move |tab_id| {
            let mut st = state.borrow_mut();
            if let Some(pos) = st.tab_order.iter().position(|id| id == tab_id.as_str()) {
                let is_ods = st.tables.get(tab_id.as_str()).map(|t| t.is_ods).unwrap_or(false);
                let prev_pos = st.tab_order[..pos]
                    .iter()
                    .rposition(|id| st.tables.get(id).map(|t| t.is_ods == is_ods).unwrap_or(false));
                if let Some(target_pos) = prev_pos {
                    st.tab_order.swap(pos, target_pos);
                    st.save_tab_order_to_collection();
                    if let Some(ui) = ui_handle.upgrade() {
                        sync_ui(&ui, &st);
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_tab_move_right(move |tab_id| {
            let mut st = state.borrow_mut();
            if let Some(pos) = st.tab_order.iter().position(|id| id == tab_id.as_str()) {
                let is_ods = st.tables.get(tab_id.as_str()).map(|t| t.is_ods).unwrap_or(false);
                let next_pos = st.tab_order[pos + 1..]
                    .iter()
                    .position(|id| st.tables.get(id).map(|t| t.is_ods == is_ods).unwrap_or(false))
                    .map(|offset| pos + 1 + offset);
                if let Some(target_pos) = next_pos {
                    st.tab_order.swap(pos, target_pos);
                    st.save_tab_order_to_collection();
                    if let Some(ui) = ui_handle.upgrade() {
                        sync_ui(&ui, &st);
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_view_selected(move |view_name| {
            let mut st = state.borrow_mut();
            if let Some(idx) = st
                .views_config
                .views
                .iter()
                .position(|v| v.name == view_name.as_str())
            {
                st.activate_and_load_view(idx);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_status_message(slint::SharedString::from(format!(
                        "Kollektion '{}' geladen.",
                        view_name
                    )));
                    sync_ui(&ui, &st);
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_collection_single_clicked(move |view_name| {
            let mut st = state.borrow_mut();
            st.selected_collections.clear();
            st.selected_collections.insert(view_name.to_string());
            if let Some(idx) = st.views_config.views.iter().position(|v| v.name == view_name.as_str()) {
                st.activate_and_load_view(idx);
            }
            st.active_view_name = Some(view_name.to_string());
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message(slint::SharedString::from(format!(
                    "Filter auf Kollektion '{}' gesetzt.",
                    view_name
                )));
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_collection_toggled(move |view_name, checked| {
            let mut st = state.borrow_mut();
            if checked {
                st.selected_collections.insert(view_name.to_string());
                if let Some(idx) = st.views_config.views.iter().position(|v| v.name == view_name.as_str()) {
                    st.activate_and_load_view(idx);
                }
                st.active_view_name = Some(view_name.to_string());
            } else {
                st.selected_collections.remove(view_name.as_str());
                if st.selected_collections.is_empty() {
                    st.active_view_name = None;
                } else {
                    st.active_view_name = st.selected_collections.iter().next().cloned();
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_collection_clear_filter(move || {
            let mut st = state.borrow_mut();
            st.selected_collections.clear();
            st.active_view_name = None;
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message(slint::SharedString::from("Alle Kollektionen werden angezeigt (Filter aufgehoben)."));
                sync_ui(&ui, &st);
            }
        });
    }

    // --- TABS MULTILINE TOGGLE ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_toggle_tabs_multiline(move || {
            let mut st = state.borrow_mut();
            st.app_config.tabs_multiline = !st.app_config.tabs_multiline;
            let _ = save_app_config(&default_config_path(), &st.app_config);
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_tabs_multiline(st.app_config.tabs_multiline);
                sync_ui(&ui, &st);
            }
        });
    }

    // --- SAVE COLLECTION DIALOG ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_show_save_collection_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let default_name = {
                    let st = state.borrow();
                    if let Some(ref tc) = st.temp_collection {
                        if tc.files.len() == 1 {
                            std::path::Path::new(&tc.files[0].relative_path)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Neue Kollektion".to_string())
                        } else {
                            format!("Kollektion {}", st.views_config.views.len() + 1)
                        }
                    } else {
                        format!("Kollektion {}", st.views_config.views.len() + 1)
                    }
                };
                ui.set_save_collection_name_input(slint::SharedString::from(default_name));
                ui.set_save_collection_dialog_visible(true);
            }
        });
    }

    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_save_collection_confirmed(move |name| {
            let name_str = name.trim().to_string();
            if name_str.is_empty() {
                return;
            }
            let mut st = state.borrow_mut();
            let mut view_to_save = if let Some(mut tc) = st.temp_collection.take() {
                tc.name = name_str.clone();
                tc
            } else {
                let current_files: Vec<ViewFileItem> = st.tables.values()
                    .filter_map(|tbl| tbl.file_path.as_ref())
                    .map(|p| {
                        let p_str = p.to_string_lossy().to_string();
                        ViewFileItem {
                            relative_path: p_str.clone(),
                            full_path: Some(p_str),
                            info_file_path: None,
                            storage_mode: Some("Dateipfad".to_string()),
                        }
                    })
                    .collect();
                ViewDefinition {
                    name: name_str.clone(),
                    is_default_view: false,
                    files: current_files,
                    spreadsheet_path: None,
                    color_pattern: None,
                    export_color_pattern: None,
                    export_paths: Vec::new(),
                }
            };
            view_to_save.name = name_str.clone();
            if let Some(existing) = st.views_config.views.iter_mut().find(|v| v.name == name_str) {
                *existing = view_to_save;
            } else {
                st.views_config.views.push(view_to_save);
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            st.active_view_name = Some(name_str.clone());
            st.selected_collections.clear();
            st.selected_collections.insert(name_str.clone());
            log_info("Collection", &format!("Kollektion '{}' gespeichert.", name_str));
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_status_message(slint::SharedString::from(format!(
                    "Kollektion '{}' erfolgreich gespeichert.",
                    name_str
                )));
                sync_ui(&ui, &st);
            }
        });
    }

    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_tab_close_clicked(move |tab_id| {
            let mut st = state.borrow_mut();
            st.tab_order.retain(|id| id != tab_id.as_str());
            st.tables.remove(tab_id.as_str());
            if st.active_tab_id.as_deref() == Some(tab_id.as_str()) {
                st.active_tab_id = st.tab_order.last().cloned();
            }
            let open_paths: HashSet<String> = st.tables.values()
                .filter_map(|t| t.file_path.as_ref().map(|p| p.to_string_lossy().to_string()))
                .collect();
            let mut remove_temp = false;
            if let Some(ref mut tc) = st.temp_collection {
                tc.files.retain(|f| open_paths.contains(&f.relative_path) || f.full_path.as_ref().map(|fp| open_paths.contains(fp)).unwrap_or(false));
                if tc.files.is_empty() {
                    remove_temp = true;
                }
            }
            if remove_temp {
                st.temp_collection = None;
                if st.active_view_name.as_deref() == Some("Temporäre Kollektion") {
                    st.active_view_name = None;
                }
                st.selected_collections.remove("Temporäre Kollektion");
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- SELECTION & MARKERS ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_select_all_toggled(move |checked| {
            let mut st = state.borrow_mut();
            let only_marked = st.only_marked;
            if let Some(tbl) = st.active_table_mut() {
                if checked {
                    let visible = tbl.visible_row_indices(only_marked);
                    for r in visible {
                        tbl.marked_rows.insert(r);
                    }
                } else {
                    tbl.marked_rows.clear();
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_only_marked_toggled(move |checked| {
            let mut st = state.borrow_mut();
            st.only_marked = checked;
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_row_mark_toggled(move |orig_idx, checked| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                if checked {
                    tbl.marked_rows.insert(orig_idx as usize);
                } else {
                    tbl.marked_rows.remove(&(orig_idx as usize));
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_mark_empty_rows_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                for (i, row) in tbl.rows.iter().enumerate() {
                    if row.iter().all(|c| c.trim().is_empty()) {
                        tbl.marked_rows.insert(i);
                    }
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_clear_selection_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                tbl.marked_rows.clear();
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_apply_marker_color(move |hex| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let color = match hex.as_str() {
                    "#fff59d" => TextMarkerColor::Yellow,
                    "#c8e6c9" => TextMarkerColor::Green,
                    "#bbdefb" => TextMarkerColor::Blue,
                    "#ffcdd2" => TextMarkerColor::Red,
                    "#ffe0b2" => TextMarkerColor::Orange,
                    "#e1bee7" => TextMarkerColor::Purple,
                    _ => TextMarkerColor::Custom(hex.to_string()),
                };
                let marked: Vec<usize> = tbl.marked_rows.iter().copied().collect();
                for r in marked {
                    tbl.row_markers.insert(r, color.clone());
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_clear_markers_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let marked: Vec<usize> = tbl.marked_rows.iter().copied().collect();
                for r in marked {
                    tbl.row_markers.remove(&r);
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- SEARCH & SORT & FILTER ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_search_text_changed(move |text| {
            let mut st = state.borrow_mut();
            st.search_query = text.to_string();
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_header_sort_clicked(move |col_idx| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let c = col_idx as usize;
                tbl.sort_column = match tbl.sort_column {
                    Some((cur_c, true)) if cur_c == c => Some((c, false)),
                    Some((cur_c, false)) if cur_c == c => None,
                    _ => Some((c, true)),
                };
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_filter_popup(move |col_idx| {
            let st = state.borrow();
            if let Some(tbl) = st.active_table() {
                let c = col_idx as usize;
                let col_name = tbl.headers.get(c).cloned().unwrap_or_default();

                // Rows matching all OTHER active filters, only_marked, and search_query
                let other_matched_rows =
                    tbl.rows_matching_other_filters(c, st.only_marked, &st.search_query);

                // Count occurrences of each value in column `c` across `other_matched_rows`
                let mut value_counts: HashMap<String, usize> = HashMap::new();
                for &r_idx in &other_matched_rows {
                    if let Some(row) = tbl.rows.get(r_idx) {
                        let raw = row.get(c).map(|s| s.as_str()).unwrap_or("");
                        let key = if raw.trim().is_empty() {
                            String::new()
                        } else {
                            raw.to_string()
                        };
                        *value_counts.entry(key).or_insert(0) += 1;
                    }
                }

                // Ensure empty "" entry is present if any row in the table has an empty cell in column `c`
                let table_has_empty = tbl.rows.iter().any(|r| {
                    r.get(c)
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                });
                if table_has_empty {
                    value_counts.entry(String::new()).or_insert(0);
                }

                // Also ensure any value currently selected in `current_filter` is shown (even if count is 0)
                let current_filter = tbl.active_filters.get(&c);
                if let Some(cf) = current_filter {
                    if cf.include_empty {
                        value_counts.entry(String::new()).or_insert(0);
                    }
                    for sv in &cf.selected_values {
                        value_counts.entry(sv.clone()).or_insert(0);
                    }
                }

                let mut distinct_values: Vec<String> = value_counts.keys().cloned().collect();
                distinct_values.sort_by(|a, b| {
                    if a.is_empty() && !b.is_empty() {
                        std::cmp::Ordering::Less
                    } else if !a.is_empty() && b.is_empty() {
                        std::cmp::Ordering::Greater
                    } else {
                        natord::compare(a, b)
                    }
                });

                let items: Vec<FilterChoiceItem> = distinct_values
                    .into_iter()
                    .map(|val| {
                        let count = value_counts.get(&val).copied().unwrap_or(0) as i32;
                        let selected = match current_filter {
                            None => true,
                            Some(f) if f.all_selected => true,
                            Some(f) => {
                                if val.is_empty() {
                                    f.include_empty || f.selected_values.contains("")
                                } else {
                                    f.selected_values.contains(&val)
                                        || f.selected_values.contains(val.trim())
                                }
                            }
                        };
                        let display = if val.is_empty() {
                            "(Leere)".to_string()
                        } else {
                            val.clone()
                        };
                        FilterChoiceItem {
                            value: slint::SharedString::from(val),
                            display_text: slint::SharedString::from(display),
                            count,
                            selected,
                            visible: true,
                        }
                    })
                    .collect();

                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_filter_dialog_col_idx(col_idx);
                    ui.set_filter_dialog_col_name(slint::SharedString::from(col_name));
                    ui.set_filter_dialog_total_hits(other_matched_rows.len() as i32);
                    ui.set_filter_dialog_items(ModelRc::new(VecModel::from(items)));
                    ui.set_filter_dialog_search(slint::SharedString::default());
                    ui.set_filter_dialog_visible(true);
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_filter_toggle_item(move |idx, checked| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_filter_dialog_items();
                if let Some(mut item) = model.row_data(idx as usize) {
                    item.selected = checked;
                    model.set_row_data(idx as usize, item);
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_filter_search_changed(move |query| {
            if let Some(ui) = ui_handle.upgrade() {
                let q = query.trim().to_lowercase();
                let model = ui.get_filter_dialog_items();
                let count = model.row_count();
                for i in 0..count {
                    if let Some(mut item) = model.row_data(i) {
                        item.visible = q.is_empty()
                            || item.display_text.to_lowercase().contains(&q);
                        model.set_row_data(i, item);
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_filter_select_all(move |select_all| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_filter_dialog_items();
                let count = model.row_count();
                for i in 0..count {
                    if let Some(mut item) = model.row_data(i) {
                        if item.visible {
                            item.selected = select_all;
                            model.set_row_data(i, item);
                        }
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_filter_reset_column(move |col_idx| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                tbl.active_filters.remove(&(col_idx as usize));
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_filter_apply_clicked(move |col_idx| {
            let mut st = state.borrow_mut();
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_filter_dialog_items();
                let mut selected_vals = HashSet::new();
                let mut include_empty = false;
                let mut all_checked = true;
                let count = model.row_count();
                for i in 0..count {
                    if let Some(item) = model.row_data(i) {
                        if item.selected {
                            if item.value.is_empty() {
                                include_empty = true;
                            } else {
                                selected_vals.insert(item.value.to_string());
                            }
                        } else {
                            all_checked = false;
                        }
                    }
                }

                if let Some(tbl) = st.active_table_mut() {
                    if all_checked && count > 0 {
                        // All values selected -> filter is not restricting anything
                        tbl.active_filters.remove(&(col_idx as usize));
                    } else {
                        let cond = FilterCondition {
                            include_non_empty: !selected_vals.is_empty(),
                            selected_values: selected_vals,
                            all_selected: false,
                            include_empty,
                            text_search: String::new(),
                        };
                        tbl.active_filters.insert(col_idx as usize, cond);
                    }
                }
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_filter_clear_all_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                tbl.active_filters.clear();
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- RULES & ANALYSIS ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_run_rules_clicked(move || {
            let mut st = state.borrow_mut();
            st.execute_all_rules();
            if let Some(tbl) = st.active_table() {
                log_info(
                    "Rules",
                    &format!(
                        "Regelprüfung für '{}' abgeschlossen: {} Hervorhebungen, {} Regeln.",
                        tbl.display_name,
                        tbl.rule_highlights.len(),
                        tbl.rule_summaries.len()
                    ),
                );
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- CELL EDITING ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_cell_edited(move |row, col, new_val| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                tbl.set_cell(row as usize, col as usize, new_val.to_string());
            }
            st.execute_all_rules();
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- WORKFLOWS: FILL, SPLIT, SMART ADD, DELETE ---
    {
        let ui_handle = main_window.as_weak();
        main_window.on_show_fill_dialog_clicked(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_fill_dialog_visible(true);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_apply_fill_column(move |col_idx, mode, static_val, prefix, suffix, start, step| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let fill_mode = match mode {
                    0 => FillMode::Static(static_val.to_string()),
                    1 => FillMode::PrefixSuffix {
                        prefix: prefix.to_string(),
                        suffix: suffix.to_string(),
                    },
                    2 => FillMode::Increment {
                        start: start as i64,
                        step: step as i64,
                        prefix: prefix.to_string(),
                        pad_zeros: 0,
                    },
                    _ => FillMode::Static(static_val.to_string()),
                };
                let marked: Vec<usize> = tbl.marked_rows.iter().copied().collect();
                fill_column(tbl, col_idx as usize, &marked, fill_mode);
                log_info("Transform", &format!("Spalte {} befüllt.", col_idx + 1));
            }
            st.execute_all_rules();
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_show_split_dialog_clicked(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_split_dialog_visible(true);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_apply_split_column(move |col_idx, delim_str, new_names_str| {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let delim = delim_str.chars().next().unwrap_or(',');
                let new_names: Vec<String> = new_names_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                split_column(tbl, col_idx as usize, delim, &new_names);
                log_info("Transform", &format!("Spalte {} nach '{}' aufgeteilt.", col_idx + 1, delim));
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_smart_add_row_clicked(move || {
            let st = state.borrow();
            let Some(tbl) = st.active_table() else {
                return;
            };
            let dialog_state =
                build_smart_add_dialog_state(tbl, st.only_marked, &st.search_query);

            let slint_fields: Vec<SmartAddFieldItem> = dialog_state
                .fields
                .into_iter()
                .map(|f| {
                    let suggs: Vec<SmartAddSuggestionItem> = f
                        .suggestions
                        .into_iter()
                        .map(|s| SmartAddSuggestionItem {
                            value: slint::SharedString::from(s.value),
                            display: slint::SharedString::from(s.display),
                            count: s.count as i32,
                            is_load_more: s.is_load_more,
                        })
                        .collect();
                    SmartAddFieldItem {
                        col_idx: f.col_idx as i32,
                        column_name: slint::SharedString::from(f.column_name),
                        concept_name: slint::SharedString::from(f.concept_name),
                        value: slint::SharedString::from(f.value),
                        suggestions: ModelRc::new(VecModel::from(suggs)),
                        dropdown_open: false,
                    }
                })
                .collect();

            if let Some(ui) = ui_handle.upgrade() {
                ui.set_smart_add_basis_text(slint::SharedString::from(
                    dialog_state.basis_description,
                ));
                ui.set_smart_add_row_count(1);
                ui.set_smart_add_row_count_text(slint::SharedString::from("1"));
                ui.set_smart_add_fields(ModelRc::new(VecModel::from(slint_fields)));
                ui.set_smart_add_dialog_visible(true);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_smart_add_field_edited(move |field_idx, new_val| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_smart_add_fields();
                let idx = field_idx as usize;
                if let Some(mut item) = model.row_data(idx) {
                    item.value = new_val;
                    model.set_row_data(idx, item);
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_smart_add_toggle_dropdown(move |field_idx| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_smart_add_fields();
                let target_idx = field_idx as usize;
                let count = model.row_count();
                for i in 0..count {
                    if let Some(mut item) = model.row_data(i) {
                        if i == target_idx {
                            item.dropdown_open = !item.dropdown_open;
                        } else {
                            item.dropdown_open = false;
                        }
                        model.set_row_data(i, item);
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_smart_add_suggestion_selected(move |field_idx, sugg_idx| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let model = ui.get_smart_add_fields();
            let f_idx = field_idx as usize;
            let s_idx = sugg_idx as usize;
            let Some(mut field_item) = model.row_data(f_idx) else {
                return;
            };
            let Some(sugg_item) = field_item.suggestions.row_data(s_idx) else {
                return;
            };

            if sugg_item.is_load_more {
                // Load all suggestions from all (unfiltered) rows of the table
                let st = state.borrow();
                if let Some(tbl) = st.active_table() {
                    let all_suggs =
                        load_all_suggestions_for_column(tbl, field_item.col_idx as usize);
                    let slint_suggs: Vec<SmartAddSuggestionItem> = all_suggs
                        .into_iter()
                        .map(|s| SmartAddSuggestionItem {
                            value: slint::SharedString::from(s.value),
                            display: slint::SharedString::from(s.display),
                            count: s.count as i32,
                            is_load_more: false,
                        })
                        .collect();
                    field_item.suggestions = ModelRc::new(VecModel::from(slint_suggs));
                    field_item.dropdown_open = true;
                    model.set_row_data(f_idx, field_item);
                }
            } else {
                field_item.value = sugg_item.value;
                field_item.dropdown_open = false;
                model.set_row_data(f_idx, field_item);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_smart_add_clear_all_fields(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_smart_add_fields();
                let count = model.row_count();
                for i in 0..count {
                    if let Some(mut item) = model.row_data(i) {
                        item.value = slint::SharedString::default();
                        item.dropdown_open = false;
                        model.set_row_data(i, item);
                    }
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_confirm_smart_add_rows(move |row_count| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let model = ui.get_smart_add_fields();
            let mut col_values: Vec<(usize, String)> = Vec::new();
            let count = model.row_count();
            for i in 0..count {
                if let Some(item) = model.row_data(i) {
                    let val = item.value.to_string();
                    if !val.is_empty() {
                        col_values.push((item.col_idx as usize, val));
                    }
                }
            }

            let mut st = state.borrow_mut();
            let only_marked = st.only_marked;
            let search_q = st.search_query.clone();
            let mut clear_search = false;

            if let Some(tbl) = st.active_table_mut() {
                let created_indices =
                    execute_smart_add_rows(tbl, row_count.max(1) as usize, &col_values);
                if only_marked {
                    for &idx in &created_indices {
                        tbl.marked_rows.insert(idx);
                    }
                }
                if !search_q.trim().is_empty() {
                    let q_low = search_q.trim().to_lowercase();
                    if let Some(&first_new) = created_indices.first() {
                        if let Some(new_row) = tbl.rows.get(first_new) {
                            if !new_row.iter().any(|c| c.to_lowercase().contains(&q_low)) {
                                clear_search = true;
                            }
                        }
                    }
                }
                log_info(
                    "SmartAdd",
                    &format!(
                        "{} neue Zeile(n) zu '{}' hinzugefügt.",
                        created_indices.len(),
                        tbl.display_name
                    ),
                );
            }

            if clear_search {
                st.search_query.clear();
                ui.set_search_query(slint::SharedString::default());
            }

            st.execute_all_rules();
            ui.set_status_message(slint::SharedString::from(format!(
                "{} neue Zeile(n) hinzugefügt.",
                row_count.max(1)
            )));
            sync_ui(&ui, &st);
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_delete_marked_rows_clicked(move || {
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let mut marked_sorted: Vec<usize> = tbl.marked_rows.iter().copied().collect();
                marked_sorted.sort_by(|a, b| b.cmp(a));
                for r in marked_sorted {
                    if r < tbl.rows.len() {
                        tbl.rows.remove(r);
                    }
                }
                tbl.marked_rows.clear();
                tbl.is_dirty = true;
                log_info("Transform", "Markierte Zeilen gelöscht.");
            }
            st.execute_all_rules();
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
        });
    }

    // --- CLIPBOARD (TSV) ---
    {
        let state = state.clone();
        main_window.on_copy_clipboard_clicked(move || {
            let st = state.borrow();
            if let Some(tbl) = st.active_table() {
                let rows_to_copy = tbl.visible_row_indices(st.only_marked);
                let text = export_to_clipboard_string(tbl, &rows_to_copy, '\t', true);
                if let Ok(mut clip) = Clipboard::new() {
                    let _ = clip.set_text(text);
                    log_info("Clipboard", "Tabellendaten in die Zwischenablage kopiert.");
                }
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_paste_clipboard_clicked(move || {
            if let Ok(mut clip) = Clipboard::new() {
                if let Ok(text) = clip.get_text() {
                    let mut st = state.borrow_mut();
                    if let Some(tbl) = st.active_table_mut() {
                        paste_clipboard_data(tbl, &text, tbl.rows.len(), 0);
                        log_info("Clipboard", "Daten aus der Zwischenablage eingefügt.");
                    }
                    st.execute_all_rules();
                    if let Some(ui) = ui_handle.upgrade() {
                        sync_ui(&ui, &st);
                    }
                }
            }
        });
    }

    // --- DIFF & SYNC ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_show_diff_dialog_clicked(move || {
            let st = state.borrow();
            if let Some(tbl) = st.active_table() {
                // Find another table to compare against, or compare with self
                let other_tbl = st.tables.values().find(|t| t.id != tbl.id);
                if let Some(target) = other_tbl {
                    let report = compare_tables(tbl, target, None);
                    let summary = format!(
                        "Diff zwischen '{}' und '{}': +{} Neu, -{} Gelöscht, ~{} Geändert",
                        tbl.display_name, target.display_name, report.added_count, report.deleted_count, report.modified_count
                    );
                    let mut details = String::new();
                    for row_diff in report.rows.iter().take(20) {
                        match row_diff {
                            configurable_csv_analyzer::core::transforms::TableDiffRow::Added { row_idx, row_data } => {
                                details.push_str(&format!("+ [Neu Zeile {}]: {}\n", row_idx + 1, row_data.join(", ")));
                            }
                            configurable_csv_analyzer::core::transforms::TableDiffRow::Deleted { row_idx, row_data } => {
                                details.push_str(&format!("- [Gelöscht Zeile {}]: {}\n", row_idx + 1, row_data.join(", ")));
                            }
                            configurable_csv_analyzer::core::transforms::TableDiffRow::Modified { row_idx, cell_diffs } => {
                                details.push_str(&format!("~ [Geändert Zeile {}]:\n", row_idx + 1));
                                for c in cell_diffs {
                                    details.push_str(&format!("    {}: '{}' -> '{}'\n", c.col_name, c.source_val, c.target_val));
                                }
                            }
                            configurable_csv_analyzer::core::transforms::TableDiffRow::Unchanged { .. } => {}
                        }
                    }

                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_diff_summary_text(slint::SharedString::from(summary));
                        ui.set_diff_details_text(slint::SharedString::from(details));
                        ui.set_diff_dialog_visible(true);
                    }
                } else if let Some(ui) = ui_handle.upgrade() {
                    ui.set_diff_summary_text(slint::SharedString::from("Keine zweite Tabelle zum Vergleichen geöffnet."));
                    ui.set_diff_details_text(slint::SharedString::from("Bitte öffnen Sie mindestens zwei Tabellen (z. B. CSV und ODS)."));
                    ui.set_diff_dialog_visible(true);
                }
            }
        });
    }

    // --- BOTTOM DOCK TAB & COLLAPSE ---
    {
        let ui_handle = main_window.as_weak();
        main_window.on_dock_tab_selected(move |tab_idx| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_dock_tab_idx(tab_idx);
            }
        });
    }
    {
        let ui_handle = main_window.as_weak();
        main_window.on_toggle_dock_collapsed(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let cur = ui.get_dock_collapsed();
                ui.set_dock_collapsed(!cur);
            }
        });
    }

    // --- LOGS & PROPOSALS ---
    {
        main_window.on_copy_logs_clicked(move || {
            let text = get_global_logger().format_text(None);
            if let Ok(mut clip) = Clipboard::new() {
                let _ = clip.set_text(text);
                log_info("Logs", "Logs in die Zwischenablage kopiert.");
            }
        });
    }

    sync_ui(&main_window, &state.borrow());
    main_window.run()
}
