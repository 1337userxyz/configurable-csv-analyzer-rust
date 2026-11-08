use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use configurable_csv_analyzer::core::model::FilterCondition;

use crate::{sync_ui, AppState, FilterChoiceItem, MainWindow};

pub fn register(main_window: &MainWindow, state: Rc<RefCell<AppState>>) {
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
}
