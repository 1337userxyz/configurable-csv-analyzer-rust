use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use slint::ComponentHandle;

use configurable_csv_analyzer::core::io::info::{
    default_config_path, default_views_path, save_app_config, save_views_file,
};
use configurable_csv_analyzer::core::logging::log_info;
use configurable_csv_analyzer::core::model::{ViewDefinition, ViewFileItem};

use crate::{sync_ui, AppState, MainWindow};

pub fn register(main_window: &MainWindow, state: Rc<RefCell<AppState>>) {
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
}
