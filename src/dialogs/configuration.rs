use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};

use configurable_csv_analyzer::core::color_cache::{cached_color, get_global_color_cache};
use configurable_csv_analyzer::core::io::csv::{find_all_csvs_in_dir, load_csv};
use configurable_csv_analyzer::core::io::info::{
    check_file_integrity, default_config_path, default_info_path_for_table, default_views_path,
    diff_info_files, move_info_file, save_app_config, save_views_file,
};
use configurable_csv_analyzer::core::io::ods::{create_ods_file, ensure_valid_ods_file};
use configurable_csv_analyzer::core::logging::{log_error, log_info};
use configurable_csv_analyzer::core::model::{
    BrushCategory, BrushDefinition, ExternalAppConfig, InfoFileStorageMode, TableData,
    ViewDefinition, ViewFileItem,
};

use crate::{
    AppState, ConfigurationWindow, InfoFileEditorWindow, MainWindow,
    SlintBrushItem, SlintExternalAppItem, SlintViewFileItem, SlintViewItem,
};
use crate::dialogs::info_editor::open_info_editor_for_table_or_path;

/// Stellt sicher, dass mindestens eine Kollektion (View) existiert und gibt den gültigen Index zurück.
pub fn ensure_selected_view(window: &ConfigurationWindow, state: &mut AppState) -> usize {
    if state.views_config.views.is_empty() {
        let storage_mode_str = state.app_config.selected_storage_mode.as_str().to_string();
        let mut initial_files = Vec::new();
        for tab_id in &state.tab_order {
            if let Some(tbl) = state.tables.get(tab_id) {
                if let Some(ref p) = tbl.file_path {
                    let path_str = p.to_string_lossy().to_string();
                    if !initial_files.iter().any(|f: &ViewFileItem| f.relative_path == path_str) {
                        initial_files.push(ViewFileItem {
                            relative_path: path_str.clone(),
                            full_path: Some(path_str),
                            info_file_path: None,
                            storage_mode: Some(storage_mode_str.clone()),
                        });
                    }
                }
            }
        }

        let name = state
            .active_view_name
            .clone()
            .unwrap_or_else(|| "Standard-Kollektion".to_string());
        state.views_config.views.push(ViewDefinition {
            name: name.clone(),
            is_default_view: true,
            files: initial_files,
            spreadsheet_path: None,
            color_pattern: Some("None".into()),
            export_color_pattern: Some("None".into()),
            export_paths: Vec::new(),
        });
        state.active_view_name = Some(name);
        window.set_selected_view_idx(0);
        return 0;
    }

    let raw_idx = window.get_selected_view_idx();
    let clamped = if raw_idx < 0 {
        0
    } else {
        (raw_idx as usize).min(state.views_config.views.len() - 1)
    };
    window.set_selected_view_idx(clamped as i32);
    clamped
}

