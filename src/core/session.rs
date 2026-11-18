use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::core::color_cache::get_global_color_cache;
use crate::core::io::csv::{load_csv, save_csv};
use crate::core::io::info::InfoStoragePolicy;
use crate::core::logging::{log_error, log_info};
use crate::core::model::{
    AppConfig, BrushDefinition, ColumnSetting, FilterCondition, TableData, TextMarkerColor,
};
use crate::core::rules::RuleEngine;
use crate::core::transforms::{
    build_smart_add_dialog_state, execute_smart_add_rows, export_to_clipboard_string, fill_column,
    paste_clipboard_data, split_column, FillMode, SmartAddDialogState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPopupItem {
    pub value: String,
    pub display_text: String,
    pub count: usize,
    pub selected: bool,
    pub visible: bool,
}

/// Wiederverwendbare, UI- und kollektionsunabhängige Session für CSV-Tabellen,
/// `.info`-Konfigurationen, RuleEngine, Filter, Sortierung, SmartAdd, Fill und Textmarker.
pub struct CsvTableSession {
    pub tables: HashMap<String, TableData>,
    pub table_order: Vec<String>,
    pub active_table_id: Option<String>,
    pub info_policy: InfoStoragePolicy,
    pub brushes: Vec<BrushDefinition>,
    pub search_query: String,
    pub only_marked: bool,
    pub column_widths: HashMap<String, u32>,
}

impl CsvTableSession {
    pub fn new(info_policy: InfoStoragePolicy) -> Self {
        let brushes = AppConfig::default_brushes();
        if let Ok(mut cache) = get_global_color_cache().lock() {
            cache.update_from_brushes(&brushes);
        }
        Self {
            tables: HashMap::new(),
            table_order: Vec::new(),
            active_table_id: None,
            info_policy,
            brushes,
            search_query: String::new(),
            only_marked: false,
            column_widths: HashMap::new(),
        }
    }

    pub fn load_csv_file(&mut self, path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let table = load_csv(path, None, None, true)?;
        Ok(self.add_table(table))
    }

    pub fn add_table(&mut self, mut table: TableData) -> String {
        match self.info_policy.load_info_for_table(&mut table) {
            Ok(Some(info_path)) => {
                log_info(
                    "InfoFile",
                    &format!(
                        "Konfiguration geladen für '{}': {:?} ({} Regeln)",
                        table.display_name,
                        info_path,
                        table.rules.len()
                    ),
                );
            }
            Ok(None) => {}
            Err(e) => {
                log_error(
                    "InfoFile",
                    &format!(
                        "Fehler beim Laden der Info-Datei für '{}': {}",
                        table.display_name, e
                    ),
                );
            }
        }

        let id = table.id.clone();
        if !self.table_order.contains(&id) {
            self.table_order.push(id.clone());
        }
        self.tables.insert(id.clone(), table);
        self.active_table_id = Some(id.clone());
        self.execute_all_rules();
        id
    }

    pub fn save_active_table(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let policy = self.info_policy.clone();
        let tbl = self
            .active_table_mut()
            .ok_or("Keine aktive Tabelle zum Speichern ausgewählt.")?;
        let path = tbl
            .file_path
            .clone()
            .ok_or("Aktive Tabelle hat keinen Dateipfad.")?;

        save_csv(&path, tbl, tbl.delimiter, &tbl.encoding)?;
        tbl.is_dirty = false;
        let saved_info = policy.save_info_for_table(tbl)?;
        log_info(
            "Save",
            &format!(
                "Tabelle {:?} und Info-Datei {:?} gespeichert.",
                path, saved_info
            ),
        );
        Ok(())
    }

    pub fn revert_active_table(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (path, delimiter, encoding) = {
            let tbl = self
                .active_table()
                .ok_or("Keine aktive Tabelle zum Zurücksetzen ausgewählt.")?;
            let path = tbl
                .file_path
                .clone()
                .ok_or("Aktive Tabelle hat keinen Dateipfad.")?;
            (path, tbl.delimiter, tbl.encoding.clone())
        };

        let reloaded = load_csv(&path, Some(delimiter), Some(&encoding), true)?;
        log_info(
            "Revert",
            &format!("Tabelle zurückgesetzt auf Stand der Datei {:?}", path),
        );
        self.add_table(reloaded);
        Ok(())
    }

    pub fn select_table(&mut self, table_id: &str) {
        if self.tables.contains_key(table_id) {
            self.active_table_id = Some(table_id.to_string());
        }
    }

    pub fn active_table(&self) -> Option<&TableData> {
        self.active_table_id
            .as_ref()
            .and_then(|id| self.tables.get(id))
    }

    pub fn active_table_mut(&mut self) -> Option<&mut TableData> {
        self.active_table_id
            .as_ref()
            .and_then(|id| self.tables.get_mut(id))
    }

    pub fn execute_all_rules(&mut self) {
        let ids = self.table_order.clone();
        for id in ids {
            let all_tbls = self.tables.clone();
            if let Some(tbl) = self.tables.get_mut(&id) {
                if !tbl.rules.is_empty() || tbl.is_virtual_col.iter().any(|&v| v) {
                    RuleEngine::execute(tbl, &all_tbls);
                }
            }
        }
    }

    pub fn find_brush_hex<'a>(&'a self, key_or_hex: &'a str) -> &'a str {
        self.brushes
            .iter()
            .find(|b| b.key.eq_ignore_ascii_case(key_or_hex))
            .map(|b| b.hex_value.as_str())
            .unwrap_or(key_or_hex)
    }

    pub fn visible_row_indices_for_table(&self, tbl: &TableData) -> Vec<usize> {
        let mut visible = tbl.visible_row_indices(self.only_marked);
        let q = self.search_query.trim().to_lowercase();
        if !q.is_empty() {
            visible.retain(|&r_idx| {
                tbl.rows
                    .get(r_idx)
                    .map(|row| row.iter().any(|cell| cell.to_lowercase().contains(&q)))
                    .unwrap_or(false)
            });
        }
        visible
    }

    pub fn resolve_column_width(&self, tbl: &TableData, idx: usize, header: &str) -> u32 {
        let col_key = format!(
            "{}::{}",
            tbl.display_name.to_lowercase(),
            header.to_lowercase()
        );
        if let Some(&w) = self
            .column_widths
            .get(&col_key)
            .or_else(|| self.column_widths.get(header))
        {
            return w;
        }
        tbl.column_settings
            .iter()
            .find(|s| s.column_name.eq_ignore_ascii_case(header))
            .or_else(|| tbl.column_settings.get(idx))
            .map(|s| s.width)
            .unwrap_or(150)
    }

    pub fn set_cell(&mut self, orig_row: usize, col: usize, new_val: String) {
        if let Some(tbl) = self.active_table_mut() {
            tbl.set_cell(orig_row, col, new_val);
        }
        self.execute_all_rules();
    }

    pub fn toggle_sort(&mut self, col_idx: usize) {
        if let Some(tbl) = self.active_table_mut() {
            tbl.sort_column = match tbl.sort_column {
                Some((cur_c, true)) if cur_c == col_idx => Some((col_idx, false)),
                Some((cur_c, false)) if cur_c == col_idx => None,
                _ => Some((col_idx, true)),
            };
        }
    }

    pub fn resize_column(&mut self, col_idx: usize, new_width: u32) {
        let new_w = new_width.max(40);
        let mut key_opt = None;
        if let Some(tbl) = self.active_table_mut() {
            if let Some(h) = tbl.headers.get(col_idx).cloned() {
                key_opt = Some(format!(
                    "{}::{}",
                    tbl.display_name.to_lowercase(),
                    h.to_lowercase()
                ));
                if let Some(setting) = tbl
                    .column_settings
                    .iter_mut()
                    .find(|s| s.column_name.eq_ignore_ascii_case(&h))
                {
                    setting.width = new_w;
                } else {
                    tbl.column_settings.push(ColumnSetting {
                        column_name: h,
                        width: new_w,
                        is_visible: true,
                        display_order: col_idx as u32,
                    });
                }
            }
        }
        if let Some(key) = key_opt {
            self.column_widths.insert(key, new_w);
        }
    }

    pub fn toggle_row_mark(&mut self, orig_row: usize, checked: bool) {
        if let Some(tbl) = self.active_table_mut() {
            if checked {
                tbl.marked_rows.insert(orig_row);
            } else {
                tbl.marked_rows.remove(&orig_row);
            }
        }
    }

    pub fn select_all_visible(&mut self, checked: bool) {
        let visible = if checked {
            self.active_table()
                .map(|tbl| self.visible_row_indices_for_table(tbl))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(tbl) = self.active_table_mut() {
            if checked {
                for r in visible {
                    tbl.marked_rows.insert(r);
                }
            } else {
                tbl.marked_rows.clear();
            }
        }
    }

    pub fn mark_empty_rows(&mut self) {
        if let Some(tbl) = self.active_table_mut() {
            for (i, row) in tbl.rows.iter().enumerate() {
                if row.iter().all(|c| c.trim().is_empty()) {
                    tbl.marked_rows.insert(i);
                }
            }
        }
    }

    pub fn clear_selection(&mut self) {
        if let Some(tbl) = self.active_table_mut() {
            tbl.marked_rows.clear();
        }
    }

    pub fn apply_marker_color(&mut self, hex: &str) {
        if let Some(tbl) = self.active_table_mut() {
            let color = match hex {
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
    }

    pub fn clear_markers(&mut self) {
        if let Some(tbl) = self.active_table_mut() {
            let marked: Vec<usize> = tbl.marked_rows.iter().copied().collect();
            for r in marked {
                tbl.row_markers.remove(&r);
            }
        }
    }

    pub fn delete_marked_rows(&mut self) {
        if let Some(tbl) = self.active_table_mut() {
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
        self.execute_all_rules();
    }

    pub fn apply_fill_column(&mut self, col_idx: usize, mode: FillMode) {
        if let Some(tbl) = self.active_table_mut() {
            let marked: Vec<usize> = tbl.marked_rows.iter().copied().collect();
            fill_column(tbl, col_idx, &marked, mode);
            log_info("Transform", &format!("Spalte {} befüllt.", col_idx + 1));
        }
        self.execute_all_rules();
    }

    pub fn apply_split_column(&mut self, col_idx: usize, delimiter: char, new_col_names: &[String]) {
        if let Some(tbl) = self.active_table_mut() {
            split_column(tbl, col_idx, delimiter, new_col_names);
            log_info(
                "Transform",
                &format!("Spalte {} nach '{}' aufgeteilt.", col_idx + 1, delimiter),
            );
        }
        self.execute_all_rules();
    }

    pub fn build_smart_add_state(&self) -> Option<SmartAddDialogState> {
        let tbl = self.active_table()?;
        Some(build_smart_add_dialog_state(
            tbl,
            self.only_marked,
            &self.search_query,
        ))
    }

    pub fn confirm_smart_add_rows(
        &mut self,
        row_count: usize,
        col_values: &[(usize, String)],
    ) -> Vec<usize> {
        let only_marked = self.only_marked;
        let created = if let Some(tbl) = self.active_table_mut() {
            let indices = execute_smart_add_rows(tbl, row_count.max(1), col_values);
            if only_marked {
                for &idx in &indices {
                    tbl.marked_rows.insert(idx);
                }
            }
            log_info(
                "SmartAdd",
                &format!(
                    "{} neue Zeile(n) zu '{}' hinzugefügt.",
                    indices.len(),
                    tbl.display_name
                ),
            );
            indices
        } else {
            Vec::new()
        };
        if !created.is_empty() {
            self.execute_all_rules();
        }
        created
    }

    pub fn build_filter_popup(
        &self,
        col_idx: usize,
    ) -> Option<(String, usize, Vec<FilterPopupItem>)> {
        let tbl = self.active_table()?;
        let col_name = tbl.headers.get(col_idx).cloned().unwrap_or_default();

        let other_matched_rows =
            tbl.rows_matching_other_filters(col_idx, self.only_marked, &self.search_query);

        let mut value_counts: HashMap<String, usize> = HashMap::new();
        for &r_idx in &other_matched_rows {
            if let Some(row) = tbl.rows.get(r_idx) {
                let raw = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let key = if raw.trim().is_empty() {
                    String::new()
                } else {
                    raw.to_string()
                };
                *value_counts.entry(key).or_insert(0) += 1;
            }
        }

        let table_has_empty = tbl.rows.iter().any(|r| {
            r.get(col_idx)
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        });
        if table_has_empty {
            value_counts.entry(String::new()).or_insert(0);
        }

        let current_filter = tbl.active_filters.get(&col_idx);
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

        let items = distinct_values
            .into_iter()
            .map(|val| {
                let count = value_counts.get(&val).copied().unwrap_or(0);
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
                let display_text = if val.is_empty() {
                    "(Leere)".to_string()
                } else {
                    val.clone()
                };
                FilterPopupItem {
                    value: val,
                    display_text,
                    count,
                    selected,
                    visible: true,
                }
            })
            .collect();

        Some((col_name, other_matched_rows.len(), items))
    }

    pub fn apply_column_filter(
        &mut self,
        col_idx: usize,
        selected_values: HashSet<String>,
        include_empty: bool,
        all_checked: bool,
    ) {
        if let Some(tbl) = self.active_table_mut() {
            if all_checked {
                tbl.active_filters.remove(&col_idx);
            } else {
                let cond = FilterCondition {
                    include_non_empty: !selected_values.is_empty(),
                    selected_values,
                    all_selected: false,
                    include_empty,
                    text_search: String::new(),
                };
                tbl.active_filters.insert(col_idx, cond);
            }
        }
    }

    pub fn reset_column_filter(&mut self, col_idx: usize) {
        if let Some(tbl) = self.active_table_mut() {
            tbl.active_filters.remove(&col_idx);
        }
    }

    pub fn clear_all_filters(&mut self) {
        if let Some(tbl) = self.active_table_mut() {
            tbl.active_filters.clear();
        }
    }

    pub fn export_clipboard_tsv(&self, include_headers: bool) -> Option<String> {
        let tbl = self.active_table()?;
        let rows_to_copy = self.visible_row_indices_for_table(tbl);
        Some(export_to_clipboard_string(
            tbl,
            &rows_to_copy,
            '\t',
            include_headers,
        ))
    }

    pub fn paste_clipboard_tsv(&mut self, text: &str) {
        if let Some(tbl) = self.active_table_mut() {
            paste_clipboard_data(tbl, text, tbl.rows.len(), 0);
            log_info("Clipboard", "Daten aus der Zwischenablage eingefügt.");
        }
        self.execute_all_rules();
    }
}
