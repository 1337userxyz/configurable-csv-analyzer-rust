use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};

use configurable_csv_analyzer::core::io::csv::load_csv;
use configurable_csv_analyzer::core::io::info::{
    apply_info_to_table, build_info_from_table, default_info_path_for_table,
    load_info_file, save_info_file,
};
use configurable_csv_analyzer::core::logging::{log_info, log_warn};
use configurable_csv_analyzer::core::model::{
    ColumnSetting, GroupConsistencyCheck, InfoFile, KeyMapping, LinkStep, NumericRangeCheck,
    RuleDefinition, TableData, ValidationSeverity, VirtualColumnMapping,
};
use configurable_csv_analyzer::core::rules::RuleEngine;
use configurable_csv_analyzer::core::validation::TableConfigValidator;

use crate::{
    AppState, InfoFileEditorWindow, KitColorItem, KitComboItem, MainWindow,
    SlintApplyRuleItem, SlintCalcRuleItem, SlintColumnDisplayItem,
    SlintComparisonRuleItem, SlintGroupValidationItem, SlintHighlightRuleItem,
    SlintJoinRuleItem, SlintKeyMappingItem, SlintLinkStepItem, SlintMappingItem,
    SlintVirtualColumnItem,
};
use configurable_csv_analyzer::core::color_cache::cached_color;

#[derive(Default)]
pub struct InfoEditorSession {
    pub target_table_id: Option<String>,
    pub file_path: Option<PathBuf>,
    pub working_table: Option<TableData>,
    pub working_info: InfoFile,
    pub is_dirty: bool,
}

fn get_table_concepts_and_headers(tbl: &TableData) -> Vec<String> {
    let mut list = Vec::new();
    for k in tbl.column_mappings.keys() {
        if !list.iter().any(|existing: &String| existing.eq_ignore_ascii_case(k)) {
            list.push(k.clone());
        }
    }
    for (idx, h) in tbl.headers.iter().enumerate() {
        let is_virt = tbl.is_virtual_col.get(idx).copied().unwrap_or(false);
        if !is_virt && !list.iter().any(|existing: &String| existing.eq_ignore_ascii_case(h)) {
            list.push(h.clone());
        }
    }
    if list.is_empty() {
        for h in &tbl.headers {
            if !list.iter().any(|existing: &String| existing.eq_ignore_ascii_case(h)) {
                list.push(h.clone());
            }
        }
    }
    if list.is_empty() {
        list.push("Key".to_string());
    }
    list
}

fn resolve_col_index_in_session(sess: &InfoEditorSession, col_concept: &str) -> Option<usize> {
    let tbl = sess.working_table.as_ref()?;
    if let Some((_, mapped_header)) = sess
        .working_info
        .column_mappings
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(col_concept))
    {
        if let Some(pos) = tbl
            .headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(mapped_header))
        {
            return Some(pos);
        }
    }
    tbl.find_col_index(col_concept)
}