pub fn sync_configuration_ui(window: &ConfigurationWindow, state: &AppState) {
    // 1. Views
    let view_items: Vec<SlintViewItem> = state
        .views_config
        .views
        .iter()
        .enumerate()
        .map(|(idx, v)| SlintViewItem {
            id: idx as i32,
            name: slint::SharedString::from(&v.name),
            is_default: v.is_default_view,
            file_count: v.files.len() as i32,
            spreadsheet_path: slint::SharedString::from(
                v.spreadsheet_path.as_deref().unwrap_or(""),
            ),
            color_pattern: slint::SharedString::from(v.color_pattern.as_deref().unwrap_or("None")),
            export_color_pattern: slint::SharedString::from(
                v.export_color_pattern.as_deref().unwrap_or("None"),
            ),
        })
        .collect();

    window.set_views(ModelRc::new(VecModel::from(view_items)));

    let raw_idx = window.get_selected_view_idx();
    let cur_idx = if raw_idx < 0 || state.views_config.views.is_empty() {
        0
    } else {
        (raw_idx as usize).min(state.views_config.views.len() - 1)
    };
    if !state.views_config.views.is_empty() && raw_idx != cur_idx as i32 {
        window.set_selected_view_idx(cur_idx as i32);
    }

    if let Some(v) = state.views_config.views.get(cur_idx) {
        window.set_current_spreadsheet_path(slint::SharedString::from(
            v.spreadsheet_path.as_deref().unwrap_or(""),
        ));
        let pat = v.color_pattern.as_deref().unwrap_or("None");
        let exp_pat = v.export_color_pattern.as_deref().unwrap_or("None");
        window.set_current_color_pattern(slint::SharedString::from(
            if pat.trim().is_empty() { "None" } else { pat },
        ));
        window.set_current_export_color_pattern(slint::SharedString::from(
            if exp_pat.trim().is_empty() { "None" } else { exp_pat },
        ));

        let file_items: Vec<SlintViewFileItem> = v
            .files
            .iter()
            .enumerate()
            .map(|(f_idx, f)| {
                let status =
                    check_file_integrity(&f.relative_path, f.info_file_path.as_deref(), None);
                let sm = f.storage_mode.as_deref().unwrap_or("Standard");
                SlintViewFileItem {
                    id: f_idx as i32,
                    relative_path: slint::SharedString::from(&f.relative_path),
                    info_file_path: slint::SharedString::from(
                        f.info_file_path.as_deref().unwrap_or(""),
                    ),
                    storage_mode: slint::SharedString::from(sm),
                    integrity_status: slint::SharedString::from(status),
                }
            })
            .collect();
        window.set_current_view_files(ModelRc::new(VecModel::from(file_items)));
    } else {
        window.set_current_spreadsheet_path(slint::SharedString::from(""));
        window.set_current_view_files(ModelRc::new(VecModel::from(Vec::<SlintViewFileItem>::new())));
    }

    // 2. Storage
    let mode_idx = match state.app_config.selected_storage_mode {
        InfoFileStorageMode::Dateipfad => 0,
        InfoFileStorageMode::Stammverzeichnis => 1,
        InfoFileStorageMode::Temporaer => 2,
        InfoFileStorageMode::BenutzerEntscheidet => 3,
    };
    window.set_selected_storage_mode(mode_idx);
    window.set_central_directory_path(slint::SharedString::from(
        &state.app_config.central_config_directory,
    ));

    // 3. Brushes
    refresh_brushes_list(window, state);
    window.set_marker_priority(state.app_config.marker_highlight_priority as i32);

    // 4. Associations
    window.set_check_associations_on_startup(state.app_config.check_file_associations_on_startup);

    // 5. External Apps
    let app_items: Vec<SlintExternalAppItem> = state
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
    window.set_external_apps(ModelRc::new(VecModel::from(app_items)));

    let raw_app_idx = window.get_selected_app_idx();
    if raw_app_idx >= 0 && (raw_app_idx as usize) < state.app_config.external_apps.len() {
        let app = &state.app_config.external_apps[raw_app_idx as usize];
        window.set_edit_app_name(slint::SharedString::from(&app.name));
        window.set_edit_app_path(slint::SharedString::from(&app.command_or_path));
        window.set_edit_app_arguments(slint::SharedString::from(&app.arguments));
    } else if raw_app_idx < 0 && !state.app_config.external_apps.is_empty() {
        window.set_selected_app_idx(0);
        let app = &state.app_config.external_apps[0];
        window.set_edit_app_name(slint::SharedString::from(&app.name));
        window.set_edit_app_path(slint::SharedString::from(&app.command_or_path));
        window.set_edit_app_arguments(slint::SharedString::from(&app.arguments));
    }
}

fn refresh_brushes_list(window: &ConfigurationWindow, state: &AppState) {
    let query = window.get_brush_search_query().to_lowercase();
    let brush_items: Vec<SlintBrushItem> = state
        .app_config
        .brushes
        .iter()
        .filter(|b| {
            if query.is_empty() {
                true
            } else {
                b.key.to_lowercase().contains(&query)
                    || b.hex_value.to_lowercase().contains(&query)
                    || b.category.as_str().to_lowercase().contains(&query)
            }
        })
        .map(|b| SlintBrushItem {
            key: slint::SharedString::from(&b.key),
            hex_value: slint::SharedString::from(&b.hex_value),
            category: slint::SharedString::from(b.category.as_str()),
            is_system: b.is_system,
            color: cached_color(&b.hex_value, 255),
        })
        .collect();

    window.set_brushes(ModelRc::new(VecModel::from(brush_items)));
}

