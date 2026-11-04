#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use arboard::Clipboard;
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};

use configurable_csv_analyzer::core::io::csv::{find_all_csvs_in_dir, load_csv, save_csv};
use configurable_csv_analyzer::core::io::info::{
    apply_info_to_table, default_config_path, default_info_path_for_table, default_views_path,
    load_app_config, load_info_file, load_views_file, save_info_file, build_info_from_table,
};
use configurable_csv_analyzer::core::io::ods::{list_ods_sheets, load_ods_sheet, open_in_libreoffice};
use configurable_csv_analyzer::core::color_cache::cached_color;
use configurable_csv_analyzer::core::logging::{get_global_logger, log_error, log_info};
use configurable_csv_analyzer::core::model::{
    AppConfig, FilterCondition, TableData, TextMarkerColor, ViewsFile,
};
use configurable_csv_analyzer::core::rules::RuleEngine;
use configurable_csv_analyzer::core::transforms::{
    compare_tables, export_to_clipboard_string, fill_column, paste_clipboard_data,
    smart_add_row, split_column, FillMode,
};

mod dialogs;

slint::include_modules!();

pub struct AppState {
    pub tables: HashMap<String, TableData>,
    pub tab_order: Vec<String>,
    pub active_tab_id: Option<String>,
    pub search_query: String,
    pub only_marked: bool,
    pub pending_ods_path: Option<PathBuf>,
    pub app_config: AppConfig,
    pub views_config: ViewsFile,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            tab_order: Vec::new(),
            active_tab_id: None,
            search_query: String::new(),
            only_marked: false,
            pending_ods_path: None,
            app_config: AppConfig::default(),
            views_config: ViewsFile::default(),
        }
    }

    pub fn add_table(&mut self, mut table: TableData) -> String {
        // Try auto-loading .info file if exists
        if let Some(ref p) = table.file_path {
            let info_p = default_info_path_for_table(p);
            if info_p.exists() {
                if let Ok(info) = load_info_file(&info_p) {
                    apply_info_to_table(&mut table, &info);
                    log_info("InfoFile", &format!("Konfiguration automatisch geladen: {:?}", info_p));
                }
            }
        }

        let id = table.id.clone();
        if !self.tab_order.contains(&id) {
            self.tab_order.push(id.clone());
        }
        self.tables.insert(id.clone(), table);
        self.active_tab_id = Some(id.clone());
        id
    }

    pub fn active_table(&self) -> Option<&TableData> {
        self.active_tab_id.as_ref().and_then(|id| self.tables.get(id))
    }

    pub fn active_table_mut(&mut self) -> Option<&mut TableData> {
        self.active_tab_id.as_ref().and_then(|id| self.tables.get_mut(id))
    }
}

