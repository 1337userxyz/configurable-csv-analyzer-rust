use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use slint::{ComponentHandle, ModelRc, VecModel};
use kit_picker::PickRequest;

use configurable_csv_analyzer::core::io::csv::{find_all_csvs_in_dir, load_csv, save_csv};
use configurable_csv_analyzer::core::io::info::{
    build_info_from_table, default_info_path_for_table, save_info_file,
};
use configurable_csv_analyzer::core::io::ods::{
    create_ods_file, list_ods_sheets, load_ods_sheet, open_in_external_app, open_in_libreoffice,
};
use configurable_csv_analyzer::core::logging::{log_error, log_info};
use configurable_csv_analyzer::core::model::TableData;

use crate::{sync_ui, AppState, MainWindow};

pub fn register(main_window: &MainWindow, state: Rc<RefCell<AppState>>) {
    // --- CSV ÖFFNEN (Mehrfachauswahl) ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_csv_clicked(move || {
            let files = PickRequest::files()
                .title("CSV-Dateien öffnen (Mehrfachauswahl möglich)")
                .context("csv_open")
                .add_filter("CSV Dateien", &["csv", "tsv", "txt"])
                .add_filter("Alle Dateien", &["*"])
                .pick_files_sync();

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
            let file = PickRequest::file()
                .title("ODS-Datei öffnen")
                .context("ods_open")
                .add_filter("LibreOffice Calc (.ods)", &["ods"])
                .add_filter("Alle Dateien", &["*"])
                .pick_sync();

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
            let folder = PickRequest::folder()
                .title("Ordner mit CSV-Dateien auswählen")
                .context("folder_import")
                .pick_sync();

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
                    let chosen = PickRequest::save()
                        .title("Ziel-ODS-Datei für CSV-Export auswählen")
                        .context("export_ods")
                        .default_extension("ods")
                        .add_filter("LibreOffice Calc (.ods)", &["ods"])
                        .pick_sync();
                    match chosen {
                        Some(p) => p,
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

            let folder = PickRequest::folder()
                .title("Zielordner für CSV-Export auswählen")
                .context("export_csv_folder")
                .pick_sync();

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
                    "✓ {} Tabellenblatt/-blätter als CSV in '{:?}' exportiert.",
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
}