pub fn setup_configuration_window(
    config_window: Rc<ConfigurationWindow>,
    info_editor_window: Rc<InfoFileEditorWindow>,
    main_window_weak: Weak<MainWindow>,
    state: Rc<RefCell<AppState>>,
) {
    // --- VIEWS OPERATIONS ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_select_view(move |idx| {
            win.set_selected_view_idx(idx);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_view_clicked(move |name| {
            let mut st = state.borrow_mut();
            let trimmed = name.trim();
            let view_name = if trimmed.is_empty() {
                format!("Kollektion {}", st.views_config.views.len() + 1)
            } else {
                trimmed.to_string()
            };
            let is_default = st.views_config.views.is_empty();
            st.views_config.views.push(ViewDefinition {
                name: view_name.clone(),
                is_default_view: is_default,
                files: Vec::new(),
                spreadsheet_path: None,
                color_pattern: Some("None".into()),
                export_color_pattern: Some("None".into()),
                export_paths: Vec::new(),
            });
            let new_idx = st.views_config.views.len() - 1;
            win.set_selected_view_idx(new_idx as i32);
            let _ = save_views_file(&default_views_path(), &st.views_config);
            log_info("Config", &format!("Neue Kollektion '{}' angelegt.", view_name));
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_delete_view_clicked(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i < st.views_config.views.len() {
                let removed = st.views_config.views.remove(i);
                log_info("Config", &format!("Kollektion '{}' gelöscht.", removed.name));
                if win.get_selected_view_idx() >= st.views_config.views.len() as i32 {
                    win.set_selected_view_idx(
                        (st.views_config.views.len().saturating_sub(1)) as i32,
                    );
                }
                let _ = save_views_file(&default_views_path(), &st.views_config);
            }
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_set_default_view_clicked(move |idx| {
            let mut st = state.borrow_mut();
            let target_idx = idx as usize;
            for (i, v) in st.views_config.views.iter_mut().enumerate() {
                v.is_default_view = i == target_idx;
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_view_up(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i > 0 && i < st.views_config.views.len() {
                st.views_config.views.swap(i, i - 1);
                win.set_selected_view_idx((i - 1) as i32);
                let _ = save_views_file(&default_views_path(), &st.views_config);
            }
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_view_down(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i + 1 < st.views_config.views.len() {
                st.views_config.views.swap(i, i + 1);
                win.set_selected_view_idx((i + 1) as i32);
                let _ = save_views_file(&default_views_path(), &st.views_config);
            }
            sync_configuration_ui(&win, &st);
        });
    }

    // ODS Picker (allows selecting an existing .ods OR typing a new .ods filename to create it!)
    // --- ODS AUSWÄHLEN (Bestehend) ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_browse_spreadsheet_clicked(move || {
            let file = rfd::FileDialog::new()
                .set_title("Bestehende ODS-Datei für diese Kollektion auswählen")
                .add_filter("LibreOffice Calc (.ods)", &["ods"])
                .add_filter("Alle Dateien", &["*"])
                .pick_file();

            if let Some(path) = file {
                let _ = ensure_valid_ods_file(&path, &[]);
                let path_str = path.to_string_lossy().to_string();
                let mut st = state.borrow_mut();
                let cur_idx = ensure_selected_view(&win, &mut st);
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    v.spreadsheet_path = Some(path_str.clone());
                    log_info(
                        "ODS",
                        &format!("ODS-Datei '{}' mit Kollektion '{}' verknüpft.", path.display(), v.name),
                    );
                }
                let _ = save_views_file(&default_views_path(), &st.views_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    // --- ODS NEU ERSTELLEN ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_create_spreadsheet_clicked(move || {
            let default_name = {
                let mut st = state.borrow_mut();
                let cur_idx = ensure_selected_view(&win, &mut st);
                st.views_config
                    .views
                    .get(cur_idx)
                    .map(|v| format!("{}.ods", v.name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")))
                    .unwrap_or_else(|| "Kollektion.ods".to_string())
            };

            let file = rfd::FileDialog::new()
                .set_title("Neue ODS-Datei für Kollektion erstellen (Speicherort wählen)")
                .add_filter("LibreOffice Calc (.ods)", &["ods"])
                .set_file_name(&default_name)
                .save_file();

            if let Some(mut path) = file {
                if path.extension().is_none() {
                    path.set_extension("ods");
                }
                let mut st = state.borrow_mut();
                let cur_idx = ensure_selected_view(&win, &mut st);

                let mut tables_for_ods: Vec<TableData> = Vec::new();
                if let Some(v) = st.views_config.views.get(cur_idx) {
                    for f in &v.files {
                        let fp = PathBuf::from(&f.relative_path);
                        if let Some(open_tbl) = st.tables.values().find(|t| t.file_path.as_ref() == Some(&fp)) {
                            tables_for_ods.push(open_tbl.clone());
                        } else if fp.exists() {
                            if let Ok(loaded) = load_csv(&fp, None, None, true) {
                                tables_for_ods.push(loaded);
                            }
                        }
                    }
                }
                if tables_for_ods.is_empty() {
                    for id in &st.tab_order {
                        if let Some(t) = st.tables.get(id) {
                            tables_for_ods.push(t.clone());
                        }
                    }
                }

                match create_ods_file(&path, &tables_for_ods) {
                    Ok(_) => {
                        log_info(
                            "ODS",
                            &format!(
                                "Neue ODS-Datei erstellt unter {:?} ({} Tabellenblätter)",
                                path,
                                tables_for_ods.len().max(1)
                            ),
                        );
                    }
                    Err(e) => {
                        log_error(
                            "ODS",
                            &format!("Fehler beim Erstellen der ODS-Datei {:?}: {}", path, e),
                        );
                    }
                }

                let path_str = path.to_string_lossy().to_string();
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    v.spreadsheet_path = Some(path_str);
                }
                let _ = save_views_file(&default_views_path(), &st.views_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_spreadsheet_path_edited(move |new_path| {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let trimmed = new_path.trim();
                v.spreadsheet_path = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_clear_spreadsheet_clicked(move || {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                v.spreadsheet_path = None;
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_view_patterns_changed(move |color_p, export_p| {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                v.color_pattern = Some(color_p.to_string());
                v.export_color_pattern = Some(export_p.to_string());
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
        });
    }

    // --- VIEW FILES OPERATIONS ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_csv_to_view_clicked(move || {
            let files = rfd::FileDialog::new()
                .set_title("CSV- oder ODS-Dateien zur Kollektion hinzufügen (Mehrfachauswahl möglich)")
                .add_filter("Tabellen-Dateien (.csv, .tsv, .txt, .ods)", &["csv", "tsv", "txt", "ods"])
                .add_filter("Alle Dateien", &["*"])
                .pick_files();

            if let Some(files) = files {
                let mut st = state.borrow_mut();
                let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
                let cur_idx = ensure_selected_view(&win, &mut st);
                let mut added_count = 0;
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    for file in files {
                        let rel = file.to_string_lossy().to_string();
                        if !v.files.iter().any(|f| f.relative_path.eq_ignore_ascii_case(&rel)) {
                            v.files.push(ViewFileItem {
                                relative_path: rel.clone(),
                                full_path: Some(rel),
                                info_file_path: None,
                                storage_mode: Some(storage_mode_str.clone()),
                            });
                            added_count += 1;
                        }
                    }
                    log_info(
                        "Config",
                        &format!(
                            "{} Datei(en) zur Kollektion '{}' hinzugefügt (Gesamt: {}).",
                            added_count,
                            v.name,
                            v.files.len()
                        ),
                    );
                }
                let _ = save_views_file(&default_views_path(), &st.views_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_folder_to_view_clicked(move || {
            let folder = rfd::FileDialog::new()
                .set_title("Ordner mit CSV-Dateien zur Kollektion hinzufügen")
                .pick_folder();

            if let Some(folder) = folder {
                let csvs = find_all_csvs_in_dir(&folder);
                let mut st = state.borrow_mut();
                let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
                let cur_idx = ensure_selected_view(&win, &mut st);
                let mut added_count = 0;
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    for path in csvs {
                        let rel = path.to_string_lossy().to_string();
                        if !v.files.iter().any(|f| f.relative_path.eq_ignore_ascii_case(&rel)) {
                            v.files.push(ViewFileItem {
                                relative_path: rel.clone(),
                                full_path: Some(rel),
                                info_file_path: None,
                                storage_mode: Some(storage_mode_str.clone()),
                            });
                            added_count += 1;
                        }
                    }
                    log_info(
                        "Config",
                        &format!(
                            "{} CSV-Datei(en) aus Ordner {:?} zur Kollektion '{}' hinzugefügt.",
                            added_count, folder, v.name
                        ),
                    );
                }
                let _ = save_views_file(&default_views_path(), &st.views_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_import_open_tabs_to_view_clicked(move || {
            let mut st = state.borrow_mut();
            let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
            let cur_idx = ensure_selected_view(&win, &mut st);
            let open_paths: Vec<String> = st
                .tab_order
                .iter()
                .filter_map(|id| st.tables.get(id))
                .filter_map(|t| t.file_path.as_ref().map(|p| p.to_string_lossy().to_string()))
                .collect();

            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                for rel in open_paths {
                    if !v.files.iter().any(|f| f.relative_path.eq_ignore_ascii_case(&rel)) {
                        v.files.push(ViewFileItem {
                            relative_path: rel.clone(),
                            full_path: Some(rel),
                            info_file_path: None,
                            storage_mode: Some(storage_mode_str.clone()),
                        });
                    }
                }
                log_info(
                    "Config",
                    &format!("Geöffnete Tabs in Kollektion '{}' übernommen.", v.name),
                );
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_remove_file_from_view_clicked(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx < v.files.len() {
                    v.files.remove(idx);
                }
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_file_up(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx > 0 && idx < v.files.len() {
                    v.files.swap(idx, idx - 1);
                    win.set_selected_file_idx((idx - 1) as i32);
                }
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_file_down(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx + 1 < v.files.len() {
                    v.files.swap(idx, idx + 1);
                    win.set_selected_file_idx((idx + 1) as i32);
                }
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    // Migrate info file between central and decentralized
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_migrate_info_file_clicked(move |f_idx| {
            let mut st = state.borrow_mut();
            let central = PathBuf::from(&st.app_config.central_config_directory);
            let cur_idx = ensure_selected_view(&win, &mut st);
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if let Some(f) = v.files.get_mut(idx) {
                    let csv_p = PathBuf::from(&f.relative_path);
                    let info_p = f
                        .info_file_path
                        .as_ref()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| default_info_path_for_table(&csv_p));

                    // Toggle mode
                    let target_mode = if f.storage_mode.as_deref() == Some("Stammverzeichnis") {
                        InfoFileStorageMode::Dateipfad
                    } else {
                        InfoFileStorageMode::Stammverzeichnis
                    };

                    match move_info_file(&csv_p, &info_p, &target_mode, &central) {
                        Ok(new_p) => {
                            f.info_file_path = Some(new_p.to_string_lossy().to_string());
                            f.storage_mode = Some(target_mode.as_str().to_string());
                            log_info(
                                "InfoMigrate",
                                &format!("Info-Datei migriert nach {:?}", new_p),
                            );
                        }
                        Err(e) => {
                            log_info("InfoMigrate", &format!("Fehler bei Migration: {}", e));
                        }
                    }
                }
            }
            let _ = save_views_file(&default_views_path(), &st.views_config);
            sync_configuration_ui(&win, &st);
        });
    }

    // Diff info files
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_diff_info_file_clicked(move |f_idx| {
            let st = state.borrow();
            let cur_idx = win.get_selected_view_idx().max(0) as usize;
            if let Some(v) = st.views_config.views.get(cur_idx) {
                let idx = f_idx as usize;
                if let Some(f) = v.files.get(idx) {
                    let csv_p = PathBuf::from(&f.relative_path);
                    let local_info = default_info_path_for_table(&csv_p);
                    let central_info = PathBuf::from(&st.app_config.central_config_directory).join(
                        format!(
                            "{}.info",
                            csv_p.file_name().unwrap_or_default().to_string_lossy()
                        ),
                    );

                    match diff_info_files(&local_info, &central_info) {
                        Ok(diff_text) => {
                            win.set_diff_modal_text(slint::SharedString::from(diff_text));
                            win.set_diff_modal_visible(true);
                        }
                        Err(e) => {
                            win.set_diff_modal_text(slint::SharedString::from(format!(
                                "Diff konnte nicht erstellt werden: {}",
                                e
                            )));
                            win.set_diff_modal_visible(true);
                        }
                    }
                }
            }
        });
    }

    {
        let win = config_window.clone();
        config_window.on_close_diff_modal(move || {
            win.set_diff_modal_visible(false);
        });
    }

    // Open .info editor for file
    {
        let win = config_window.clone();
        let info_win = info_editor_window.clone();
        let state = state.clone();
        let main_ui = main_window_weak.clone();
        config_window.on_open_info_editor_for_file_clicked(move |f_idx| {
            let st = state.borrow();
            let cur_idx = win.get_selected_view_idx().max(0) as usize;
            if let Some(v) = st.views_config.views.get(cur_idx) {
                let idx = f_idx as usize;
                if let Some(f) = v.files.get(idx) {
                    let csv_path = PathBuf::from(&f.relative_path);
                    drop(st);
                    open_info_editor_for_table_or_path(
                        &info_win,
                        Some(&csv_path),
                        &state,
                        main_ui.clone(),
                    );
                    let _ = info_win.show();
                }
            }
        });
    }

    // --- STORAGE CONFIGURATION ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_storage_mode_changed(move |mode_idx| {
            let mut st = state.borrow_mut();
            st.app_config.selected_storage_mode = match mode_idx {
                1 => InfoFileStorageMode::Stammverzeichnis,
                2 => InfoFileStorageMode::Temporaer,
                3 => InfoFileStorageMode::BenutzerEntscheidet,
                _ => InfoFileStorageMode::Dateipfad,
            };
            win.set_selected_storage_mode(mode_idx);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_browse_central_directory(move || {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                let path_str = dir.to_string_lossy().to_string();
                let mut st = state.borrow_mut();
                st.app_config.central_config_directory = path_str.clone();
                win.set_central_directory_path(slint::SharedString::from(path_str));
            }
        });
    }

    {
        let state = state.clone();
        config_window.on_central_directory_edited(move |dir_str| {
            let mut st = state.borrow_mut();
            st.app_config.central_config_directory = dir_str.to_string();
        });
    }

    // --- BRUSHES CONFIGURATION ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_brush_selected(move |b_idx| {
            let st = state.borrow();
            let idx = b_idx as usize;
            if let Some(b) = st.app_config.brushes.get(idx) {
                win.set_edit_brush_key(slint::SharedString::from(&b.key));
                win.set_edit_brush_hex(slint::SharedString::from(&b.hex_value));
                win.set_edit_brush_category(slint::SharedString::from(b.category.as_str()));
                win.set_edit_brush_is_system(b.is_system);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_brush_search_changed(move |_s| {
            refresh_brushes_list(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_update_brush_clicked(move |key, hex, cat| {
            let mut st = state.borrow_mut();
            if let Some(b) = st
                .app_config
                .brushes
                .iter_mut()
                .find(|b| b.key == key.as_str())
            {
                b.hex_value = hex.to_string();
                b.category = BrushCategory::from_str_lenient(&cat);
            }
            if let Ok(mut cache) = get_global_color_cache().lock() {
                cache.update_from_brushes(&st.app_config.brushes);
            }
            refresh_brushes_list(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_brush_clicked(move |key, hex, cat| {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                let mut st = state.borrow_mut();
                if !st
                    .app_config
                    .brushes
                    .iter()
                    .any(|b| b.key.eq_ignore_ascii_case(trimmed))
                {
                    st.app_config.brushes.push(BrushDefinition {
                        key: trimmed.to_string(),
                        hex_value: if hex.trim().is_empty() {
                            "#ffffff".to_string()
                        } else {
                            hex.trim().to_string()
                        },
                        category: BrushCategory::from_str_lenient(&cat),
                        is_system: false,
                    });
                }
                if let Ok(mut cache) = get_global_color_cache().lock() {
                    cache.update_from_brushes(&st.app_config.brushes);
                }
                refresh_brushes_list(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_delete_brush_clicked(move |key| {
            let mut st = state.borrow_mut();
            st.app_config
                .brushes
                .retain(|b| !(b.key == key.as_str() && !b.is_system));
            refresh_brushes_list(&win, &st);
        });
    }

    {
        let state = state.clone();
        config_window.on_marker_priority_changed(move |p| {
            let mut st = state.borrow_mut();
            st.app_config.marker_highlight_priority = p.max(0) as u32;
        });
    }

    // --- ASSOCIATIONS CONFIGURATION ---
    {
        let state = state.clone();
        config_window.on_toggle_associations_startup(move |checked| {
            let mut st = state.borrow_mut();
            st.app_config.check_file_associations_on_startup = checked;
        });
    }

    {
        let win = config_window.clone();
        config_window.on_register_associations_now(move || {
            win.set_association_status_message(slint::SharedString::from(
                "Dateiverknüpfungen für .csv und .csv.info erfolgreich im System registriert.",
            ));
            log_info("Associations", "Dateiverknüpfungen registriert.");
        });
    }

    // --- EXTERNAL APPS CONFIGURATION ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_select_app(move |idx| {
            let st = state.borrow();
            let i = idx as usize;
            if let Some(app) = st.app_config.external_apps.get(i) {
                win.set_selected_app_idx(idx);
                win.set_edit_app_name(slint::SharedString::from(&app.name));
                win.set_edit_app_path(slint::SharedString::from(&app.command_or_path));
                win.set_edit_app_arguments(slint::SharedString::from(&app.arguments));
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_app_clicked(move |name, path, args| {
            let mut st = state.borrow_mut();
            let app_name = name.trim().to_string();
            if app_name.is_empty() {
                return;
            }
            st.app_config.external_apps.push(ExternalAppConfig {
                name: app_name,
                command_or_path: path.trim().to_string(),
                arguments: args.trim().to_string(),
            });
            let new_idx = (st.app_config.external_apps.len() - 1) as i32;
            win.set_selected_app_idx(new_idx);
            let _ = save_app_config(&default_config_path(), &st.app_config);
            sync_configuration_ui(&win, &st);
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_update_app_clicked(move |idx, name, path, args| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i < st.app_config.external_apps.len() {
                let app_name = name.trim().to_string();
                if app_name.is_empty() {
                    return;
                }
                st.app_config.external_apps[i] = ExternalAppConfig {
                    name: app_name,
                    command_or_path: path.trim().to_string(),
                    arguments: args.trim().to_string(),
                };
                let _ = save_app_config(&default_config_path(), &st.app_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_delete_app_clicked(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i < st.app_config.external_apps.len() {
                st.app_config.external_apps.remove(i);
                let new_idx = if st.app_config.external_apps.is_empty() {
                    -1
                } else {
                    (i.min(st.app_config.external_apps.len() - 1)) as i32
                };
                win.set_selected_app_idx(new_idx);
                let _ = save_app_config(&default_config_path(), &st.app_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_app_up(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i > 0 && i < st.app_config.external_apps.len() {
                st.app_config.external_apps.swap(i, i - 1);
                win.set_selected_app_idx((i - 1) as i32);
                let _ = save_app_config(&default_config_path(), &st.app_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_app_down(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i + 1 < st.app_config.external_apps.len() {
                st.app_config.external_apps.swap(i, i + 1);
                win.set_selected_app_idx((i + 1) as i32);
                let _ = save_app_config(&default_config_path(), &st.app_config);
                sync_configuration_ui(&win, &st);
            }
        });
    }

    {
        let win = config_window.clone();
        config_window.on_browse_app_path_clicked(move || {
            let file = rfd::FileDialog::new()
                .add_filter("Ausführbare Dateien (*.exe, *.cmd, *.bat)", &["exe", "cmd", "bat"])
                .pick_file();
            if let Some(p) = file {
                win.set_edit_app_path(slint::SharedString::from(p.to_string_lossy().as_ref()));
            }
        });
    }

    // --- SAVE AND CLOSE ---
    {
        let win = config_window.clone();
        let state = state.clone();
        let main_ui = main_window_weak.clone();
        config_window.on_save_and_close_clicked(move || {
            let mut st = state.borrow_mut();
            let _ = save_app_config(&default_config_path(), &st.app_config);
            let _ = save_views_file(&default_views_path(), &st.views_config);
            if let Ok(mut cache) = get_global_color_cache().lock() {
                cache.update_from_brushes(&st.app_config.brushes);
            }

            let cur_idx = win.get_selected_view_idx().max(0) as usize;
            if cur_idx < st.views_config.views.len() {
                st.activate_and_load_view(cur_idx);
            }

            log_info("Config", "Hauptkonfiguration & Kollektionen gespeichert und angewandt.");
            if let Some(ui) = main_ui.upgrade() {
                crate::sync_ui(&ui, &st);
            }
            let _ = win.hide();
        });
    }

    {
        let win = config_window.clone();
        config_window.on_cancel_clicked(move || {
            let _ = win.hide();
        });
    }
}
