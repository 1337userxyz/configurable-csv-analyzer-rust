use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use slint::ComponentHandle;
use kit_slint::ToSlintModel;

use configurable_csv_analyzer::core::io::info::{
    default_config_path, default_info_path_for_table, default_views_path, save_app_config, save_views_file,
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
            st.collection_filter_active = true;
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
            st.collection_filter_active = true;
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
            st.collection_filter_active = false;
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
                    base_path: None,
                    color_pattern: None,
                    export_color_pattern: None,
                    export_paths: Vec::new(),
                    column_widths: std::collections::HashMap::new(),
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

    // --- MOVE TABLE TO COLLECTION (WITH SAFETY CONFIRMATION) ---
    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_open_move_table_dialog(move |table_id| {
            let ui = match ui_handle.upgrade() {
                Some(u) => u,
                None => return,
            };
            let st = state.borrow();
            let tbl = match st.tables.get(table_id.as_str()) {
                Some(t) => t,
                None => return,
            };

            // Find source collection
            let mut src_col = "Temporäre Kollektion".to_string();
            if let Some(v) = st.views_config.views.iter().find(|v| st.is_table_in_view(tbl, v)) {
                src_col = v.name.clone();
            }

            // Available targets (all views with a different name)
            let mut targets: Vec<String> = st
                .views_config
                .views
                .iter()
                .filter(|v| v.name != src_col)
                .map(|v| v.name.clone())
                .collect();
            if targets.is_empty() {
                targets.push("Standard-Kollektion".to_string());
            }

            let first_target = targets[0].clone();
            let target_view = st.views_config.views.iter().find(|v| v.name == first_target);
            let has_base = target_view.and_then(|v| v.base_path.as_ref()).is_some();
            let base_path = target_view
                .and_then(|v| v.base_path.clone())
                .unwrap_or_default();

            ui.set_move_modal_table_id(table_id);
            ui.set_move_modal_table_name(slint::SharedString::from(&tbl.display_name));
            ui.set_move_modal_src_collection(slint::SharedString::from(&src_col));
            ui.set_move_modal_target_collection(slint::SharedString::from(&first_target));
            let targets_slint: Vec<slint::SharedString> =
                targets.iter().map(slint::SharedString::from).collect();
            ui.set_move_modal_available_targets(targets_slint.to_slint_model());
            ui.set_move_modal_has_target_base_path(has_base);
            ui.set_move_modal_target_base_path(slint::SharedString::from(base_path));
            ui.set_move_modal_move_physical_file(has_base);
            ui.set_move_modal_visible(true);
        });
    }

    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_move_modal_target_changed(move |target_name| {
            let ui = match ui_handle.upgrade() {
                Some(u) => u,
                None => return,
            };
            let st = state.borrow();
            let target_view = st
                .views_config
                .views
                .iter()
                .find(|v| v.name == target_name.as_str());
            let has_base = target_view.and_then(|v| v.base_path.as_ref()).is_some();
            let base_path = target_view
                .and_then(|v| v.base_path.clone())
                .unwrap_or_default();
            ui.set_move_modal_has_target_base_path(has_base);
            ui.set_move_modal_target_base_path(slint::SharedString::from(base_path));
            ui.set_move_modal_move_physical_file(has_base);
        });
    }

    {
        let ui_handle = main_window.as_weak();
        let state = state.clone();
        main_window.on_confirm_move_table(move |table_id, target_col, move_file| {
            let ui = match ui_handle.upgrade() {
                Some(u) => u,
                None => return,
            };
            let mut st = state.borrow_mut();
            let (old_path, _is_ods, fname) = match st.tables.get(table_id.as_str()) {
                Some(t) => {
                    let p = t.file_path.clone();
                    let fnm = p
                        .as_ref()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                        .unwrap_or_else(|| format!("{}.csv", t.display_name));
                    (p, t.is_ods, fnm)
                }
                None => return,
            };

            // 1. Remove table entry from all source views
            for v in &mut st.views_config.views {
                if let Some(ref p) = old_path {
                    let p_str = p.to_string_lossy().to_string();
                    v.files.retain(|f| {
                        !f.relative_path.eq_ignore_ascii_case(&p_str)
                            && f.full_path
                                .as_deref()
                                .map(|fp| !fp.eq_ignore_ascii_case(&p_str))
                                .unwrap_or(true)
                    });
                }
            }
            if let Some(ref mut tc) = st.temp_collection {
                if let Some(ref p) = old_path {
                    let p_str = p.to_string_lossy().to_string();
                    tc.files.retain(|f| {
                        !f.relative_path.eq_ignore_ascii_case(&p_str)
                            && f.full_path
                                .as_deref()
                                .map(|fp| !fp.eq_ignore_ascii_case(&p_str))
                                .unwrap_or(true)
                    });
                }
            }

            // 2. Determine final path (physically move if requested and target has base_path)
            let mut final_path = old_path.clone();
            if move_file {
                if let Some(target_view) = st
                    .views_config
                    .views
                    .iter()
                    .find(|v| v.name == target_col.as_str())
                {
                    if let Some(ref base_str) = target_view.base_path {
                        let base_p = PathBuf::from(base_str);
                        let _ = std::fs::create_dir_all(&base_p);
                        let new_dest = base_p.join(&fname);
                        if let Some(ref src_p) = old_path {
                            if src_p.exists() && *src_p != new_dest {
                                let _ = std::fs::copy(src_p, &new_dest);
                                let _ = std::fs::remove_file(src_p);

                                let old_info = default_info_path_for_table(src_p);
                                if old_info.exists() {
                                    let new_info = default_info_path_for_table(&new_dest);
                                    let _ = std::fs::copy(&old_info, &new_info);
                                    let _ = std::fs::remove_file(&old_info);
                                }
                                log_info(
                                    "Collection",
                                    &format!(
                                        "Datei physisch verschoben von {:?} nach {:?}",
                                        src_p, new_dest
                                    ),
                                );
                            }
                        }
                        final_path = Some(new_dest);
                    }
                }
            }

            // 3. Update table in AppState
            if let Some(t) = st.tables.get_mut(table_id.as_str()) {
                t.file_path = final_path.clone();
            }

            // 4. Add to target collection in views_config
            let final_path_str = final_path
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if !final_path_str.is_empty() {
                let storage_mode_str = st.app_config.selected_storage_mode.as_str().to_string();
                if let Some(target_view) = st
                    .views_config
                    .views
                    .iter_mut()
                    .find(|v| v.name == target_col.as_str())
                {
                    target_view.files.push(ViewFileItem {
                        relative_path: final_path_str.clone(),
                        full_path: Some(final_path_str.clone()),
                        info_file_path: None,
                        storage_mode: Some(storage_mode_str),
                    });
                }
            }

            let _ = save_views_file(&default_views_path(), &st.views_config);
            log_info(
                "Collection",
                &format!(
                    "Tabelle '{}' in Kollektion '{}' verschoben.",
                    table_id, target_col
                ),
            );
            ui.set_status_message(slint::SharedString::from(format!(
                "Tabelle in Kollektion '{}' verschoben.",
                target_col
            )));
            sync_ui(&ui, &st);
        });
    }
}