pub fn update_hl_column_value_counts(
    win: &InfoFileEditorWindow,
    sess: &InfoEditorSession,
    col_concept: &str,
) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    if let Some(col_idx) = resolve_col_index_in_session(sess, col_concept) {
        if let Some(tbl) = sess.working_table.as_ref() {
            for row in &tbl.rows {
                let val = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                *counts.entry(val.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut pairs: Vec<(String, usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let items: Vec<KitComboItem> = pairs
        .into_iter()
        .take(500)
        .map(|(val, cnt)| {
            let label = if val.is_empty() {
                "(Leer)".to_string()
            } else {
                val.clone()
            };
            KitComboItem {
                value: slint::SharedString::from(val),
                label: slint::SharedString::from(label),
                badge: slint::SharedString::from(format!("{}x", cnt)),
            }
        })
        .collect();

    win.set_hl_value_counts(ModelRc::new(VecModel::from(items)));
}

pub fn open_info_editor_for_table_or_path(
    window: &InfoFileEditorWindow,
    file_path_override: Option<&Path>,
    state: &Rc<RefCell<AppState>>,
    _main_window_weak: Weak<MainWindow>,
) {
    let mut session = InfoEditorSession::default();
    let st = state.borrow();

    if let Some(path) = file_path_override {
        session.file_path = Some(path.to_path_buf());
        let info_path = default_info_path_for_table(path);
        let info = load_info_file(&info_path).unwrap_or_else(|_| {
            let mut inf = InfoFile::default();
            inf.csv_settings.file_name = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            inf
        });
        session.working_info = info;

        // Try load headers from CSV if exists
        let mut tbl = TableData::new(
            "temp".into(),
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );
        tbl.file_path = Some(path.to_path_buf());
        if path.exists() {
            if let Ok(loaded) = load_csv(path, None, None, true) {
                tbl.headers = loaded.headers;
                tbl.rows = loaded.rows;
            }
        }
        apply_info_to_table(&mut tbl, &session.working_info);
        session.working_table = Some(tbl);

        let fname = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let fpath = path.to_string_lossy().to_string();
        window.set_table_title(slint::SharedString::from(fname));
        window.set_file_path_display(slint::SharedString::from(fpath));
    } else if let Some(active_tbl) = st.active_table() {
        session.target_table_id = Some(active_tbl.id.clone());
        session.file_path = active_tbl.file_path.clone();
        session.working_table = Some(active_tbl.clone());
        session.working_info = build_info_from_table(active_tbl);

        window.set_table_title(slint::SharedString::from(&active_tbl.display_name));
        let path_str = active_tbl
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Neues Dokument".into());
        window.set_file_path_display(slint::SharedString::from(path_str));
    } else {
        // Fallback empty
        let tbl = TableData::new("new".into(), "Unbenannt".into());
        session.working_info = build_info_from_table(&tbl);
        session.working_table = Some(tbl);
        window.set_table_title(slint::SharedString::from("Unbenannt"));
        window.set_file_path_display(slint::SharedString::from("Keine Datei verknüpft"));
    }

    // Available Tables for joins & comparisons
    let tbl_names: Vec<slint::SharedString> = st
        .tables
        .values()
        .map(|t| slint::SharedString::from(&t.display_name))
        .collect();
    window.set_available_tables(ModelRc::new(VecModel::from(tbl_names)));

    // Available Brushes & Color choices
    let default_brushes = configurable_csv_analyzer::core::model::AppConfig::default_brushes();
    let brushes = if !st.app_config.brushes.is_empty() {
        &st.app_config.brushes
    } else {
        &default_brushes
    };

    let mut brush_names: Vec<slint::SharedString> = vec![slint::SharedString::from("Keine")];
    brush_names.extend(brushes.iter().map(|b| slint::SharedString::from(&b.key)));
    window.set_available_brushes(ModelRc::new(VecModel::from(brush_names)));

    let color_items: Vec<KitColorItem> = brushes
        .iter()
        .map(|b| KitColorItem {
            key: slint::SharedString::from(&b.key),
            hex_value: slint::SharedString::from(&b.hex_value),
            color: cached_color(&b.hex_value, 255),
        })
        .collect();

    let mut join_color_items: Vec<KitColorItem> = vec![KitColorItem {
        key: slint::SharedString::from("Keine"),
        hex_value: slint::SharedString::from("Transparent"),
        color: cached_color("Keine", 0),
    }];
    join_color_items.extend(color_items.clone());

    window.set_color_choices(ModelRc::new(VecModel::from(color_items)));
    window.set_join_color_choices(ModelRc::new(VecModel::from(join_color_items)));

    // Reset selection indices and auto-select first rule of each type if present
    window.set_selected_calc_rule_idx(-1);
    window.set_selected_join_idx(-1);
    window.set_selected_comp_idx(-1);
    window.set_selected_group_idx(-1);
    window.set_selected_hl_idx(-1);
    window.set_selected_apply_idx(-1);

    refresh_all_tabs(window, &session);

    // Auto-select first Highlight / Join rule if present so editor is populated
    let first_join_idx = session
        .working_info
        .rules
        .iter()
        .position(|r| matches!(r, RuleDefinition::VirtualRowJoin { .. }));
    if let Some(j_idx) = first_join_idx {
        window.set_selected_join_idx(j_idx as i32);
        populate_selected_join_rule(window, &mut session, &st, j_idx);
    }

    let first_hl_idx = session
        .working_info
        .rules
        .iter()
        .position(|r| matches!(r, RuleDefinition::Highlight { .. }));
    if let Some(h_idx) = first_hl_idx {
        window.set_selected_hl_idx(h_idx as i32);
        populate_selected_highlight_rule(window, &session, h_idx);
    }

    validate_and_update_banner(window, &session, &st);
    drop(st);

    // Store session into window / global session
    get_active_session().replace(session);
}

// Global active session for InfoEditorWindow
thread_local! {
    static ACTIVE_INFO_SESSION: RefCell<InfoEditorSession> = RefCell::new(InfoEditorSession::default());
}

fn get_active_session() -> &'static std::thread::LocalKey<RefCell<InfoEditorSession>> {
    &ACTIVE_INFO_SESSION
}

fn populate_selected_highlight_rule(
    win: &InfoFileEditorWindow,
    sess: &InfoEditorSession,
    r_idx: usize,
) {
    if let Some(RuleDefinition::Highlight {
        id,
        column_concept,
        condition,
        value_to_match,
        invert_condition,
        brush_key,
        target,
        priority,
        is_active,
        ..
    }) = sess.working_info.rules.get(r_idx)
    {
        win.set_edit_hl_name(slint::SharedString::from(id));
        win.set_edit_hl_column(slint::SharedString::from(column_concept));
        win.set_edit_hl_condition(slint::SharedString::from(condition));
        win.set_edit_hl_val(slint::SharedString::from(value_to_match));
        win.set_edit_hl_invert(*invert_condition);
        win.set_edit_hl_brush(slint::SharedString::from(brush_key));
        win.set_edit_hl_color(cached_color(brush_key, 255));
        win.set_edit_hl_target(slint::SharedString::from(target));
        win.set_edit_hl_priority(*priority as i32);
        win.set_edit_hl_active(*is_active);
        update_hl_column_value_counts(win, sess, column_concept);
    }
}

pub fn refresh_all_tabs(window: &InfoFileEditorWindow, session: &InfoEditorSession) {
    let info = &session.working_info;
    let table = session.working_table.as_ref();

    // Tab 0: Allgemein
    let dirty_key = info.dirty_cell_brush_key.as_deref().unwrap_or("DirtyCellBrush");
    window.set_general_delimiter(slint::SharedString::from(&info.csv_settings.delimiter));
    window.set_general_encoding(slint::SharedString::from(&info.csv_settings.encoding));
    window.set_general_has_header(info.csv_settings.has_header);
    window.set_general_dirty_brush(slint::SharedString::from(dirty_key));
    window.set_general_dirty_color(cached_color(dirty_key, 255));
    window.set_general_storage_override(slint::SharedString::from(
        info.storage_mode_override.as_deref().unwrap_or("Standard"),
    ));

    // Tab 1: Spalten
    let mut cols = Vec::new();
    if let Some(t) = table {
        for (idx, h) in t.headers.iter().enumerate() {
            let setting = info
                .column_settings
                .iter()
                .find(|c| c.column_name.eq_ignore_ascii_case(h));
            let is_vis = setting.map(|c| c.is_visible).unwrap_or(true);
            let w = setting.map(|c| c.width).unwrap_or(150);
            let order = setting.map(|c| c.display_order).unwrap_or(idx as u32);
            cols.push(SlintColumnDisplayItem {
                id: idx as i32,
                column_name: slint::SharedString::from(h),
                is_visible: is_vis,
                width: w as i32,
                display_index: order as i32,
            });
        }
    }
    window.set_columns_list(ModelRc::new(VecModel::from(cols)));

    // Available Headers & Concepts (include both mapped concepts AND all table column headers)
    if let Some(t) = table {
        let headers: Vec<slint::SharedString> = t
            .headers
            .iter()
            .map(|h| slint::SharedString::from(h.as_str()))
            .collect();
        window.set_available_headers(ModelRc::new(VecModel::from(headers)));

        let mut concepts_vec: Vec<String> = Vec::new();
        for k in info.column_mappings.keys() {
            if !concepts_vec.iter().any(|c| c.eq_ignore_ascii_case(k)) {
                concepts_vec.push(k.clone());
            }
        }
        for (idx, h) in t.headers.iter().enumerate() {
            let is_virt = t.is_virtual_col.get(idx).copied().unwrap_or(false);
            if !is_virt && !concepts_vec.iter().any(|c| c.eq_ignore_ascii_case(h)) {
                concepts_vec.push(h.clone());
            }
        }
        if concepts_vec.is_empty() {
            for h in &t.headers {
                if !concepts_vec.iter().any(|c| c.eq_ignore_ascii_case(h)) {
                    concepts_vec.push(h.clone());
                }
            }
        }
        let concepts: Vec<slint::SharedString> = concepts_vec
            .into_iter()
            .map(slint::SharedString::from)
            .collect();
        window.set_available_concepts(ModelRc::new(VecModel::from(concepts)));
    }

    // Tab 2: Mappings
    let mut mappings = Vec::new();
    for (m_idx, (k, v)) in info.column_mappings.iter().enumerate() {
        let valid = table
            .map(|t| t.headers.iter().any(|h| h.eq_ignore_ascii_case(v)))
            .unwrap_or(true);
        mappings.push(SlintMappingItem {
            id: m_idx as i32,
            concept_name: slint::SharedString::from(k),
            csv_column_name: slint::SharedString::from(v),
            is_valid_header: valid,
        });
    }
    window.set_mappings_list(ModelRc::new(VecModel::from(mappings)));

    // Tab 3..8: Rules
    let mut calc_items = Vec::new();
    let mut join_items = Vec::new();
    let mut comp_items = Vec::new();
    let mut grp_items = Vec::new();
    let mut hl_items = Vec::new();
    let mut apply_items = Vec::new();

    for (r_idx, rule) in info.rules.iter().enumerate() {
        match rule {
            RuleDefinition::Calculation {
                id,
                description: _,
                target_column,
                formula,
                is_active,
                is_highlighted,
                ..
            } => {
                calc_items.push(SlintCalcRuleItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    target_column: slint::SharedString::from(target_column),
                    formula: slint::SharedString::from(formula),
                    is_active: *is_active,
                    is_highlighted: *is_highlighted,
                });
            }
            RuleDefinition::VirtualRowJoin {
                id,
                target_table,
                key_concept,
                target_key_concept,
                import_concepts,
                link_steps,
                virtual_columns,
                is_active,
                ..
            } => {
                let preview = if !link_steps.is_empty() {
                    let steps_summary = link_steps
                        .iter()
                        .map(|s| s.target_table_key.as_str())
                        .collect::<Vec<_>>()
                        .join(" ➔ ");
                    format!("{} ({} Spalten)", steps_summary, virtual_columns.len())
                } else if !target_table.is_empty() {
                    format!("{} ➔ {}", target_table, import_concepts.join(", "))
                } else {
                    "Neu / Nicht konfiguriert".to_string()
                };
                let imp = import_concepts.first().map(|s| s.as_str()).unwrap_or("");
                join_items.push(SlintJoinRuleItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    target_table: slint::SharedString::from(target_table),
                    host_concept: slint::SharedString::from(key_concept),
                    remote_concept: slint::SharedString::from(target_key_concept),
                    imported_concept: slint::SharedString::from(imp),
                    match_preview_text: slint::SharedString::from(preview),
                    is_active: *is_active,
                });
            }
            RuleDefinition::CrossTableComparison {
                id,
                target_table,
                key_concept,
                target_key_concept,
                comparison_type,
                mismatch_brush_key,
                show_proposal_dialog,
                is_active,
                ..
            } => {
                comp_items.push(SlintComparisonRuleItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    target_table: slint::SharedString::from(target_table),
                    host_concept: slint::SharedString::from(key_concept),
                    remote_concept: slint::SharedString::from(target_key_concept),
                    comparison_type: slint::SharedString::from(
                        comparison_type.as_deref().unwrap_or("MissingInRemote"),
                    ),
                    mismatch_brush: slint::SharedString::from(
                        mismatch_brush_key
                            .as_deref()
                            .unwrap_or("CrossTableMismatchBrush"),
                    ),
                    mismatch_target: slint::SharedString::from("Row"),
                    show_proposal_dialog: show_proposal_dialog.unwrap_or(true),
                    is_active: *is_active,
                });
            }
            RuleDefinition::GroupValidation {
                id,
                group_by_concept,
                validation_type,
                unique_concept,
                numeric_range,
                mismatch_brush_key,
                is_active,
                ..
            } => {
                let p_concept = unique_concept.as_deref().unwrap_or("");
                let min_str = numeric_range
                    .as_ref()
                    .and_then(|r| r.min)
                    .map(|m| m.to_string())
                    .unwrap_or_default();
                let max_str = numeric_range
                    .as_ref()
                    .and_then(|r| r.max)
                    .map(|m| m.to_string())
                    .unwrap_or_default();

                grp_items.push(SlintGroupValidationItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    group_by_concept: slint::SharedString::from(group_by_concept),
                    validation_type: slint::SharedString::from(
                        validation_type.as_deref().unwrap_or("isUnique"),
                    ),
                    param_concept: slint::SharedString::from(p_concept),
                    range_min: slint::SharedString::from(min_str),
                    range_max: slint::SharedString::from(max_str),
                    mismatch_brush: slint::SharedString::from(
                        mismatch_brush_key
                            .as_deref()
                            .unwrap_or("UniquenessViolationBrush"),
                    ),
                    is_active: *is_active,
                });
            }
            RuleDefinition::Highlight {
                id,
                column_concept,
                condition,
                value_to_match,
                invert_condition,
                brush_key,
                target,
                priority,
                is_active,
                ..
            } => {
                hl_items.push(SlintHighlightRuleItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    column_concept: slint::SharedString::from(column_concept),
                    condition: slint::SharedString::from(condition),
                    value_to_match: slint::SharedString::from(value_to_match),
                    invert_condition: *invert_condition,
                    brush_key: slint::SharedString::from(brush_key),
                    target: slint::SharedString::from(target),
                    priority: *priority as i32,
                    is_active: *is_active,
                });
            }
            RuleDefinition::Apply {
                id,
                source_column_concepts,
                target_table,
                requires_confirmation,
                execute_automatically,
                is_active,
                ..
            } => {
                let src = source_column_concepts.first().map(|s| s.as_str()).unwrap_or("");
                apply_items.push(SlintApplyRuleItem {
                    id: r_idx as i32,
                    rule_name: slint::SharedString::from(id),
                    source_concept: slint::SharedString::from(src),
                    target_table: slint::SharedString::from(target_table),
                    target_concept: slint::SharedString::from(src),
                    requires_confirmation: *requires_confirmation,
                    execute_automatically: *execute_automatically,
                    is_active: *is_active,
                });
            }
            _ => {}
        }
    }

    window.set_calc_rules(ModelRc::new(VecModel::from(calc_items)));
    window.set_join_rules(ModelRc::new(VecModel::from(join_items)));
    window.set_comparison_rules(ModelRc::new(VecModel::from(comp_items)));
    window.set_group_rules(ModelRc::new(VecModel::from(grp_items)));
    window.set_highlight_rules(ModelRc::new(VecModel::from(hl_items)));
    window.set_apply_rules(ModelRc::new(VecModel::from(apply_items)));
    window.set_is_dirty(session.is_dirty);
}

