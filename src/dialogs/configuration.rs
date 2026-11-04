use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};

use configurable_csv_analyzer::core::color_cache::cached_color;
use configurable_csv_analyzer::core::io::csv::find_all_csvs_in_dir;
use configurable_csv_analyzer::core::io::info::{
    check_file_integrity, default_config_path, default_info_path_for_table, default_views_path,
    diff_info_files, move_info_file, save_app_config, save_views_file,
};
use configurable_csv_analyzer::core::logging::log_info;
use configurable_csv_analyzer::core::model::{
    BrushCategory, BrushDefinition, InfoFileStorageMode, ViewDefinition, ViewFileItem,
};

use crate::{
    AppState, ConfigurationWindow, InfoFileEditorWindow, MainWindow,
    SlintBrushItem, SlintViewFileItem, SlintViewItem,
};
use crate::dialogs::info_editor::open_info_editor_for_table_or_path;

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
            spreadsheet_path: slint::SharedString::from(v.spreadsheet_path.as_deref().unwrap_or("")),
            color_pattern: slint::SharedString::from(v.color_pattern.as_deref().unwrap_or("None")),
            export_color_pattern: slint::SharedString::from(v.export_color_pattern.as_deref().unwrap_or("None")),
        })
        .collect();

    window.set_views(ModelRc::new(VecModel::from(view_items)));

    let cur_idx = window.get_selected_view_idx() as usize;
    if let Some(v) = state.views_config.views.get(cur_idx) {
        window.set_current_spreadsheet_path(slint::SharedString::from(v.spreadsheet_path.as_deref().unwrap_or("")));
        window.set_current_color_pattern(slint::SharedString::from(v.color_pattern.as_deref().unwrap_or("None")));
        window.set_current_export_color_pattern(slint::SharedString::from(v.export_color_pattern.as_deref().unwrap_or("None")));

        let file_items: Vec<SlintViewFileItem> = v
            .files
            .iter()
            .enumerate()
            .map(|(f_idx, f)| {
                let status = check_file_integrity(&f.relative_path, f.info_file_path.as_deref(), None);
                let sm = f.storage_mode.as_deref().unwrap_or("Standard");
                SlintViewFileItem {
                    id: f_idx as i32,
                    relative_path: slint::SharedString::from(&f.relative_path),
                    info_file_path: slint::SharedString::from(f.info_file_path.as_deref().unwrap_or("")),
                    storage_mode: slint::SharedString::from(sm),
                    integrity_status: slint::SharedString::from(status),
                }
            })
            .collect();
        window.set_current_view_files(ModelRc::new(VecModel::from(file_items)));
    } else {
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
    window.set_central_directory_path(slint::SharedString::from(&state.app_config.central_config_directory));

    // 3. Brushes
    refresh_brushes_list(window, state);
    window.set_marker_priority(state.app_config.marker_highlight_priority as i32);

    // 4. Associations
    window.set_check_associations_on_startup(state.app_config.check_file_associations_on_startup);
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
            let is_default = st.views_config.views.is_empty();
            st.views_config.views.push(ViewDefinition {
                name: name.to_string(),
                is_default_view: is_default,
                files: Vec::new(),
                spreadsheet_path: None,
                color_pattern: Some("None".into()),
                export_color_pattern: Some("None".into()),
                export_paths: Vec::new(),
            });
            win.set_selected_view_idx((st.views_config.views.len() - 1) as i32);
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_delete_view_clicked(move |idx| {
            let mut st = state.borrow_mut();
            let i = idx as usize;
            if i < st.views_config.views.len() {
                st.views_config.views.remove(i);
                if win.get_selected_view_idx() >= st.views_config.views.len() as i32 {
                    win.set_selected_view_idx((st.views_config.views.len().saturating_sub(1)) as i32);
                }
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
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
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
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
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
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
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_browse_spreadsheet_clicked(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("LibreOffice Calc", &["ods"])
                .pick_file()
            {
                let mut st = state.borrow_mut();
                let cur_idx = win.get_selected_view_idx() as usize;
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    v.spreadsheet_path = Some(path.to_string_lossy().to_string());
                }
                drop(st);
                sync_configuration_ui(&win, &state.borrow());
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_clear_spreadsheet_clicked(move || {
            let mut st = state.borrow_mut();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                v.spreadsheet_path = None;
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_view_patterns_changed(move |color_p, export_p| {
            let mut st = state.borrow_mut();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                v.color_pattern = Some(color_p.to_string());
                v.export_color_pattern = Some(export_p.to_string());
            }
        });
    }

    // --- VIEW FILES OPERATIONS ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_csv_to_view_clicked(move || {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("CSV Dateien", &["csv", "tsv", "txt"])
                .pick_file()
            {
                let mut st = state.borrow_mut();
                let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
                let cur_idx = win.get_selected_view_idx() as usize;
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    let rel = file.to_string_lossy().to_string();
                    if !v.files.iter().any(|f| f.relative_path == rel) {
                        v.files.push(ViewFileItem {
                            relative_path: rel,
                            full_path: Some(file.to_string_lossy().to_string()),
                            info_file_path: None,
                            storage_mode: Some(storage_mode_str),
                        });
                    }
                }
                drop(st);
                sync_configuration_ui(&win, &state.borrow());
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_folder_to_view_clicked(move || {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let csvs = find_all_csvs_in_dir(&folder);
                let mut st = state.borrow_mut();
                let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
                let cur_idx = win.get_selected_view_idx() as usize;
                if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                    for path in csvs {
                        let rel = path.to_string_lossy().to_string();
                        if !v.files.iter().any(|f| f.relative_path == rel) {
                            v.files.push(ViewFileItem {
                                relative_path: rel,
                                full_path: Some(path.to_string_lossy().to_string()),
                                info_file_path: None,
                                storage_mode: Some(storage_mode_str.clone()),
                            });
                        }
                    }
                }
                drop(st);
                sync_configuration_ui(&win, &state.borrow());
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_remove_file_from_view_clicked(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx < v.files.len() {
                    v.files.remove(idx);
                }
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_file_up(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx > 0 && idx < v.files.len() {
                    v.files.swap(idx, idx - 1);
                }
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_move_file_down(move |f_idx| {
            let mut st = state.borrow_mut();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get_mut(cur_idx) {
                let idx = f_idx as usize;
                if idx + 1 < v.files.len() {
                    v.files.swap(idx, idx + 1);
                }
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    // Migrate info file between central and decentralized
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_migrate_info_file_clicked(move |f_idx| {
            let mut st = state.borrow_mut();
            let central = PathBuf::from(&st.app_config.central_config_directory);
            let cur_idx = win.get_selected_view_idx() as usize;
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
                            log_info("InfoMigrate", &format!("Info-Datei migriert nach {:?}", new_p));
                        }
                        Err(e) => {
                            log_info("InfoMigrate", &format!("Fehler bei Migration: {}", e));
                        }
                    }
                }
            }
            drop(st);
            sync_configuration_ui(&win, &state.borrow());
        });
    }

    // Diff info files
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_diff_info_file_clicked(move |f_idx| {
            let st = state.borrow();
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get(cur_idx) {
                let idx = f_idx as usize;
                if let Some(f) = v.files.get(idx) {
                    let csv_p = PathBuf::from(&f.relative_path);
                    let local_info = default_info_path_for_table(&csv_p);
                    let central_info = PathBuf::from(&st.app_config.central_config_directory)
                        .join(format!("{}.info", csv_p.file_name().unwrap_or_default().to_string_lossy()));

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
            let cur_idx = win.get_selected_view_idx() as usize;
            if let Some(v) = st.views_config.views.get(cur_idx) {
                let idx = f_idx as usize;
                if let Some(f) = v.files.get(idx) {
                    let csv_path = PathBuf::from(&f.relative_path);
                    open_info_editor_for_table_or_path(&info_win, Some(&csv_path), &state, main_ui.clone());
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
            if let Some(b) = st.app_config.brushes.iter_mut().find(|b| b.key == key.as_str()) {
                b.hex_value = hex.to_string();
                b.category = BrushCategory::from_str_lenient(&cat);
            }
            drop(st);
            refresh_brushes_list(&win, &state.borrow());
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_add_brush_clicked(move |key, hex, cat| {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                let mut st = state.borrow_mut();
                if !st.app_config.brushes.iter().any(|b| b.key.eq_ignore_ascii_case(trimmed)) {
                    st.app_config.brushes.push(BrushDefinition {
                        key: trimmed.to_string(),
                        hex_value: if hex.trim().is_empty() { "#ffffff".to_string() } else { hex.trim().to_string() },
                        category: BrushCategory::from_str_lenient(&cat),
                        is_system: false,
                    });
                }
                drop(st);
                refresh_brushes_list(&win, &state.borrow());
            }
        });
    }

    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_delete_brush_clicked(move |key| {
            let mut st = state.borrow_mut();
            st.app_config.brushes.retain(|b| !(b.key == key.as_str() && !b.is_system));
            drop(st);
            refresh_brushes_list(&win, &state.borrow());
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
            // Register file associations (or set clean status on Windows)
            win.set_association_status_message(slint::SharedString::from(
                "Dateiverknüpfungen für .csv und .csv.info erfolgreich im System registriert.",
            ));
            log_info("Associations", "Dateiverknüpfungen registriert.");
        });
    }

    // --- SAVE AND CLOSE ---
    {
        let win = config_window.clone();
        let state = state.clone();
        config_window.on_save_and_close_clicked(move || {
            let st = state.borrow();
            let _ = save_app_config(&default_config_path(), &st.app_config);
            let _ = save_views_file(&default_views_path(), &st.views_config);
            log_info("Config", "Hauptkonfiguration & Views gespeichert.");
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
