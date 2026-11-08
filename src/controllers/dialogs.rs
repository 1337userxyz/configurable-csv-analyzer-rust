use std::cell::RefCell;
use std::rc::Rc;
use arboard::Clipboard;
use slint::ComponentHandle;

use configurable_csv_analyzer::core::logging::{get_global_logger, log_info};
use configurable_csv_analyzer::core::transforms::compare_tables;

use crate::{dialogs, AppState, ConfigurationWindow, InfoFileEditorWindow, MainWindow};

pub fn register(
    main_window: &MainWindow,
    state: Rc<RefCell<AppState>>,
    config_window: Rc<ConfigurationWindow>,
    info_editor_window: Rc<InfoFileEditorWindow>,
) {
    // --- SHOW CONFIGURATION WINDOW ---
    {
        let config_win = config_window.clone();
        let state = state.clone();
        main_window.on_show_config_clicked(move || {
            {
                let mut st = state.borrow_mut();
                dialogs::configuration::ensure_selected_view(&config_win, &mut st);
                dialogs::configuration::sync_configuration_ui(&config_win, &st);
            }
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
}