fn validate_and_update_banner(
    window: &InfoFileEditorWindow,
    session: &InfoEditorSession,
    app_state: &AppState,
) {
    let dummy_table = TableData::new("val".into(), "val".into());
    let table_ref = session.working_table.as_ref().unwrap_or(&dummy_table);

    let issues = TableConfigValidator::validate(
        &session.working_info,
        table_ref,
        &app_state.tables,
        &app_state.app_config,
    );

    let error_count = issues
        .iter()
        .filter(|i| i.severity == ValidationSeverity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|i| i.severity == ValidationSeverity::Warning)
        .count();

    window.set_has_validation_errors(error_count > 0);
    window.set_error_count(error_count as i32);
    window.set_warning_count(warning_count as i32);

    if error_count > 0 {
        let first_err = issues
            .iter()
            .find(|i| i.severity == ValidationSeverity::Error)
            .unwrap();
        window.set_validation_banner_message(slint::SharedString::from(format!(
            "❌ {} Validierungsfehler: {} (Tab {})",
            error_count,
            first_err.message,
            first_err.tab_index + 1
        )));
    } else if warning_count > 0 {
        let first_warn = issues
            .iter()
            .find(|i| i.severity == ValidationSeverity::Warning)
            .unwrap();
        window.set_validation_banner_message(slint::SharedString::from(format!(
            "⚠ {} Warnungen: {} (Tab {})",
            warning_count,
            first_warn.message,
            first_warn.tab_index + 1
        )));
    } else {
        window.set_validation_banner_message(slint::SharedString::from(
            "✓ Konfiguration ist valide und bereit zum Speichern.",
        ));
    }
}

