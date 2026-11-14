use std::cell::RefCell;
use std::rc::Rc;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use configurable_csv_analyzer::core::logging::log_info;
use configurable_csv_analyzer::core::transforms::{
    build_smart_add_dialog_state, execute_smart_add_rows, fill_column,
    load_all_suggestions_for_column, split_column, FillMode,
};

use crate::{sync_ui, AppState, KitComboItem, MainWindow, SmartAddFieldItem};

pub fn register(main_window: &MainWindow, state: Rc<RefCell<AppState>>) {
    // --- WORKFLOWS: FILL, SPLIT, SMART ADD ---
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
                    let items: Vec<KitComboItem> = f
                        .suggestions
                        .into_iter()
                        .map(|s| {
                            if s.is_load_more {
                                KitComboItem {
                                    value: slint::SharedString::from("__LOAD_MORE__"),
                                    label: slint::SharedString::from(format!("🔄 {}", s.display)),
                                    badge: slint::SharedString::default(),
                                }
                            } else {
                                KitComboItem {
                                    value: slint::SharedString::from(s.value),
                                    label: slint::SharedString::from(s.display),
                                    badge: slint::SharedString::from(format!("{}×", s.count)),
                                }
                            }
                        })
                        .collect();
                    SmartAddFieldItem {
                        col_idx: f.col_idx as i32,
                        column_name: slint::SharedString::from(f.column_name),
                        concept_name: slint::SharedString::from(f.concept_name),
                        value: slint::SharedString::from(f.value),
                        items: ModelRc::new(VecModel::from(items)),
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
        let state = state.clone();
        main_window.on_smart_add_field_edited(move |field_idx, new_val| {
            if let Some(ui) = ui_handle.upgrade() {
                let model = ui.get_smart_add_fields();
                let idx = field_idx as usize;
                if let Some(mut item) = model.row_data(idx) {
                    if new_val.as_str() == "__LOAD_MORE__" {
                        let st = state.borrow();
                        if let Some(tbl) = st.active_table() {
                            let all_suggs =
                                load_all_suggestions_for_column(tbl, item.col_idx as usize);
                            let slint_items: Vec<KitComboItem> = all_suggs
                                .into_iter()
                                .map(|s| KitComboItem {
                                    value: slint::SharedString::from(s.value),
                                    label: slint::SharedString::from(s.display),
                                    badge: slint::SharedString::from(format!("{}×", s.count)),
                                })
                                .collect();
                            item.items = ModelRc::new(VecModel::from(slint_items));
                            model.set_row_data(idx, item);
                        }
                    } else {
                        item.value = new_val;
                        model.set_row_data(idx, item);
                    }
                }
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
}
