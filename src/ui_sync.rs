use std::collections::HashMap;
use slint::{Color, ComponentHandle, ModelRc, VecModel};
use kit_slint::ToSlintModel;
use kit_slint::color::contrast_text;

use configurable_csv_analyzer::core::color_cache::cached_color;
use configurable_csv_analyzer::core::logging::get_global_logger;
use configurable_csv_analyzer::core::model::{BrushCategory, HighlightItem, TableData};

use crate::{
    AppState, AppTheme, CellItem, KitColorItem, ConfigurationWindow, HeaderItem,
    InfoFileEditorWindow, LogViewItem, MainWindow, ProposalItem, RowItem, RuleSummaryItem,
    SlintCollectionFilterItem, SlintCollectionTabGroup, SlintExternalAppItem, TabItem, TabRowItem,
};

pub fn apply_theme_all(
    main_window: Option<&MainWindow>,
    config_window: Option<&ConfigurationWindow>,
    info_editor_window: Option<&InfoFileEditorWindow>,
    mode: kit_theme::ThemeMode,
) {
    let m: slint::SharedString = mode.as_str().into();
    if let Some(w) = main_window {
        w.global::<AppTheme>().invoke_apply(m.clone());
    }
    if let Some(w) = config_window {
        w.global::<AppTheme>().invoke_apply(m.clone());
    }
    if let Some(w) = info_editor_window {
        w.global::<AppTheme>().invoke_apply(m);
    }
}

pub fn resolve_brush_color(key_or_hex: &str, state: &AppState) -> Color {
    if let Some(hex) = state.app_config.find_brush_hex(key_or_hex) {
        cached_color(hex, 180)
    } else {
        cached_color(key_or_hex, 180)
    }
}

pub fn parse_hex_color(hex: &str) -> Color {
    cached_color(hex, 180)
}