fn populate_selected_join_rule(
    win: &InfoFileEditorWindow,
    sess: &mut InfoEditorSession,
    st: &AppState,
    r_idx: usize,
) {
    let host_table = match sess.working_table.as_ref() {
        Some(t) => t.clone(),
        None => return,
    };

    if let Some(RuleDefinition::VirtualRowJoin {
        id,
        action_group,
        description,
        is_active,
        is_highlighted,
        target_table,
        key_concept,
        target_key_concept,
        import_concepts,
        link_steps,
        virtual_columns,
        ..
    }) = sess.working_info.rules.get_mut(r_idx)
    {
        // Auto-migrate from legacy fields if link_steps is empty
        if link_steps.is_empty() {
            let default_tgt = if !target_table.is_empty() {
                target_table.clone()
            } else {
                st.tables
                    .values()
                    .map(|t| t.display_name.clone())
                    .next()
                    .unwrap_or_else(|| "$self".to_string())
            };
            let default_h = if !key_concept.is_empty() {
                key_concept.clone()
            } else {
                host_table
                    .headers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Key".to_string())
            };
            let default_r = if !target_key_concept.is_empty() {
                target_key_concept.clone()
            } else {
                default_h.clone()
            };
            *link_steps = vec![LinkStep {
                target_table_key: default_tgt.clone(),
                key_mappings: vec![KeyMapping::new(&default_h, &default_r)],
            }];
            if virtual_columns.is_empty() {
                let imp = import_concepts
                    .first()
                    .cloned()
                    .unwrap_or_else(|| default_r.clone());
                *virtual_columns = vec![VirtualColumnMapping {
                    remote_column_concept: imp.clone(),
                    virtual_column_header: VirtualColumnMapping::generate_default_header(
                        &imp,
                        &default_tgt,
                        &host_table.display_name,
                        "First",
                    ),
                    virtual_column_key: VirtualColumnMapping::generate_default_key(
                        &imp,
                        &default_tgt,
                        &host_table.display_name,
                        "First",
                    ),
                    multi_result_display: "First".to_string(),
                    brush_key: None,
                    priority: Some(5),
                }];
            }
        }

        win.set_edit_join_name(slint::SharedString::from(&*id));
        win.set_edit_join_action_group(slint::SharedString::from(
            action_group.as_deref().unwrap_or(""),
        ));
        win.set_edit_join_description(slint::SharedString::from(&*description));
        win.set_edit_join_is_active(*is_active);
        win.set_edit_join_is_highlighted(*is_highlighted);

        // Chain & Relationship analysis
        let chain_res = RuleEngine::analyze_chain(&host_table, link_steps, &st.tables);
        win.set_join_relationship_badge(slint::SharedString::from(&chain_res.relationship_badge));
        win.set_join_relationship_is_one_to_many(chain_res.is_one_to_many);

        // Steps
        let mut slint_steps: Vec<SlintLinkStepItem> = Vec::new();
        let mut prev_tbl: &TableData = &host_table;

        for (s_idx, step) in link_steps.iter().enumerate() {
            let host_concepts: Vec<slint::SharedString> = get_table_concepts_and_headers(prev_tbl)
                .into_iter()
                .map(slint::SharedString::from)
                .collect();

            let tgt_table =
                RuleEngine::resolve_table(&step.target_table_key, &host_table, &st.tables);
            let remote_concepts: Vec<slint::SharedString> = if let Some(target) = tgt_table {
                get_table_concepts_and_headers(target)
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect()
            } else {
                Vec::new()
            };

            let hit_stat = chain_res
                .step_stats
                .get(s_idx)
                .cloned()
                .unwrap_or_else(|| "Warte...".to_string());
            let src_name = if s_idx == 0 {
                host_table.display_name.clone()
            } else {
                link_steps[s_idx - 1].target_table_key.clone()
            };
            let step_title = format!(
                "Schritt {}: Verknüpfung von '{}' nach:",
                s_idx + 1,
                src_name
            );

            let mappings: Vec<SlintKeyMappingItem> = step
                .key_mappings
                .iter()
                .enumerate()
                .map(|(m_idx, m)| SlintKeyMappingItem {
                    step_idx: s_idx as i32,
                    mapping_idx: m_idx as i32,
                    host_concept: slint::SharedString::from(&m.host_concept),
                    remote_concept: slint::SharedString::from(&m.remote_concept),
                    manual_value: slint::SharedString::from(
                        m.manual_remote_value.as_deref().unwrap_or(""),
                    ),
                    is_case_sensitive: m.is_case_sensitive,
                })
                .collect();

            slint_steps.push(SlintLinkStepItem {
                step_idx: s_idx as i32,
                step_title: slint::SharedString::from(step_title),
                target_table: slint::SharedString::from(&step.target_table_key),
                hit_stat: slint::SharedString::from(hit_stat),
                available_host_concepts: ModelRc::new(VecModel::from(host_concepts)),
                available_remote_concepts: ModelRc::new(VecModel::from(remote_concepts)),
                mappings: ModelRc::new(VecModel::from(mappings)),
            });

            if let Some(target) = tgt_table {
                prev_tbl = target;
            }
        }
        win.set_join_steps(ModelRc::new(VecModel::from(slint_steps)));

        // Final target table concepts for Virtual Columns
        let final_remote_concepts: Vec<slint::SharedString> =
            if let Some(last_step) = link_steps.last() {
                if let Some(final_tbl) =
                    RuleEngine::resolve_table(&last_step.target_table_key, &host_table, &st.tables)
                {
                    get_table_concepts_and_headers(final_tbl)
                        .into_iter()
                        .map(slint::SharedString::from)
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
        win.set_available_final_remote_concepts(ModelRc::new(VecModel::from(
            final_remote_concepts,
        )));

        // Virtual Columns
        let slint_cols: Vec<SlintVirtualColumnItem> = virtual_columns
            .iter()
            .enumerate()
            .map(|(vc_idx, vc)| {
                let b_key = vc.brush_key.as_deref().unwrap_or("Keine");
                let b_col = if b_key == "Keine" || b_key.is_empty() {
                    cached_color("Keine", 0)
                } else {
                    cached_color(b_key, 255)
                };
                SlintVirtualColumnItem {
                    col_idx: vc_idx as i32,
                    remote_concept: slint::SharedString::from(&vc.remote_column_concept),
                    header: slint::SharedString::from(&vc.virtual_column_header),
                    binding_key: slint::SharedString::from(&vc.virtual_column_key),
                    display_mode: slint::SharedString::from(&vc.multi_result_display),
                    brush_key: slint::SharedString::from(b_key),
                    brush_color: b_col,
                }
            })
            .collect();
        win.set_join_virtual_columns(ModelRc::new(VecModel::from(slint_cols)));
    }
}

pub fn setup_info_editor_window(
    info_window: Rc<InfoFileEditorWindow>,
    main_window_weak: Weak<MainWindow>,
    state: Rc<RefCell<AppState>>,
) {
    // --- TAB 0: ALLGEMEIN ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_general_settings_changed(move |delim, enc, has_hdr, dirty_b, stor| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                sess.working_info.csv_settings.delimiter = delim.to_string();
                sess.working_info.csv_settings.encoding = enc.to_string();
                sess.working_info.csv_settings.has_header = has_hdr;
                sess.working_info.dirty_cell_brush_key = Some(dirty_b.to_string());
                sess.working_info.storage_mode_override = Some(stor.to_string());
                sess.is_dirty = true;
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    // --- TAB 1: SPALTEN ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_column_toggle_visibility(move |c_idx, visible| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                if let Some(ref mut t) = sess.working_table {
                    let idx = c_idx as usize;
                    if let Some(h) = t.headers.get(idx) {
                        let h_name = h.clone();
                        if let Some(setting) = sess
                            .working_info
                            .column_settings
                            .iter_mut()
                            .find(|c| c.column_name.eq_ignore_ascii_case(&h_name))
                        {
                            setting.is_visible = visible;
                        } else {
                            sess.working_info.column_settings.push(ColumnSetting {
                                column_name: h_name,
                                width: 150,
                                is_visible: visible,
                                display_order: idx as u32,
                            });
                        }
                        sess.is_dirty = true;
                    }
                }
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_column_show_all(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                for c in &mut sess.working_info.column_settings {
                    c.is_visible = true;
                }
                sess.is_dirty = true;
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_column_hide_all(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                for c in &mut sess.working_info.column_settings {
                    c.is_visible = false;
                }
                sess.is_dirty = true;
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_column_reset_default_widths(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                for c in &mut sess.working_info.column_settings {
                    c.width = 150;
                }
                sess.is_dirty = true;
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
            });
        });
    }

    // --- TAB 2: MAPPINGS ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_mapping_clicked(move |concept, csv_col| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                sess.working_info
                    .column_mappings
                    .insert(concept.to_string(), csv_col.to_string());
                let cloned_mappings = sess.working_info.column_mappings.clone();
                if let Some(ref mut wt) = sess.working_table {
                    wt.column_mappings = cloned_mappings;
                }
                sess.is_dirty = true;
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_mapping_clicked(move |m_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let key_opt = sess
                    .working_info
                    .column_mappings
                    .keys()
                    .nth(m_idx as usize)
                    .cloned();
                if let Some(key) = key_opt {
                    sess.working_info.column_mappings.remove(&key);
                    let cloned_mappings = sess.working_info.column_mappings.clone();
                    if let Some(ref mut wt) = sess.working_table {
                        wt.column_mappings = cloned_mappings;
                    }
                    sess.is_dirty = true;
                }
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_mapping_csv_col(move |m_idx, new_csv| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let key_opt = sess
                    .working_info
                    .column_mappings
                    .keys()
                    .nth(m_idx as usize)
                    .cloned();
                if let Some(key) = key_opt {
                    sess.working_info
                        .column_mappings
                        .insert(key, new_csv.to_string());
                    let cloned_mappings = sess.working_info.column_mappings.clone();
                    if let Some(ref mut wt) = sess.working_table {
                        wt.column_mappings = cloned_mappings;
                    }
                    sess.is_dirty = true;
                }
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    // --- TAB 3: BERECHNUNGEN ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_calc_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let id = format!("Calc_{}", sess.working_info.rules.len() + 1);
                let target_col = format!("Neu_{}", id);
                let formula_str = "1 + 1".to_string();
                sess.working_info.rules.push(RuleDefinition::Calculation {
                    id: id.clone(),
                    description: format!("Berechnung {}", id),
                    action_group: None,
                    is_active: true,
                    is_highlighted: true,
                    target_column: target_col.clone(),
                    virtual_column_key: None,
                    formula: formula_str.clone(),
                });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_calc_rule_idx(new_idx as i32);
                win.set_edit_calc_name(slint::SharedString::from(id));
                win.set_edit_calc_target(slint::SharedString::from(target_col));
                win.set_edit_calc_formula(slint::SharedString::from(formula_str));
                win.set_edit_calc_active(true);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_select_calc_rule(move |r_idx| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                if let Some(RuleDefinition::Calculation {
                    id,
                    target_column,
                    formula,
                    is_active,
                    ..
                }) = sess.working_info.rules.get(r_idx as usize)
                {
                    win.set_edit_calc_name(slint::SharedString::from(id));
                    win.set_edit_calc_target(slint::SharedString::from(target_column));
                    win.set_edit_calc_formula(slint::SharedString::from(formula));
                    win.set_edit_calc_active(*is_active);
                    win.set_calc_syntax_status(slint::SharedString::from("Syntax: Bereit"));
                }
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_calc_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_calc_rule_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_calc_rule_edit(move |r_idx, name, target, formula, active| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if let Some(RuleDefinition::Calculation {
                    id,
                    target_column,
                    formula: f_ref,
                    is_active,
                    ..
                }) = sess.working_info.rules.get_mut(idx)
                {
                    *id = name.to_string();
                    *target_column = target.to_string();
                    *f_ref = formula.to_string();
                    *is_active = active;
                    sess.is_dirty = true;
                }
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_insert_formula_token(move |token| {
            let mut cur = win.get_edit_calc_formula().to_string();
            if !cur.is_empty() && !cur.ends_with(' ') {
                cur.push(' ');
            }
            cur.push_str(&token);
            cur.push(' ');
            win.set_edit_calc_formula(slint::SharedString::from(cur));
        });
    }

    {
        let win = info_window.clone();
        info_window.on_test_formula_syntax(move |formula| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                let checker = |c: &str| {
                    sess.working_info.column_mappings.contains_key(c)
                        || sess
                            .working_table
                            .as_ref()
                            .map(|t| t.headers.iter().any(|h| h.eq_ignore_ascii_case(c)))
                            .unwrap_or(false)
                };
                let (ok, msg) = TableConfigValidator::validate_formula_syntax(&formula, &checker);
                if ok {
                    win.set_calc_syntax_status(slint::SharedString::from(
                        "✓ Formelsyntax ist gültig",
                    ));
                } else {
                    win.set_calc_syntax_status(slint::SharedString::from(format!(
                        "❌ Syntaxfehler: {}",
                        msg
                    )));
                }
            });
        });
    }

    // --- TAB 4: JOINS / VIRTUELLE SPALTEN ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_join_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let st = state.borrow();
                let host_table = sess
                    .working_table
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| TableData::new("new".into(), "Tabelle".into()));
                let id = format!("Join_{}", sess.working_info.rules.len() + 1);

                let default_target = st
                    .tables
                    .values()
                    .map(|t| t.display_name.clone())
                    .next()
                    .unwrap_or_else(|| "$self".to_string());
                let host_concepts = get_table_concepts_and_headers(&host_table);
                let first_host_c = host_concepts
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Key".to_string());

                let tgt_tbl =
                    RuleEngine::resolve_table(&default_target, &host_table, &st.tables);
                let remote_concepts = tgt_tbl
                    .map(get_table_concepts_and_headers)
                    .unwrap_or_default();
                let first_remote_c = remote_concepts
                    .first()
                    .cloned()
                    .unwrap_or_else(|| first_host_c.clone());

                let initial_step = LinkStep {
                    target_table_key: default_target.clone(),
                    key_mappings: vec![KeyMapping::new(&first_host_c, &first_remote_c)],
                };

                let initial_vcol = VirtualColumnMapping {
                    remote_column_concept: first_remote_c.clone(),
                    virtual_column_header: VirtualColumnMapping::generate_default_header(
                        &first_remote_c,
                        &default_target,
                        &host_table.display_name,
                        "First",
                    ),
                    virtual_column_key: VirtualColumnMapping::generate_default_key(
                        &first_remote_c,
                        &default_target,
                        &host_table.display_name,
                        "First",
                    ),
                    multi_result_display: "First".to_string(),
                    brush_key: None,
                    priority: Some(5),
                };

                sess.working_info.rules.push(RuleDefinition::VirtualRowJoin {
                    id: id.clone(),
                    description: format!("Join {}", id),
                    action_group: None,
                    is_active: true,
                    is_highlighted: true,
                    key_concept: first_host_c.clone(),
                    target_table: default_target.clone(),
                    target_key_concept: first_remote_c.clone(),
                    import_concepts: vec![first_remote_c.clone()],
                    injected_column_names: Vec::new(),
                    link_steps: vec![initial_step],
                    virtual_columns: vec![initial_vcol],
                });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_join_idx(new_idx as i32);
                populate_selected_join_rule(&win, &mut sess, &st, new_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_select_join_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let st = state.borrow();
                populate_selected_join_rule(&win, &mut sess, &st, r_idx as usize);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_join_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_join_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_join_rule_edit(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx();
                if r_idx < 0 {
                    return;
                }
                let r_idx = r_idx as usize;
                if let Some(RuleDefinition::VirtualRowJoin {
                    id,
                    action_group,
                    description,
                    is_active,
                    is_highlighted,
                    target_table,
                    key_concept,
                    target_key_concept,
                    import_concepts,
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    *id = win.get_edit_join_name().to_string();
                    let ag = win.get_edit_join_action_group().to_string();
                    *action_group = if ag.is_empty() { None } else { Some(ag) };
                    *description = win.get_edit_join_description().to_string();
                    *is_active = win.get_edit_join_is_active();
                    *is_highlighted = win.get_edit_join_is_highlighted();

                    // Sync legacy fields
                    if let Some(first_step) = link_steps.first() {
                        *target_table = first_step.target_table_key.clone();
                        if let Some(first_map) = first_step.key_mappings.first() {
                            *key_concept = first_map.host_concept.clone();
                            *target_key_concept = first_map.remote_concept.clone();
                        }
                    }
                    *import_concepts = virtual_columns
                        .iter()
                        .map(|vc| vc.remote_column_concept.clone())
                        .collect();
                    sess.is_dirty = true;
                }
                let st = state.borrow();
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_join_step(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let default_target = st
                        .tables
                        .values()
                        .map(|t| t.display_name.clone())
                        .next()
                        .unwrap_or_else(|| "$self".to_string());
                    let prev_target = link_steps
                        .last()
                        .map(|s| s.target_table_key.clone())
                        .unwrap_or_else(|| "$self".to_string());
                    let prev_tbl =
                        RuleEngine::resolve_table(&prev_target, &host_tbl, &st.tables)
                            .unwrap_or(&host_tbl);
                    let prev_concepts = get_table_concepts_and_headers(prev_tbl);
                    let first_host_c = prev_concepts
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Key".to_string());

                    let tgt_tbl =
                        RuleEngine::resolve_table(&default_target, &host_tbl, &st.tables)
                            .unwrap_or(&host_tbl);
                    let tgt_concepts = get_table_concepts_and_headers(tgt_tbl);
                    let first_remote_c = tgt_concepts
                        .first()
                        .cloned()
                        .unwrap_or_else(|| first_host_c.clone());

                    link_steps.push(LinkStep {
                        target_table_key: default_target.clone(),
                        key_mappings: vec![KeyMapping::new(&first_host_c, &first_remote_c)],
                    });

                    // Update virtual columns to target the new final table
                    for vc in virtual_columns.iter_mut() {
                        vc.remote_column_concept = first_remote_c.clone();
                        let host_name = &host_tbl.display_name;
                        vc.virtual_column_header = VirtualColumnMapping::generate_default_header(
                            &first_remote_c,
                            &default_target,
                            host_name,
                            &vc.multi_result_display,
                        );
                        vc.virtual_column_key = VirtualColumnMapping::generate_default_key(
                            &first_remote_c,
                            &default_target,
                            host_name,
                            &vc.multi_result_display,
                        );
                    }
                    sess.is_dirty = true;
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_join_step(move |step_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    if s_idx < link_steps.len() && link_steps.len() > 1 {
                        link_steps.remove(s_idx);
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_change_step_target_table(move |step_idx, new_tgt| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    if s_idx < link_steps.len() {
                        link_steps[s_idx].target_table_key = new_tgt.to_string();

                        if let Some(target) =
                            RuleEngine::resolve_table(&new_tgt, &host_tbl, &st.tables)
                        {
                            let concepts = get_table_concepts_and_headers(target);
                            if let Some(first_c) = concepts.first() {
                                for m in &mut link_steps[s_idx].key_mappings {
                                    if m.remote_concept.is_empty()
                                        || !concepts.contains(&m.remote_concept)
                                    {
                                        m.remote_concept = first_c.clone();
                                    }
                                }
                                if s_idx == link_steps.len() - 1 {
                                    for vc in virtual_columns.iter_mut() {
                                        if vc.remote_column_concept.is_empty()
                                            || !concepts.contains(&vc.remote_column_concept)
                                        {
                                            vc.remote_column_concept = first_c.clone();
                                            let host_name = &host_tbl.display_name;
                                            vc.virtual_column_header =
                                                VirtualColumnMapping::generate_default_header(
                                                    first_c,
                                                    &new_tgt,
                                                    host_name,
                                                    &vc.multi_result_display,
                                                );
                                            vc.virtual_column_key =
                                                VirtualColumnMapping::generate_default_key(
                                                    first_c,
                                                    &new_tgt,
                                                    host_name,
                                                    &vc.multi_result_display,
                                                );
                                        }
                                    }
                                }
                            }
                        }
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_key_mapping(move |step_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    if s_idx < link_steps.len() {
                        let prev_target = if s_idx == 0 {
                            host_tbl.display_name.clone()
                        } else {
                            link_steps[s_idx - 1].target_table_key.clone()
                        };
                        let prev_tbl =
                            RuleEngine::resolve_table(&prev_target, &host_tbl, &st.tables)
                                .unwrap_or(&host_tbl);
                        let host_c = get_table_concepts_and_headers(prev_tbl)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "Key".to_string());

                        let tgt_tbl = RuleEngine::resolve_table(
                            &link_steps[s_idx].target_table_key,
                            &host_tbl,
                            &st.tables,
                        )
                        .unwrap_or(&host_tbl);
                        let remote_c = get_table_concepts_and_headers(tgt_tbl)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| host_c.clone());

                        link_steps[s_idx]
                            .key_mappings
                            .push(KeyMapping::new(&host_c, &remote_c));
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_key_mapping(move |step_idx, map_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    let m_idx = map_idx as usize;
                    if s_idx < link_steps.len() && m_idx < link_steps[s_idx].key_mappings.len() {
                        link_steps[s_idx].key_mappings.remove(m_idx);
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_key_mapping_host(move |step_idx, map_idx, val| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    let m_idx = map_idx as usize;
                    if s_idx < link_steps.len() && m_idx < link_steps[s_idx].key_mappings.len() {
                        link_steps[s_idx].key_mappings[m_idx].host_concept = val.to_string();
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_key_mapping_remote(move |step_idx, map_idx, val| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    let m_idx = map_idx as usize;
                    if s_idx < link_steps.len() && m_idx < link_steps[s_idx].key_mappings.len() {
                        link_steps[s_idx].key_mappings[m_idx].remote_concept = val.to_string();
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_key_mapping_manual(move |step_idx, map_idx, val| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    let m_idx = map_idx as usize;
                    if s_idx < link_steps.len() && m_idx < link_steps[s_idx].key_mappings.len() {
                        let trimmed = val.trim();
                        link_steps[s_idx].key_mappings[m_idx].manual_remote_value =
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_string())
                            };
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_key_mapping_case(move |step_idx, map_idx, is_case| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin { link_steps, .. }) =
                    sess.working_info.rules.get_mut(r_idx)
                {
                    let s_idx = step_idx as usize;
                    let m_idx = map_idx as usize;
                    if s_idx < link_steps.len() && m_idx < link_steps[s_idx].key_mappings.len() {
                        link_steps[s_idx].key_mappings[m_idx].is_case_sensitive = is_case;
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_virtual_column(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let final_target = link_steps
                        .last()
                        .map(|s| s.target_table_key.clone())
                        .unwrap_or_else(|| "$self".to_string());
                    let final_tbl =
                        RuleEngine::resolve_table(&final_target, &host_tbl, &st.tables)
                            .unwrap_or(&host_tbl);
                    let concepts = get_table_concepts_and_headers(final_tbl);
                    let concept = concepts
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Wert".to_string());

                    let new_col = VirtualColumnMapping {
                        remote_column_concept: concept.clone(),
                        virtual_column_header: VirtualColumnMapping::generate_default_header(
                            &concept,
                            &final_target,
                            &host_tbl.display_name,
                            "First",
                        ),
                        virtual_column_key: VirtualColumnMapping::generate_default_key(
                            &concept,
                            &final_target,
                            &host_tbl.display_name,
                            "First",
                        ),
                        multi_result_display: "First".to_string(),
                        brush_key: None,
                        priority: Some(5),
                    };
                    virtual_columns.push(new_col);
                    sess.is_dirty = true;
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_virtual_column(move |col_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                if let Some(RuleDefinition::VirtualRowJoin {
                    virtual_columns, ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns.remove(c_idx);
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_virtual_column_concept(move |col_idx, concept| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns[c_idx].remote_column_concept = concept.to_string();
                        let final_target = link_steps
                            .last()
                            .map(|s| s.target_table_key.as_str())
                            .unwrap_or("$self");
                        virtual_columns[c_idx].virtual_column_header =
                            VirtualColumnMapping::generate_default_header(
                                &concept,
                                final_target,
                                &host_tbl.display_name,
                                &virtual_columns[c_idx].multi_result_display,
                            );
                        virtual_columns[c_idx].virtual_column_key =
                            VirtualColumnMapping::generate_default_key(
                                &concept,
                                final_target,
                                &host_tbl.display_name,
                                &virtual_columns[c_idx].multi_result_display,
                            );
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_virtual_column_header(move |col_idx, hdr| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                if let Some(RuleDefinition::VirtualRowJoin {
                    virtual_columns, ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns[c_idx].virtual_column_header = hdr.to_string();
                        sess.is_dirty = true;
                    }
                }
                let st = state.borrow();
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_virtual_column_key(move |col_idx, key| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                if let Some(RuleDefinition::VirtualRowJoin {
                    virtual_columns, ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns[c_idx].virtual_column_key = key.to_string();
                        sess.is_dirty = true;
                    }
                }
                let st = state.borrow();
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_virtual_column_mode(move |col_idx, mode| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let st = state.borrow();
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns[c_idx].multi_result_display = mode.to_string();
                        let final_target = link_steps
                            .last()
                            .map(|s| s.target_table_key.clone())
                            .unwrap_or_else(|| "$self".to_string());
                        let concept = virtual_columns[c_idx].remote_column_concept.clone();
                        virtual_columns[c_idx].virtual_column_header =
                            VirtualColumnMapping::generate_default_header(
                                &concept,
                                &final_target,
                                &host_tbl.display_name,
                                &mode,
                            );
                        virtual_columns[c_idx].virtual_column_key =
                            VirtualColumnMapping::generate_default_key(
                                &concept,
                                &final_target,
                                &host_tbl.display_name,
                                &mode,
                            );
                        sess.is_dirty = true;
                    }
                }
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_update_virtual_column_brush(move |col_idx, brush| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                if let Some(RuleDefinition::VirtualRowJoin {
                    virtual_columns, ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let c_idx = col_idx as usize;
                    if c_idx < virtual_columns.len() {
                        virtual_columns[c_idx].brush_key =
                            if brush == "Keine" || brush.is_empty() {
                                None
                            } else {
                                Some(brush.to_string())
                            };
                        sess.is_dirty = true;
                    }
                }
                let st = state.borrow();
                populate_selected_join_rule(&win, &mut sess, &st, r_idx);
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_generate_join_autotext(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let r_idx = win.get_selected_join_idx() as usize;
                let host_tbl = match sess.working_table.as_ref() {
                    Some(t) => t.clone(),
                    None => return,
                };
                if let Some(RuleDefinition::VirtualRowJoin {
                    id,
                    action_group,
                    description,
                    is_active,
                    link_steps,
                    virtual_columns,
                    ..
                }) = sess.working_info.rules.get_mut(r_idx)
                {
                    let autotext = VirtualColumnMapping::generate_autotext_description(
                        id,
                        *is_active,
                        action_group.as_deref(),
                        &host_tbl.display_name,
                        link_steps,
                        virtual_columns,
                    );
                    *description = autotext.clone();
                    win.set_edit_join_description(slint::SharedString::from(autotext));
                    sess.is_dirty = true;
                }
                let st = state.borrow();
                drop(sess);
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    // --- TAB 5: TABELLENVERGLEICHE ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_comparison_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let st = state.borrow();
                let id = format!("Compare_{}", sess.working_info.rules.len() + 1);
                let default_col = sess
                    .working_table
                    .as_ref()
                    .and_then(|t| get_table_concepts_and_headers(t).into_iter().next())
                    .unwrap_or_default();
                let default_tgt = st
                    .tables
                    .values()
                    .map(|t| t.display_name.clone())
                    .next()
                    .unwrap_or_else(|| "$self".to_string());

                sess.working_info
                    .rules
                    .push(RuleDefinition::CrossTableComparison {
                        id: id.clone(),
                        description: format!("Vergleich {}", id),
                        action_group: None,
                        is_active: true,
                        is_highlighted: true,
                        key_concept: default_col.clone(),
                        target_table: default_tgt.clone(),
                        target_key_concept: default_col.clone(),
                        compare_concepts: Vec::new(),
                        comparison_type: Some("MissingInRemote".into()),
                        mismatch_brush_key: Some("CrossTableMismatchBrush".into()),
                        mismatch_target: Some("Row".into()),
                        mismatch_priority: Some(80),
                        show_proposal_dialog: Some(true),
                    });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_comp_idx(new_idx as i32);
                win.set_edit_comp_name(slint::SharedString::from(id));
                win.set_edit_comp_target_table(slint::SharedString::from(default_tgt));
                win.set_edit_comp_host_concept(slint::SharedString::from(&default_col));
                win.set_edit_comp_remote_concept(slint::SharedString::from(&default_col));
                win.set_edit_comp_type(slint::SharedString::from("MissingInRemote"));
                win.set_edit_comp_brush(slint::SharedString::from("CrossTableMismatchBrush"));
                win.set_edit_comp_color(cached_color("CrossTableMismatchBrush", 255));
                win.set_edit_comp_proposal_dialog(true);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &st);
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_select_comparison_rule(move |r_idx| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                if let Some(RuleDefinition::CrossTableComparison {
                    id,
                    target_table,
                    key_concept,
                    target_key_concept,
                    comparison_type,
                    mismatch_brush_key,
                    show_proposal_dialog,
                    ..
                }) = sess.working_info.rules.get(r_idx as usize)
                {
                    let b_key = mismatch_brush_key
                        .as_deref()
                        .unwrap_or("CrossTableMismatchBrush");
                    win.set_edit_comp_name(slint::SharedString::from(id));
                    win.set_edit_comp_target_table(slint::SharedString::from(target_table));
                    win.set_edit_comp_host_concept(slint::SharedString::from(key_concept));
                    win.set_edit_comp_remote_concept(slint::SharedString::from(target_key_concept));
                    win.set_edit_comp_type(slint::SharedString::from(
                        comparison_type.as_deref().unwrap_or("MissingInRemote"),
                    ));
                    win.set_edit_comp_brush(slint::SharedString::from(b_key));
                    win.set_edit_comp_color(cached_color(b_key, 255));
                    win.set_edit_comp_proposal_dialog(show_proposal_dialog.unwrap_or(true));
                }
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_comparison_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_comp_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_comparison_rule_edit(
            move |r_idx, name, tgt_tbl, host_k, rem_k, c_type, brush, show_prop| {
                get_active_session().with(|s| {
                    let mut sess = s.borrow_mut();
                    let idx = r_idx as usize;
                    if let Some(RuleDefinition::CrossTableComparison {
                        id,
                        target_table,
                        key_concept,
                        target_key_concept,
                        comparison_type,
                        mismatch_brush_key,
                        show_proposal_dialog,
                        ..
                    }) = sess.working_info.rules.get_mut(idx)
                    {
                        *id = name.to_string();
                        *target_table = tgt_tbl.to_string();
                        *key_concept = host_k.to_string();
                        *target_key_concept = rem_k.to_string();
                        *comparison_type = Some(c_type.to_string());
                        *mismatch_brush_key = Some(brush.to_string());
                        *show_proposal_dialog = Some(show_prop);
                        sess.is_dirty = true;
                    }
                    drop(sess);
                    let sess = s.borrow();
                    refresh_all_tabs(&win, &sess);
                    validate_and_update_banner(&win, &sess, &state.borrow());
                });
            },
        );
    }

    // --- TAB 6: GRUPPENVALIDIERUNG ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_group_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let id = format!("Group_{}", sess.working_info.rules.len() + 1);
                let default_col = sess
                    .working_table
                    .as_ref()
                    .and_then(|t| get_table_concepts_and_headers(t).into_iter().next())
                    .unwrap_or_default();

                sess.working_info
                    .rules
                    .push(RuleDefinition::GroupValidation {
                        id: id.clone(),
                        description: format!("Gruppe {}", id),
                        action_group: None,
                        is_active: true,
                        is_highlighted: true,
                        group_by_concept: default_col.clone(),
                        validation_type: Some("isUnique".into()),
                        is_unique: true,
                        unique_concept: Some(default_col.clone()),
                        numeric_range: None,
                        group_consistency: None,
                        mismatch_brush_key: Some("UniquenessViolationBrush".into()),
                        mismatch_target: Some("Row".into()),
                        mismatch_priority: Some(90),
                    });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_group_idx(new_idx as i32);
                win.set_edit_group_name(slint::SharedString::from(id));
                win.set_edit_group_by_concept(slint::SharedString::from(&default_col));
                win.set_edit_group_val_type(slint::SharedString::from("isUnique"));
                win.set_edit_group_param_concept(slint::SharedString::from(&default_col));
                win.set_edit_group_min(slint::SharedString::from(""));
                win.set_edit_group_max(slint::SharedString::from(""));
                win.set_edit_group_brush(slint::SharedString::from("UniquenessViolationBrush"));
                win.set_edit_group_color(cached_color("UniquenessViolationBrush", 255));
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_select_group_rule(move |r_idx| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                if let Some(RuleDefinition::GroupValidation {
                    id,
                    group_by_concept,
                    validation_type,
                    unique_concept,
                    numeric_range,
                    mismatch_brush_key,
                    ..
                }) = sess.working_info.rules.get(r_idx as usize)
                {
                    let b_key = mismatch_brush_key
                        .as_deref()
                        .unwrap_or("UniquenessViolationBrush");
                    win.set_edit_group_name(slint::SharedString::from(id));
                    win.set_edit_group_by_concept(slint::SharedString::from(group_by_concept));
                    win.set_edit_group_val_type(slint::SharedString::from(
                        validation_type.as_deref().unwrap_or("isUnique"),
                    ));
                    win.set_edit_group_param_concept(slint::SharedString::from(
                        unique_concept.as_deref().unwrap_or(""),
                    ));
                    win.set_edit_group_min(slint::SharedString::from(
                        numeric_range
                            .as_ref()
                            .and_then(|r| r.min)
                            .map(|m| m.to_string())
                            .unwrap_or_default(),
                    ));
                    win.set_edit_group_max(slint::SharedString::from(
                        numeric_range
                            .as_ref()
                            .and_then(|r| r.max)
                            .map(|m| m.to_string())
                            .unwrap_or_default(),
                    ));
                    win.set_edit_group_brush(slint::SharedString::from(b_key));
                    win.set_edit_group_color(cached_color(b_key, 255));
                }
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_group_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_group_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_group_rule_edit(
            move |r_idx, name, grp_by, v_type, param, min_str, max_str, brush| {
                get_active_session().with(|s| {
                    let mut sess = s.borrow_mut();
                    let idx = r_idx as usize;
                    if let Some(RuleDefinition::GroupValidation {
                        id,
                        group_by_concept,
                        validation_type,
                        is_unique,
                        unique_concept,
                        numeric_range,
                        group_consistency,
                        mismatch_brush_key,
                        ..
                    }) = sess.working_info.rules.get_mut(idx)
                    {
                        *id = name.to_string();
                        *group_by_concept = grp_by.to_string();
                        *validation_type = Some(v_type.to_string());
                        *mismatch_brush_key = Some(brush.to_string());

                        if v_type == "isUnique" {
                            *is_unique = true;
                            *unique_concept = if param.is_empty() {
                                None
                            } else {
                                Some(param.to_string())
                            };
                            *numeric_range = None;
                            *group_consistency = None;
                        } else if v_type == "numericRange" {
                            *is_unique = false;
                            *unique_concept = None;
                            *numeric_range = Some(NumericRangeCheck {
                                concept: param.to_string(),
                                min: min_str.parse().ok(),
                                max: max_str.parse().ok(),
                            });
                            *group_consistency = None;
                        } else {
                            *is_unique = false;
                            *unique_concept = None;
                            *numeric_range = None;
                            *group_consistency = Some(GroupConsistencyCheck {
                                concept: param.to_string(),
                            });
                        }
                        sess.is_dirty = true;
                    }
                    drop(sess);
                    let sess = s.borrow();
                    refresh_all_tabs(&win, &sess);
                    validate_and_update_banner(&win, &sess, &state.borrow());
                });
            },
        );
    }

    // --- TAB 7: HIGHLIGHTS ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_highlight_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let id = format!("Highlight_{}", sess.working_info.rules.len() + 1);
                let default_col = sess
                    .working_table
                    .as_ref()
                    .and_then(|t| get_table_concepts_and_headers(t).into_iter().next())
                    .unwrap_or_default();

                sess.working_info.rules.push(RuleDefinition::Highlight {
                    id: id.clone(),
                    description: format!("Highlight {}", id),
                    action_group: None,
                    is_active: true,
                    is_highlighted: true,
                    column_concept: default_col.clone(),
                    condition: "Equals".into(),
                    value_to_match: "".into(),
                    multiple_values: Vec::new(),
                    invert_condition: false,
                    brush_key: "WarningBrush".into(),
                    target: "Cell".into(),
                    priority: 50,
                });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_hl_idx(new_idx as i32);
                drop(sess);
                let sess = s.borrow();
                populate_selected_highlight_rule(&win, &sess, new_idx);
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_select_highlight_rule(move |r_idx| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                populate_selected_highlight_rule(&win, &sess, r_idx as usize);
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_hl_column_changed(move |col_concept| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                update_hl_column_value_counts(&win, &sess, &col_concept);
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_highlight_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_hl_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_highlight_rule_edit(
            move |r_idx, name, col, cond, val, invert, brush, tgt, prio| {
                if r_idx < 0 {
                    return;
                }
                get_active_session().with(|s| {
                    let mut sess = s.borrow_mut();
                    let idx = r_idx as usize;
                    if let Some(RuleDefinition::Highlight {
                        id,
                        column_concept,
                        condition,
                        value_to_match,
                        invert_condition,
                        brush_key,
                        target,
                        priority,
                        ..
                    }) = sess.working_info.rules.get_mut(idx)
                    {
                        *id = name.to_string();
                        *column_concept = col.to_string();
                        *condition = cond.to_string();
                        *value_to_match = val.to_string();
                        *invert_condition = invert;
                        *brush_key = brush.to_string();
                        *target = tgt.to_string();
                        *priority = prio.max(0) as u32;
                        sess.is_dirty = true;
                    }
                    drop(sess);
                    let sess = s.borrow();
                    refresh_all_tabs(&win, &sess);
                    validate_and_update_banner(&win, &sess, &state.borrow());
                });
            },
        );
    }

    // --- TAB 8: ÜBERTRAGUNGEN ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_add_apply_rule(move || {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let id = format!("Apply_{}", sess.working_info.rules.len() + 1);
                let default_col = sess
                    .working_table
                    .as_ref()
                    .and_then(|t| get_table_concepts_and_headers(t).into_iter().next())
                    .unwrap_or_default();

                sess.working_info.rules.push(RuleDefinition::Apply {
                    id: id.clone(),
                    description: format!("Übertragung {}", id),
                    action_group: None,
                    is_active: true,
                    source_column_concepts: if default_col.is_empty() {
                        Vec::new()
                    } else {
                        vec![default_col.clone()]
                    },
                    requires_confirmation: true,
                    execute_automatically: false,
                    target_table: "$self".into(),
                    key_mappings: Vec::new(),
                    field_assignments: Vec::new(),
                });
                sess.is_dirty = true;
                let new_idx = sess.working_info.rules.len() - 1;
                win.set_selected_apply_idx(new_idx as i32);
                win.set_edit_apply_name(slint::SharedString::from(id));
                win.set_edit_apply_source(slint::SharedString::from(&default_col));
                win.set_edit_apply_target_table(slint::SharedString::from("$self"));
                win.set_edit_apply_target_concept(slint::SharedString::from(&default_col));
                win.set_edit_apply_confirm(true);
                win.set_edit_apply_auto(false);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_select_apply_rule(move |r_idx| {
            get_active_session().with(|s| {
                let sess = s.borrow();
                if let Some(RuleDefinition::Apply {
                    id,
                    source_column_concepts,
                    target_table,
                    requires_confirmation,
                    execute_automatically,
                    ..
                }) = sess.working_info.rules.get(r_idx as usize)
                {
                    let src = source_column_concepts
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    win.set_edit_apply_name(slint::SharedString::from(id));
                    win.set_edit_apply_source(slint::SharedString::from(src));
                    win.set_edit_apply_target_table(slint::SharedString::from(target_table));
                    win.set_edit_apply_target_concept(slint::SharedString::from(src));
                    win.set_edit_apply_confirm(*requires_confirmation);
                    win.set_edit_apply_auto(*execute_automatically);
                }
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_delete_apply_rule(move |r_idx| {
            get_active_session().with(|s| {
                let mut sess = s.borrow_mut();
                let idx = r_idx as usize;
                if idx < sess.working_info.rules.len() {
                    sess.working_info.rules.remove(idx);
                    sess.is_dirty = true;
                }
                win.set_selected_apply_idx(-1);
                drop(sess);
                let sess = s.borrow();
                refresh_all_tabs(&win, &sess);
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_save_apply_rule_edit(
            move |r_idx, name, src, tgt_tbl, _tgt_c, conf, auto| {
                get_active_session().with(|s| {
                    let mut sess = s.borrow_mut();
                    let idx = r_idx as usize;
                    if let Some(RuleDefinition::Apply {
                        id,
                        source_column_concepts,
                        target_table,
                        requires_confirmation,
                        execute_automatically,
                        ..
                    }) = sess.working_info.rules.get_mut(idx)
                    {
                        *id = name.to_string();
                        *source_column_concepts = if src.is_empty() {
                            vec![]
                        } else {
                            vec![src.to_string()]
                        };
                        *target_table = tgt_tbl.to_string();
                        *requires_confirmation = conf;
                        *execute_automatically = auto;
                        sess.is_dirty = true;
                    }
                    drop(sess);
                    let sess = s.borrow();
                    refresh_all_tabs(&win, &sess);
                    validate_and_update_banner(&win, &sess, &state.borrow());
                });
            },
        );
    }

    // --- VALIDATION AND SAVE ---
    {
        let win = info_window.clone();
        let state = state.clone();
        info_window.on_run_validation_clicked(move || {
            get_active_session().with(|s| {
                let sess = s.borrow();
                validate_and_update_banner(&win, &sess, &state.borrow());
            });
        });
    }

    {
        let win = info_window.clone();
        let state = state.clone();
        let main_ui = main_window_weak.clone();

        info_window.on_save_and_apply_clicked(move || {
            get_active_session().with(|s| {
                let sess = s.borrow();
                let st = state.borrow();

                let dummy_table = TableData::new("val".into(), "val".into());
                let table_ref = sess.working_table.as_ref().unwrap_or(&dummy_table);

                let issues = TableConfigValidator::validate(
                    &sess.working_info,
                    table_ref,
                    &st.tables,
                    &st.app_config,
                );

                if TableConfigValidator::has_errors(&issues) {
                    for issue in &issues {
                        if issue.severity == ValidationSeverity::Error {
                            log_warn(
                                "InfoEditor",
                                &format!(
                                    "Validierungsfehler beim Speichern (Tab {}): {}",
                                    issue.tab_index + 1,
                                    issue.message
                                ),
                            );
                        }
                    }
                    validate_and_update_banner(&win, &sess, &st);
                    return;
                }

                // Determine file path to save info file
                let info_path = sess
                    .file_path
                    .as_ref()
                    .map(|p| default_info_path_for_table(p))
                    .unwrap_or_else(|| PathBuf::from("table.csv.info"));

                if let Err(e) = save_info_file(&info_path, &sess.working_info) {
                    log_warn(
                        "InfoFile",
                        &format!("Fehler beim Speichern von {:?}: {}", info_path, e),
                    );
                } else {
                    log_info(
                        "InfoFile",
                        &format!(
                            "Konfiguration erfolgreich gespeichert nach {:?} ({} Regeln)",
                            info_path,
                            sess.working_info.rules.len()
                        ),
                    );
                }

                drop(st);

                // If associated with a loaded table in AppState, apply immediately and execute all rules!
                if let Some(ref target_id) = sess.target_table_id {
                    let mut st = state.borrow_mut();
                    if let Some(target_tbl) = st.tables.get_mut(target_id) {
                        apply_info_to_table(target_tbl, &sess.working_info);
                    }
                    st.execute_all_rules();
                    if let Some(ui) = main_ui.upgrade() {
                        crate::sync_ui(&ui, &st);
                    }
                }

                let _ = win.hide();
            });
        });
    }

    {
        let win = info_window.clone();
        info_window.on_cancel_clicked(move || {
            let _ = win.hide();
        });
    }
}
