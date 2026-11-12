use std::cell::RefCell;
use std::rc::Rc;
use arboard::Clipboard;
use slint::{ComponentHandle, Model};

use configurable_csv_analyzer::core::logging::log_info;
use configurable_csv_analyzer::core::model::TextMarkerColor;
use configurable_csv_analyzer::core::transforms::{export_to_clipboard_string, paste_clipboard_data};

use crate::{sync_ui, AppState, MainWindow};

pub fn register(main_window: &MainWindow, state: Rc<RefCell<AppState>>) {
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

    // --- SEARCH & SORT ---
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
        let state = state.clone();
        main_window.on_column_resized(move |col_idx, new_width| {
            let mut st = state.borrow_mut();
            let new_w = (new_width as u32).max(40);
            let mut col_name_opt = None;
            let mut tbl_file_name = String::new();
            let mut tbl_name = String::new();

            if let Some(tbl) = st.active_table_mut() {
                if let Some(h) = tbl.headers.get(col_idx as usize) {
                    let h_str = h.clone();
                    col_name_opt = Some(h_str.clone());
                    tbl_file_name = tbl
                        .file_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    tbl_name = tbl.display_name.clone();

                    if let Some(setting) = tbl
                        .column_settings
                        .iter_mut()
                        .find(|s| s.column_name.eq_ignore_ascii_case(&h_str))
                    {
                        setting.width = new_w;
                    } else {
                        tbl.column_settings.push(configurable_csv_analyzer::core::model::ColumnSetting {
                            column_name: h_str,
                            width: new_w,
                            is_visible: true,
                            display_order: col_idx as u32,
                        });
                    }
                }
            }

            if let Some(col_name) = col_name_opt {
                let active_view_name = st.active_view_name.clone();
                let col_key = format!("{}::{}", tbl_name.to_lowercase(), col_name.to_lowercase());
                let mut saved_any = false;

                for v in &mut st.views_config.views {
                    if active_view_name.as_deref() == Some(&v.name)
                        || v.files.iter().any(|f| {
                            f.relative_path.eq_ignore_ascii_case(&tbl_file_name)
                                || f.full_path.as_deref().unwrap_or("").eq_ignore_ascii_case(&tbl_file_name)
                        })
                    {
                        v.column_widths.insert(col_key.clone(), new_w);
                        saved_any = true;
                    }
                }

                if !saved_any {
                    if let Some(ref mut temp_view) = st.temp_collection {
                        temp_view.column_widths.insert(col_key, new_w);
                    }
                }

                let _ = configurable_csv_analyzer::core::io::info::save_views_file(
                    &configurable_csv_analyzer::core::io::info::default_views_path(),
                    &st.views_config,
                );
            }

            // Update in-place in existing headers_model so TouchArea is NOT destroyed
            let c_idx = col_idx as usize;
            if let Some(mut h_item) = st.headers_model.row_data(c_idx) {
                h_item.width = new_w as f32;
                st.headers_model.set_row_data(c_idx, h_item);
            }
        });
    }

    // --- RULES & EDITING ---
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

    // --- DELETE MARKED ROWS ---
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
}
