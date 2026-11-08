use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use configurable_csv_analyzer::core::io::csv::load_csv;
use configurable_csv_analyzer::core::io::info::{
    apply_info_to_table, default_info_path_for_table, default_views_path, load_info_file,
    save_views_file,
};
use configurable_csv_analyzer::core::io::ods::{list_ods_sheets, load_ods_sheet};
use configurable_csv_analyzer::core::logging::{log_error, log_info};
use configurable_csv_analyzer::core::model::{
    AppConfig, TableData, ViewDefinition, ViewFileItem, ViewsFile,
};
use configurable_csv_analyzer::core::rules::RuleEngine;

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
        let Some(ref tbl_path) = tbl.file_path else {
            return false;
        };

        if tbl.is_ods {
            if let Some(ref sheet_path) = view.spreadsheet_path {
                let p1 = PathBuf::from(sheet_path);
                if p1 == *tbl_path || p1.file_name() == tbl_path.file_name() {
                    return true;
                }
            }
        }

        let tbl_path_str = tbl_path.to_string_lossy().to_string();
        let tbl_name = tbl_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

        for vf in &view.files {
            if vf.relative_path == tbl_path_str || vf.full_path.as_deref() == Some(&tbl_path_str) {
                return true;
            }
            let vf_p = PathBuf::from(&vf.relative_path);
            if let Some(fname) = vf_p.file_name() {
                if fname.to_string_lossy() == tbl_name {
                    return true;
                }
            }
            if let Some(ref fp) = vf.full_path {
                let fp_p = PathBuf::from(fp);
                if let Some(fname) = fp_p.file_name() {
                    if fname.to_string_lossy() == tbl_name {
                        return true;
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