pub fn sync_ui(ui: &MainWindow, state: &AppState) {
    // 0. Sync available views / active collection & marker palette & external apps
    let view_names: Vec<slint::SharedString> = state
        .views_config
        .views
        .iter()
        .map(|v| slint::SharedString::from(&v.name))
        .collect();
    ui.set_available_views(view_names.to_slint_model());
    ui.set_active_view_name(slint::SharedString::from(
        state.active_view_name.as_deref().unwrap_or(""),
    ));

    let is_filter_active = state.collection_filter_active;
    let mut collection_items: Vec<SlintCollectionFilterItem> = state
        .views_config
        .views
        .iter()
        .map(|v| SlintCollectionFilterItem {
            name: slint::SharedString::from(&v.name),
            file_count: v.files.len() as i32,
            is_selected: is_filter_active && state.selected_collections.contains(&v.name),
            is_temporary: false,
        })
        .collect();

    if let Some(ref tc) = state.temp_collection {
        collection_items.push(SlintCollectionFilterItem {
            name: slint::SharedString::from(&tc.name),
            file_count: tc.files.len() as i32,
            is_selected: is_filter_active && state.selected_collections.contains(&tc.name),
            is_temporary: true,
        });
    }

    ui.set_has_temporary_collection(state.temp_collection.is_some());
    ui.set_collection_items(collection_items.to_slint_model());

    let summary = if !is_filter_active {
        "Alle".to_string()
    } else if state.selected_collections.is_empty() {
        "Keine".to_string()
    } else if state.selected_collections.len() == 1 {
        state
            .selected_collections
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "Alle".to_string())
    } else {
        format!("{} aktiv", state.selected_collections.len())
    };
    ui.set_collection_filter_summary(slint::SharedString::from(summary));

    let external_apps: Vec<SlintExternalAppItem> = state
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
    ui.set_external_apps(external_apps.to_slint_model());

    let marker_choices: Vec<KitColorItem> = state
        .app_config
        .brushes
        .iter()
        .filter(|b| b.category == BrushCategory::Textmarker)
        .map(|b| {
            let label = match b.key.as_str() {
                "MarkerGelb" => "Gelb",
                "MarkerGruen" => "Grün",
                "MarkerBlau" => "Blau",
                "MarkerRot" => "Rot",
                "MarkerOrange" => "Orange",
                "MarkerLila" => "Lila",
                other => other.strip_prefix("Marker").unwrap_or(other),
            };
            KitColorItem {
                key: slint::SharedString::from(label),
                hex_value: slint::SharedString::from(&b.hex_value),
                color: cached_color(&b.hex_value, 255),
            }
        })
        .collect();
    if !marker_choices.is_empty() {
        ui.set_marker_choices(ModelRc::new(VecModel::from(marker_choices)));
    }

    // 1. Sync tabs and view mode (filtered by selected collections)
    let visible_tables: Vec<(&String, &TableData)> = state
        .tab_order
        .iter()
        .filter_map(|id| state.tables.get(id).map(|tbl| (id, tbl)))
        .filter(|(_, tbl)| state.is_table_in_selected_collections(tbl))
        .collect();

    let csv_count = visible_tables.iter().filter(|(_, t)| !t.is_ods).count();
    let ods_count = visible_tables.iter().filter(|(_, t)| t.is_ods).count();
    ui.set_csv_tab_count(csv_count as i32);
    ui.set_ods_tab_count(ods_count as i32);
    ui.set_active_view_mode(state.active_view_mode as i32);
    let mode_label = match state.active_view_mode {
        1 => format!("CSV-Ebene ({} Tabellen)", csv_count),
        2 => format!("ODS-Ebene ({} Blätter)", ods_count),
        _ => format!("Alle Ebenen ({} Tabellen)", csv_count + ods_count),
    };
    ui.set_active_mode_label(slint::SharedString::from(mode_label));

    // Determine effective active tab: if current active tab is not visible, select first visible
    let is_active_visible = state
        .active_tab_id
        .as_ref()
        .map(|act| visible_tables.iter().any(|(id, _)| *id == act))
        .unwrap_or(false);

    let effective_active_id = if is_active_visible {
        state.active_tab_id.as_deref()
    } else {
        visible_tables.first().map(|(id, _)| id.as_str())
    };

    let is_ods_active = effective_active_id
        .and_then(|id| state.tables.get(id))
        .map(|t| t.is_ods)
        .unwrap_or(false);
    ui.set_active_table_is_ods(is_ods_active);
    let active_name = effective_active_id
        .and_then(|id| state.tables.get(id))
        .map(|t| t.display_name.as_str())
        .unwrap_or("");
    ui.set_active_table_name(slint::SharedString::from(active_name));

    let mut tab_items = Vec::new();
    let mut csv_tab_items = Vec::new();
    let mut ods_tab_items = Vec::new();

    let total_csv = visible_tables.iter().filter(|(_, t)| !t.is_ods).count() as i32;
    let total_ods = visible_tables.iter().filter(|(_, t)| t.is_ods).count() as i32;
    let mut cur_csv_idx = 0;
    let mut cur_ods_idx = 0;

    for (id, tbl) in &visible_tables {
        let is_tab_active = effective_active_id == Some(id.as_str());
        let error_count = tbl
            .rule_highlights
            .iter()
            .filter(|h| h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error)
            .count();

        let (tab_index, total_tabs) = if tbl.is_ods {
            let idx = cur_ods_idx;
            cur_ods_idx += 1;
            (idx, total_ods)
        } else {
            let idx = cur_csv_idx;
            cur_csv_idx += 1;
            (idx, total_csv)
        };

        let item = TabItem {
            id: slint::SharedString::from(*id),
            title: slint::SharedString::from(&tbl.display_name),
            is_dirty: tbl.is_dirty,
            error_count: error_count as i32,
            is_active: is_tab_active,
            is_ods: tbl.is_ods,
            file_type: slint::SharedString::from(if tbl.is_ods { "ODS" } else { "CSV" }),
            tab_index,
            total_tabs,
        };

        if tbl.is_ods {
            ods_tab_items.push(item.clone());
        } else {
            csv_tab_items.push(item.clone());
        }

        let matches_mode = match state.active_view_mode {
            1 => !tbl.is_ods,
            2 => tbl.is_ods,
            _ => true,
        };
        if matches_mode {
            tab_items.push(item);
        }
    }

    let chunk_tabs = |tabs: &[TabItem]| -> Vec<TabRowItem> {
        let mut rows = Vec::new();
        let mut cur_row = Vec::new();
        let mut cur_chars = 0;
        for tab in tabs {
            let tab_chars = tab.title.chars().count() + 16;
            if !cur_row.is_empty() && (cur_row.len() >= 6 || (cur_chars + tab_chars > 200 && cur_row.len() >= 4)) {
                rows.push(TabRowItem { tabs: cur_row.to_slint_model() });
                cur_row = Vec::new();
                cur_chars = 0;
            }
            cur_chars += tab_chars;
            cur_row.push(tab.clone());
        }
        if !cur_row.is_empty() {
            rows.push(TabRowItem { tabs: cur_row.to_slint_model() });
        }
        rows
    };

    let csv_rows = chunk_tabs(&csv_tab_items);
    let ods_rows = chunk_tabs(&ods_tab_items);

    // Build collection groups (Strict separation: Each collection in its own visual container)
    let mut collection_map: Vec<(String, bool, Vec<TabItem>)> = Vec::new();
    let active_collection_names: Vec<String> = if state.collection_filter_active && !state.selected_collections.is_empty() {
        state.selected_collections.iter().cloned().collect()
    } else {
        let mut names: Vec<String> = state.views_config.views.iter().map(|v| v.name.clone()).collect();
        if state.temp_collection.is_some() {
            names.push("Temporäre Kollektion".to_string());
        }
        names
    };

    for coll_name in &active_collection_names {
        let is_temp = coll_name == "Temporäre Kollektion";
        let view_def = if is_temp {
            state.temp_collection.as_ref()
        } else {
            state.views_config.views.iter().find(|v| v.name == *coll_name)
        };

        if let Some(v) = view_def {
            let mut tabs_in_coll = Vec::new();
            for (id, tbl) in &visible_tables {
                if state.is_table_in_view(tbl, v) {
                    let matches_mode = match state.active_view_mode {
                        1 => !tbl.is_ods,
                        2 => tbl.is_ods,
                        _ => true,
                    };
                    if matches_mode {
                        let is_tab_active = effective_active_id == Some(id.as_str());
                        let error_count = tbl
                            .rule_highlights
                            .iter()
                            .filter(|h| h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error)
                            .count();
                        tabs_in_coll.push(TabItem {
                            id: slint::SharedString::from(*id),
                            title: slint::SharedString::from(&tbl.display_name),
                            is_dirty: tbl.is_dirty,
                            error_count: error_count as i32,
                            is_active: is_tab_active,
                            is_ods: tbl.is_ods,
                            file_type: slint::SharedString::from(if tbl.is_ods { "ODS" } else { "CSV" }),
                            tab_index: tabs_in_coll.len() as i32,
                            total_tabs: 0,
                        });
                    }
                }
            }
            if !tabs_in_coll.is_empty() {
                let total = tabs_in_coll.len() as i32;
                for t in &mut tabs_in_coll {
                    t.total_tabs = total;
                }
                collection_map.push((coll_name.clone(), is_temp, tabs_in_coll));
            }
        }
    }

    // Check if any visible tables are unassigned to any collection group
    let assigned_ids: std::collections::HashSet<String> = collection_map
        .iter()
        .flat_map(|(_, _, tabs)| tabs.iter().map(|t| t.id.to_string()))
        .collect();

    let mut collection_group_items: Vec<SlintCollectionTabGroup> = collection_map
        .into_iter()
        .map(|(name, is_temp, tabs)| {
            let count = tabs.len() as i32;
            let rows = chunk_tabs(&tabs);
            SlintCollectionTabGroup {
                name: slint::SharedString::from(name),
                is_temporary: is_temp,
                tab_count: count,
                tabs: tabs.to_slint_model(),
                tab_rows: rows.to_slint_model(),
            }
        })
        .collect();

    let mut unassigned_tabs = Vec::new();
    for (id, tbl) in &visible_tables {
        if !assigned_ids.contains(*id) {
            let matches_mode = match state.active_view_mode {
                1 => !tbl.is_ods,
                2 => tbl.is_ods,
                _ => true,
            };
            if matches_mode {
                let is_tab_active = effective_active_id == Some(id.as_str());
                let error_count = tbl
                    .rule_highlights
                    .iter()
                    .filter(|h| h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error)
                    .count();
                unassigned_tabs.push(TabItem {
                    id: slint::SharedString::from(*id),
                    title: slint::SharedString::from(&tbl.display_name),
                    is_dirty: tbl.is_dirty,
                    error_count: error_count as i32,
                    is_active: is_tab_active,
                    is_ods: tbl.is_ods,
                    file_type: slint::SharedString::from(if tbl.is_ods { "ODS" } else { "CSV" }),
                    tab_index: unassigned_tabs.len() as i32,
                    total_tabs: 0,
                });
            }
        }
    }
    if !unassigned_tabs.is_empty() {
        let total = unassigned_tabs.len() as i32;
        for t in &mut unassigned_tabs {
            t.total_tabs = total;
        }
        let rows = chunk_tabs(&unassigned_tabs);
        collection_group_items.push(SlintCollectionTabGroup {
            name: slint::SharedString::from("Weitere"),
            is_temporary: false,
            tab_count: total,
            tabs: unassigned_tabs.to_slint_model(),
            tab_rows: rows.to_slint_model(),
        });
    }

    ui.set_collection_groups(collection_group_items.to_slint_model());

    ui.set_tabs(tab_items.to_slint_model());
    ui.set_csv_tabs(csv_tab_items.to_slint_model());
    ui.set_ods_tabs(ods_tab_items.to_slint_model());
    ui.set_csv_tab_rows(csv_rows.to_slint_model());
    ui.set_ods_tab_rows(ods_rows.to_slint_model());
    ui.set_tabs_multiline(state.app_config.tabs_multiline);
    ui.set_active_tab_id(slint::SharedString::from(effective_active_id.unwrap_or("")));

    // 2. Active table sync
    let active_tbl = effective_active_id.and_then(|id| state.tables.get(id));
    if let Some(tbl) = active_tbl {
        // Headers (respecting visibility from column_settings)
        let mut header_items = Vec::new();
        for (idx, h) in tbl.headers.iter().enumerate() {
            let concept = tbl
                .column_mappings
                .iter()
                .find(|(_, target_col)| target_col.eq_ignore_ascii_case(h))
                .map(|(concept, _)| concept.as_str())
                .unwrap_or("");

            let sort_dir = match tbl.sort_column {
                Some((c, true)) if c == idx => 1,
                Some((c, false)) if c == idx => 2,
                _ => 0,
            };

            let is_filtered = tbl.active_filters.get(&idx).map(|f| f.is_active()).unwrap_or(false);

            let col_name_lower = h.to_lowercase();
            let tbl_name_lower = tbl.display_name.to_lowercase();
            let col_key = format!("{}::{}", tbl_name_lower, col_name_lower);

            let col_width_from_view = state.active_view_name.as_ref().and_then(|vname| {
                state.views_config.views.iter().find(|v| &v.name == vname).and_then(|v| {
                    v.column_widths.get(&col_key).or_else(|| v.column_widths.get(h)).copied()
                })
            }).or_else(|| {
                state.temp_collection.as_ref().and_then(|tv| {
                    tv.column_widths.get(&col_key).or_else(|| tv.column_widths.get(h)).copied()
                })
            });

            let setting = tbl
                .column_settings
                .iter()
                .find(|s| s.column_name.eq_ignore_ascii_case(h))
                .or_else(|| tbl.column_settings.get(idx));
            let width = col_width_from_view
                .or_else(|| setting.map(|s| s.width))
                .unwrap_or(150) as f32;

            header_items.push(HeaderItem {
                id: idx as i32,
                name: slint::SharedString::from(h),
                concept: slint::SharedString::from(concept),
                sort_dir,
                is_filtered,
                width,
            });
        }
        state.headers_model.set_vec(header_items);
        ui.set_headers(state.headers_model.clone().into());

        // Compute visible row indices
        let mut visible_indices = tbl.visible_row_indices(state.only_marked);

        // Apply quick search query if set
        if !state.search_query.is_empty() {
            let query = state.search_query.to_lowercase();
            visible_indices.retain(|&r_idx| {
                if let Some(row) = tbl.rows.get(r_idx) {
                    row.iter().any(|cell| cell.to_lowercase().contains(&query))
                } else {
                    false
                }
            });
        }

        // Pre-index rule highlights by row for fast lookup
        let mut row_hl_map: HashMap<usize, Vec<&HighlightItem>> = HashMap::new();
        for h in &tbl.rule_highlights {
            row_hl_map.entry(h.row).or_default().push(h);
        }

        // Map visible rows to Slint RowItems
        let mut row_items = Vec::new();
        for (view_row_idx, &orig_row_idx) in visible_indices.iter().enumerate() {
            if let Some(row) = tbl.rows.get(orig_row_idx) {
                let is_marked = tbl.marked_rows.contains(&orig_row_idx);
                let row_marker = tbl.row_markers.get(&orig_row_idx);
                let row_highlights = row_hl_map.get(&orig_row_idx);

                // Highest priority row-level highlight (col == None)
                let best_row_hl = row_highlights.and_then(|list| {
                    list.iter()
                        .filter(|h| h.col.is_none() && h.brush_key.is_some())
                        .max_by_key(|h| h.priority)
                        .copied()
                });

                let row_rule_color = best_row_hl
                    .and_then(|h| h.brush_key.as_deref())
                    .map(|bk| resolve_brush_color(bk, state))
                    .unwrap_or_else(|| Color::from_argb_u8(0, 0, 0, 0));

                let mut cell_items = Vec::new();
                for (col_idx, cell_text) in row.iter().enumerate() {
                    let has_error = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Error
                        })
                    }).unwrap_or(false);

                    let has_warning = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Warning
                        })
                    }).unwrap_or(false);

                    let has_info = row_highlights.map(|list| {
                        list.iter().any(|h| {
                            (h.col.is_none() || h.col == Some(col_idx))
                                && h.severity == configurable_csv_analyzer::core::model::HighlightSeverity::Info
                        })
                    }).unwrap_or(false);

                    let best_cell_hl = row_highlights.and_then(|list| {
                        list.iter()
                            .filter(|h| h.col == Some(col_idx) && h.brush_key.is_some())
                            .max_by_key(|h| h.priority)
                            .copied()
                    });

                    let cell_marker = tbl.cell_markers.get(&(orig_row_idx, col_idx));
                    let is_cell_dirty = tbl.dirty_cells.contains(&(orig_row_idx, col_idx));
                    let bg_color = if let Some(m) = cell_marker {
                        parse_hex_color(m.hex())
                    } else if let Some(m) = row_marker {
                        parse_hex_color(m.hex())
                    } else if let Some(ch) = best_cell_hl {
                        if let Some(ref bk) = ch.brush_key {
                            resolve_brush_color(bk, state)
                        } else {
                            row_rule_color
                        }
                    } else if row_rule_color.alpha() > 0 {
                        row_rule_color
                    } else if is_cell_dirty {
                        resolve_brush_color("DirtyCellBrush", state)
                    } else {
                        row_rule_color
                    };

                    let has_custom_bg = bg_color.alpha() >= 128;
                    let fg_color = contrast_text(bg_color, Color::from_rgb_u8(0, 0, 0));

                    cell_items.push(CellItem {
                        text: slint::SharedString::from(cell_text),
                        bg_color,
                        fg_color,
                        has_custom_bg,
                        has_error,
                        has_warning,
                        has_info,
                    });
                }

                let row_bg = if let Some(m) = row_marker {
                    parse_hex_color(m.hex())
                } else {
                    row_rule_color
                };

                row_items.push(RowItem {
                    row_idx: view_row_idx as i32,
                    original_idx: orig_row_idx as i32,
                    is_marked,
                    bg_color: row_bg,
                    cells: cell_items.to_slint_model(),
                });
            }
        }
        ui.set_rows(row_items.to_slint_model());
        ui.set_total_row_count(tbl.rows.len() as i32);
        ui.set_marked_row_count(tbl.marked_rows.len() as i32);
        ui.set_only_marked_active(state.only_marked);

        // Sync Rule Summaries & Proposals for active table
        let summaries: Vec<RuleSummaryItem> = tbl
            .rule_summaries
            .iter()
            .map(|s| RuleSummaryItem {
                id: slint::SharedString::from(&s.rule_id),
                description: slint::SharedString::from(&s.description),
                hits: s.hit_count as i32,
                errors: s.error_count as i32,
                warnings: s.warning_count as i32,
            })
            .collect();
        ui.set_rule_summaries(ModelRc::new(VecModel::from(summaries)));

        let proposals: Vec<ProposalItem> = tbl
            .rule_proposals
            .iter()
            .enumerate()
            .map(|(i, p)| ProposalItem {
                source_table: slint::SharedString::from(&p.source_table),
                key_val: slint::SharedString::from(&p.key_value),
                preview: slint::SharedString::from(p.row_data.join(", ")),
                row_idx: i as i32,
            })
            .collect();
        ui.set_proposals(ModelRc::new(VecModel::from(proposals)));
    } else {
        state.headers_model.set_vec(Vec::new());
        ui.set_headers(state.headers_model.clone().into());
        ui.set_rows(ModelRc::new(VecModel::default()));
        ui.set_total_row_count(0);
        ui.set_marked_row_count(0);
        ui.set_rule_summaries(ModelRc::new(VecModel::default()));
        ui.set_proposals(ModelRc::new(VecModel::default()));
    }

    // 3. Sync Logs
    let log_entries = get_global_logger().get_entries(None);
    let log_items: Vec<LogViewItem> = log_entries
        .into_iter()
        .rev()
        .take(100)
        .map(|e| LogViewItem {
            timestamp: slint::SharedString::from(&e.timestamp),
            level: slint::SharedString::from(e.level.as_str()),
            message: slint::SharedString::from(&format!("{}: {}", e.target, e.message)),
        })
        .collect();
    ui.set_logs(ModelRc::new(VecModel::from(log_items)));
}