pub fn sync_ui(ui: &MainWindow, state: &AppState) {
    // 1. Sync tabs
    let mut tab_items = Vec::new();
    for id in &state.tab_order {
        if let Some(tbl) = state.tables.get(id) {
            let error_count = tbl
                .rule_highlights
                .iter()
                .filter(|h| h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error)
                .count();

            tab_items.push(TabItem {
                id: slint::SharedString::from(id),
                title: slint::SharedString::from(&tbl.display_name),
                is_dirty: tbl.is_dirty,
                error_count: error_count as i32,
                is_active: state.active_tab_id.as_ref() == Some(id),
            });
        }
    }
    ui.set_tabs(ModelRc::new(VecModel::from(tab_items)));
    ui.set_active_tab_id(slint::SharedString::from(state.active_tab_id.as_deref().unwrap_or("")));

    // 2. Active table sync
    if let Some(tbl) = state.active_table() {
        // Headers
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
            let width = tbl.column_settings.get(idx).map(|s| s.width).unwrap_or(150) as f32;

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

        // Map visible rows to Slint RowItems
        let mut row_items = Vec::new();
        for (view_row_idx, &orig_row_idx) in visible_indices.iter().enumerate() {
            if let Some(row) = tbl.rows.get(orig_row_idx) {
                let is_marked = tbl.marked_rows.contains(&orig_row_idx);
                let row_marker = tbl.row_markers.get(&orig_row_idx);

                let mut cell_items = Vec::new();
                for (col_idx, cell_text) in row.iter().enumerate() {
                    let has_error = tbl.rule_highlights.iter().any(|h| {
                        h.row == orig_row_idx
                            && (h.col.is_none() || h.col == Some(col_idx))
                            && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error
                    });
                    let has_warning = tbl.rule_highlights.iter().any(|h| {
                        h.row == orig_row_idx
                            && (h.col.is_none() || h.col == Some(col_idx))
                            && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Warning
                    });
                    let has_info = tbl.rule_highlights.iter().any(|h| {
                        h.row == orig_row_idx
                            && (h.col.is_none() || h.col == Some(col_idx))
                            && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Info
                    });

                    let cell_marker = tbl.cell_markers.get(&(orig_row_idx, col_idx));
                    let bg_color = if let Some(m) = cell_marker {
                        parse_hex_color(m.hex())
                    } else if let Some(m) = row_marker {
                        parse_hex_color(m.hex())
                    } else {
                        Color::from_argb_u8(0, 0, 0, 0)
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
                    Color::from_argb_u8(0, 0, 0, 0)
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
    } else {
        ui.set_headers(ModelRc::new(VecModel::default()));
        ui.set_rows(ModelRc::new(VecModel::default()));
        ui.set_total_row_count(0);
        ui.set_marked_row_count(0);
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
    let views_config = load_views_file(&default_views_path()).unwrap_or_default();

    let mut initial_state = AppState::new();
    initial_state.app_config = app_config;
    initial_state.views_config = views_config;

    // Check if there is a default view and auto-load its files on startup
    if let Some(default_view) = initial_state.views_config.views.iter().find(|v| v.is_default_view).cloned() {
        for file_item in &default_view.files {
            let p = PathBuf::from(&file_item.relative_path);
            if p.exists() {
                if let Ok(table) = load_csv(&p, None, None, true) {
                    initial_state.add_table(table);
                }
            }
        }
    }

    let main_window = MainWindow::new()?;
    let config_window = Rc::new(ConfigurationWindow::new()?);
    let info_editor_window = Rc::new(InfoFileEditorWindow::new()?);
    let state = Rc::new(RefCell::new(initial_state));

    log_info("App", "Configurable CSV & ODS Analyzer gestartet.");

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
            dialogs::configuration::sync_configuration_ui(&config_win, &state.borrow());
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

    // --- CSV ÖFFNEN ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_csv_clicked(move || {
            let file = rfd::FileDialog::new()
                .add_filter("CSV Dateien", &["csv", "tsv", "txt"])
                .add_filter("Alle Dateien", &["*"])
                .pick_file();

            if let Some(p) = file {
                match load_csv(&p, None, None, true) {
                    Ok(mut table) => {
                        log_info("CSV", &format!("CSV erfolgreich geladen: {:?}", p));
                        let mut st = state.borrow_mut();
                        let all_tbls = st.tables.clone();
                        RuleEngine::execute(&mut table, &all_tbls);
                        st.add_table(table);
                        if let Some(ui) = ui_handle.upgrade() {
                            ui.set_status_message(slint::SharedString::from(format!("Geladen: {:?}", p.file_name().unwrap_or_default())));
                            sync_ui(&ui, &st);
                        }
                    }
                    Err(e) => {
                        log_error("CSV", &format!("Fehler beim Laden von {:?}: {}", p, e));
                        if let Some(ui) = ui_handle.upgrade() {
                            ui.set_status_message(slint::SharedString::from(format!("Fehler: {}", e)));
                        }
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
                .add_filter("LibreOffice Calc (.ods)", &["ods"])
                .pick_file();

            if let Some(p) = file {
                match list_ods_sheets(&p) {
                    Ok(sheets) => {
                        if sheets.len() > 1 {
                            let mut st = state.borrow_mut();
                            st.pending_ods_path = Some(p.clone());
                            if let Some(ui) = ui_handle.upgrade() {
                                let sheet_model: Vec<slint::SharedString> = sheets.iter().map(|s| slint::SharedString::from(s)).collect();
                                ui.set_available_sheets(ModelRc::new(VecModel::from(sheet_model)));
                                ui.set_sheet_dialog_visible(true);
                            }
                        } else {
                            match load_ods_sheet(&p, None) {
                                Ok(mut table) => {
                                    log_info("ODS", &format!("ODS erfolgreich geladen: {:?}", p));
                                    let mut st = state.borrow_mut();
                                    let all_tbls = st.tables.clone();
                                    RuleEngine::execute(&mut table, &all_tbls);
                                    st.add_table(table);
                                    if let Some(ui) = ui_handle.upgrade() {
                                        ui.set_status_message(slint::SharedString::from(format!("ODS Geladen: {:?}", p.file_name().unwrap_or_default())));
                                        sync_ui(&ui, &st);
                                    }
                                }
                                Err(e) => {
                                    log_error("ODS", &format!("Fehler beim Laden von {:?}: {}", p, e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log_error("ODS", &format!("Fehler beim Lesen der Blätter von {:?}: {}", p, e));
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
                match load_ods_sheet(&p, Some(sheet_name.as_str())) {
                    Ok(mut table) => {
                        log_info("ODS", &format!("ODS Blatt '{}' geladen aus {:?}", sheet_name, p));
                        let all_tbls = st.tables.clone();
                        RuleEngine::execute(&mut table, &all_tbls);
                        st.add_table(table);
                        if let Some(ui) = ui_handle.upgrade() {
                            sync_ui(&ui, &st);
                        }
                    }
                    Err(e) => {
                        log_error("ODS", &format!("Fehler beim Laden des Blattes: {}", e));
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
            let folder = rfd::FileDialog::new().pick_folder();
            if let Some(f) = folder {
                let csvs = find_all_csvs_in_dir(&f);
                log_info("Import", &format!("{} CSV-Dateien in {:?} gefunden.", csvs.len(), f));
                let mut st = state.borrow_mut();
                for p in csvs {
                    if let Ok(mut tbl) = load_csv(&p, None, None, true) {
                        let all_tbls = st.tables.clone();
                        RuleEngine::execute(&mut tbl, &all_tbls);
                        st.add_table(tbl);
                    }
                }
                if let Some(ui) = ui_handle.upgrade() {
                    sync_ui(&ui, &st);
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

                    if let Ok(mut new_tbl) = reloaded {
                        log_info("Revert", &format!("Tabelle zurückgesetzt auf Stand der Datei {:?}", p));
                        let all_tbls = st.tables.clone();
                        RuleEngine::execute(&mut new_tbl, &all_tbls);
                        let id = new_tbl.id.clone();
                        st.tables.insert(id, new_tbl);
                        if let Some(ui) = ui_handle.upgrade() {
                            sync_ui(&ui, &st);
                        }
                    }
                }
            }
        });
    }

    // --- LIBREOFFICE ÖFFNEN ---
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

    // --- TAB SELECTION & CLOSE ---
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
        main_window.on_tab_close_clicked(move |tab_id| {
            let mut st = state.borrow_mut();
            st.tab_order.retain(|id| id != tab_id.as_str());
            st.tables.remove(tab_id.as_str());
            if st.active_tab_id.as_deref() == Some(tab_id.as_str()) {
                st.active_tab_id = st.tab_order.last().cloned();
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

                let mut distinct_values: Vec<String> = tbl
                    .rows
                    .iter()
                    .map(|r| r.get(c).cloned().unwrap_or_default())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                distinct_values.sort();

                let current_filter = tbl.active_filters.get(&c);
                let items: Vec<FilterChoiceItem> = distinct_values
                    .into_iter()
                    .map(|val| {
                        let selected = current_filter
                            .map(|f| f.all_selected || f.selected_values.contains(&val))
                            .unwrap_or(true);
                        FilterChoiceItem {
                            value: slint::SharedString::from(val),
                            selected,
                        }
                    })
                    .collect();

                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_filter_dialog_col_idx(col_idx);
                    ui.set_filter_dialog_col_name(slint::SharedString::from(col_name));
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
        let state = state.clone();
        main_window.on_filter_apply_clicked(move |col_idx| {
            let mut st = state.borrow_mut();
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_filter_dialog_items();
                let mut selected_vals = HashSet::new();
                let count = model.row_count();
                for i in 0..count {
                    if let Some(item) = model.row_data(i) {
                        if item.selected {
                            selected_vals.insert(item.value.to_string());
                        }
                    }
                }

                if let Some(tbl) = st.active_table_mut() {
                    let cond = FilterCondition {
                        selected_values: selected_vals,
                        all_selected: false,
                        include_empty: true,
                        include_non_empty: true,
                        text_search: String::new(),
                    };
                    tbl.active_filters.insert(col_idx as usize, cond);
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
            let all_tbls = st.tables.clone();

            if let Some(tbl) = st.active_table_mut() {
                let res = RuleEngine::execute(tbl, &all_tbls);
                log_info("Rules", &format!("Regelprüfung abgeschlossen: {} Fehler/Warnungen.", res.highlights.len()));

                if let Some(ui) = ui_handle.upgrade() {
                    let summaries: Vec<RuleSummaryItem> = res
                        .summaries
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

                    let proposals: Vec<ProposalItem> = res
                        .proposals
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
                    sync_ui(&ui, &st);
                }
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
            let mut st = state.borrow_mut();
            if let Some(tbl) = st.active_table_mut() {
                let selected_row = tbl.marked_rows.iter().copied().next();
                let new_idx = smart_add_row(tbl, selected_row);
                log_info("Transform", &format!("Smart Add Zeile hinzugefügt (Index {}).", new_idx + 1));
            }
            if let Some(ui) = ui_handle.upgrade() {
                sync_ui(&ui, &st);
            }
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
