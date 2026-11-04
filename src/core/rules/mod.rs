use std::collections::{HashMap, HashSet};
use crate::core::model::{
    HighlightItem, HighlightSeverity, KeyMapping, LinkStep, RuleDefinition, TableData,
    VirtualColumnMapping,
};
use evalexpr::{eval_with_context, ContextWithMutableVariables, DefaultNumericTypes, HashMapContext, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinRelationshipType {
    OneToOne,
    OneToMany,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ChainAnalysisResult {
    pub step_stats: Vec<String>,
    pub relationship: JoinRelationshipType,
    pub relationship_badge: String,
    pub is_one_to_many: bool,
    pub total_matched_rows: usize,
    pub host_total_rows: usize,
}

#[derive(Debug, Clone)]
pub struct RuleGroupSummary {
    pub rule_id: String,
    pub description: String,
    pub hit_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone)]
pub struct ProposalRow {
    pub source_table: String,
    pub target_table: String,
    pub key_value: String,
    pub row_data: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuleEngineResult {
    pub highlights: Vec<HighlightItem>,
    pub summaries: Vec<RuleGroupSummary>,
    pub proposals: Vec<ProposalRow>,
}

pub struct RuleEngine;

impl RuleEngine {
    pub fn execute(
        current_table: &mut TableData,
        all_tables: &HashMap<String, TableData>,
    ) -> RuleEngineResult {
        let mut result = RuleEngineResult::default();
        let rules = current_table.rules.clone();

        for rule in &rules {
            match rule {
                RuleDefinition::BulkField {
                    id,
                    description,
                    concept,
                    check_all_for_empty,
                    check_all_for_placeholder,
                    placeholders,
                } => {
                    let col_idx = current_table.find_col_index(concept);
                    let mut hits = 0;
                    let mut errors = 0;

                    if let Some(col) = col_idx {
                        let default_placeholders = vec!["-", "N/A", "TBD", "???", "TODO", "NULL", "NONE"];
                        let check_ph: Vec<&str> = if placeholders.is_empty() {
                            default_placeholders
                        } else {
                            placeholders.iter().map(|s| s.as_str()).collect()
                        };

                        for (r_idx, row) in current_table.rows.iter().enumerate() {
                            let val = row.get(col).map(|s| s.trim()).unwrap_or("");
                            let mut flagged = false;
                            let mut msg = String::new();

                            if *check_all_for_empty && val.is_empty() {
                                flagged = true;
                                msg = format!("Pflichtfeld '{}' ist leer.", concept);
                            } else if *check_all_for_placeholder && !val.is_empty() {
                                if check_ph.iter().any(|ph| val.eq_ignore_ascii_case(ph)) {
                                    flagged = true;
                                    msg = format!("Feld '{}' enthält Platzhalter '{}'.", concept, val);
                                }
                            }

                            if flagged {
                                hits += 1;
                                errors += 1;
                                result.highlights.push(HighlightItem {
                                    row: r_idx,
                                    col: Some(col),
                                    severity: HighlightSeverity::Error,
                                    message: msg,
                                    rule_id: id.clone(),
                                });
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: hits,
                        error_count: errors,
                        warning_count: 0,
                    });
                }

                RuleDefinition::Calculation {
                    id,
                    description,
                    target_column,
                    formula,
                    ..
                } => {
                    let mut hits = 0;
                    let mut errors = 0;

                    // Ensure target virtual column exists in headers
                    let col_idx = match current_table.headers.iter().position(|h| h.eq_ignore_ascii_case(target_column)) {
                        Some(idx) => idx,
                        None => {
                            let new_idx = current_table.headers.len();
                            current_table.headers.push(target_column.clone());
                            current_table.is_virtual_col.push(true);
                            current_table.column_settings.push(crate::core::model::ColumnSetting {
                                column_name: target_column.clone(),
                                width: 160,
                                is_visible: true,
                                display_order: new_idx as u32,
                            });
                            for row in &mut current_table.rows {
                                row.push(String::new());
                            }
                            new_idx
                        }
                    };

                    for r_idx in 0..current_table.rows.len() {
                        let mut ctx: HashMapContext<DefaultNumericTypes> = HashMapContext::new();

                        // Populate context from row
                        for (c_idx, h) in current_table.headers.iter().enumerate() {
                            if c_idx == col_idx {
                                continue;
                            }
                            let cell_val = current_table.rows[r_idx].get(c_idx).map(|s| s.as_str()).unwrap_or("");
                            // Try int or float first
                            let clean_h = h.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                            if let Ok(i) = cell_val.parse::<i64>() {
                                let _ = ctx.set_value(clean_h, Value::Int(i));
                            } else if let Ok(f) = cell_val.parse::<f64>() {
                                let _ = ctx.set_value(clean_h, Value::Float(f));
                            } else {
                                let _ = ctx.set_value(clean_h, Value::String(cell_val.to_string()));
                            }
                        }

                        match eval_with_context(formula, &ctx) {
                            Ok(val) => {
                                let val_str = match val {
                                    Value::String(s) => s,
                                    Value::Float(f) => format!("{:.2}", f),
                                    Value::Int(i) => format!("{}", i),
                                    Value::Boolean(b) => format!("{}", b),
                                    _ => String::new(),
                                };
                                current_table.rows[r_idx][col_idx] = val_str;
                            }
                            Err(e) => {
                                hits += 1;
                                errors += 1;
                                current_table.rows[r_idx][col_idx] = format!("#ERR: {}", e);
                                result.highlights.push(HighlightItem {
                                    row: r_idx,
                                    col: Some(col_idx),
                                    severity: HighlightSeverity::Warning,
                                    message: format!("Formelfehler in Zeile {}: {}", r_idx + 1, e),
                                    rule_id: id.clone(),
                                });
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: hits,
                        error_count: 0,
                        warning_count: errors,
                    });
                }

                RuleDefinition::GroupValidation {
                    id,
                    description,
                    group_by_concept,
                    is_unique,
                    unique_concept,
                    numeric_range,
                    group_consistency,
                    ..
                } => {
                    let mut hits = 0;
                    let mut errors = 0;
                    let mut warnings = 0;

                    let grp_col = current_table.find_col_index(group_by_concept);
                    if let Some(grp_c) = grp_col {
                        // Group rows by group value
                        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
                        for (r_idx, row) in current_table.rows.iter().enumerate() {
                            let key = row.get(grp_c).map(|s| s.trim()).unwrap_or("");
                            if !key.is_empty() {
                                groups.entry(key.to_string()).or_default().push(r_idx);
                            }
                        }

                        // Check uniqueness within group or across table
                        if *is_unique {
                            let check_col = unique_concept
                                .as_ref()
                                .and_then(|c| current_table.find_col_index(c))
                                .unwrap_or(grp_c);

                            for (_grp, row_indices) in &groups {
                                let mut seen_values: HashMap<String, usize> = HashMap::new();
                                for &r_idx in row_indices {
                                    let val = current_table.rows[r_idx].get(check_col).map(|s| s.trim()).unwrap_or("");
                                    if !val.is_empty() {
                                        if let Some(&first_seen) = seen_values.get(val) {
                                            hits += 1;
                                            errors += 1;
                                            result.highlights.push(HighlightItem {
                                                row: r_idx,
                                                col: Some(check_col),
                                                severity: HighlightSeverity::Error,
                                                message: format!("Duplikat von '{}' (bereits in Zeile {})", val, first_seen + 1),
                                                rule_id: id.clone(),
                                            });
                                        } else {
                                            seen_values.insert(val.to_string(), r_idx);
                                        }
                                    }
                                }
                            }
                        }

                        // Check numeric range
                        if let Some(range) = numeric_range {
                            if let Some(target_col) = current_table.find_col_index(&range.concept) {
                                for (r_idx, row) in current_table.rows.iter().enumerate() {
                                    let val_str = row.get(target_col).map(|s| s.trim()).unwrap_or("");
                                    if let Ok(num) = val_str.parse::<f64>() {
                                        let mut out_of_range = false;
                                        if let Some(min) = range.min {
                                            if num < min {
                                                out_of_range = true;
                                            }
                                        }
                                        if let Some(max) = range.max {
                                            if num > max {
                                                out_of_range = true;
                                            }
                                        }
                                        if out_of_range {
                                            hits += 1;
                                            warnings += 1;
                                            result.highlights.push(HighlightItem {
                                                row: r_idx,
                                                col: Some(target_col),
                                                severity: HighlightSeverity::Warning,
                                                message: format!("Wert {} liegt außerhalb des gültigen Bereichs [min: {:?}, max: {:?}]", num, range.min, range.max),
                                                rule_id: id.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        // Check group consistency
                        if let Some(consistency) = group_consistency {
                            if let Some(cons_col) = current_table.find_col_index(&consistency.concept) {
                                for (grp_key, row_indices) in &groups {
                                    if row_indices.len() > 1 {
                                        let first_val = current_table.rows[row_indices[0]].get(cons_col).map(|s| s.trim()).unwrap_or("");
                                        for &r_idx in &row_indices[1..] {
                                            let cur_val = current_table.rows[r_idx].get(cons_col).map(|s| s.trim()).unwrap_or("");
                                            if cur_val != first_val {
                                                hits += 1;
                                                warnings += 1;
                                                result.highlights.push(HighlightItem {
                                                    row: r_idx,
                                                    col: Some(cons_col),
                                                    severity: HighlightSeverity::Warning,
                                                    message: format!("Inkonsistenter Wert '{}' in Gruppe '{}' (erwartet: '{}')", cur_val, grp_key, first_val),
                                                    rule_id: id.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: hits,
                        error_count: errors,
                        warning_count: warnings,
                    });
                }

                RuleDefinition::VirtualRowJoin {
                    id,
                    description,
                    key_concept,
                    target_table: target_table_name,
                    target_key_concept,
                    import_concepts,
                    link_steps,
                    virtual_columns,
                    ..
                } => {
                    let (steps, v_cols) = if !link_steps.is_empty() {
                        (link_steps.clone(), virtual_columns.clone())
                    } else if !target_table_name.is_empty() {
                        let s = LinkStep {
                            target_table_key: target_table_name.clone(),
                            key_mappings: vec![KeyMapping {
                                host_concept: key_concept.clone(),
                                remote_concept: target_key_concept.clone(),
                                manual_remote_value: None,
                                is_case_sensitive: false,
                            }],
                        };
                        let vc = import_concepts.iter().map(|imp| VirtualColumnMapping {
                            remote_column_concept: imp.clone(),
                            virtual_column_header: format!("{}.{}", target_table_name, imp),
                            virtual_column_key: format!("v_{}", imp.to_lowercase()),
                            multi_result_display: "First".to_string(),
                            brush_key: None,
                            priority: Some(5),
                        }).collect();
                        (vec![s], vc)
                    } else {
                        (Vec::new(), Vec::new())
                    };

                    let mut total_hits = 0;
                    if !steps.is_empty() && !v_cols.is_empty() {
                        let mut col_indices = Vec::new();
                        for v in &v_cols {
                            let virt_header = &v.virtual_column_header;
                            let virt_col_idx = match current_table.headers.iter().position(|h| h.eq_ignore_ascii_case(virt_header)) {
                                Some(idx) => idx,
                                None => {
                                    let new_idx = current_table.headers.len();
                                    current_table.headers.push(virt_header.clone());
                                    current_table.is_virtual_col.push(true);
                                    current_table.column_settings.push(crate::core::model::ColumnSetting {
                                        column_name: virt_header.clone(),
                                        width: 160,
                                        is_visible: true,
                                        display_order: new_idx as u32,
                                    });
                                    for row in &mut current_table.rows {
                                        row.push(String::new());
                                    }
                                    new_idx
                                }
                            };
                            col_indices.push(virt_col_idx);
                        }

                        let mut row_updates: Vec<Vec<String>> = Vec::with_capacity(current_table.rows.len());

                        for r_idx in 0..current_table.rows.len() {
                            let current_source_row = current_table.rows[r_idx].clone();
                            let mut intermediate_rows: Vec<Vec<String>> = vec![current_source_row];
                            let mut intermediate_table: Option<&TableData> = Some(&*current_table);

                            for step in &steps {
                                let tgt_table = match RuleEngine::resolve_table(&step.target_table_key, current_table, all_tables) {
                                    Some(t) => t,
                                    None => {
                                        intermediate_rows.clear();
                                        break;
                                    }
                                };

                                let prev_tbl = intermediate_table.unwrap_or(current_table);
                                let mut next_rows = Vec::new();
                                for src_row in &intermediate_rows {
                                    for tgt_row in &tgt_table.rows {
                                        let matched = step.key_mappings.iter().all(|m| {
                                            let r_col = tgt_table.find_col_index(&m.remote_concept);
                                            if let Some(rc) = r_col {
                                                let r_val = tgt_row.get(rc).map(|s| s.as_str()).unwrap_or("");
                                                if let Some(ref man_v) = m.manual_remote_value {
                                                    RuleEngine::values_match(man_v, r_val, m.is_case_sensitive)
                                                } else if let Some(hc) = prev_tbl.find_col_index(&m.host_concept) {
                                                    let h_val = src_row.get(hc).map(|s| s.as_str()).unwrap_or("");
                                                    RuleEngine::values_match(h_val, r_val, m.is_case_sensitive)
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        });
                                        if matched {
                                            next_rows.push(tgt_row.clone());
                                        }
                                    }
                                }
                                intermediate_rows = next_rows;
                                intermediate_table = Some(tgt_table);
                                if intermediate_rows.is_empty() {
                                    break;
                                }
                            }

                            if !intermediate_rows.is_empty() {
                                total_hits += 1;
                            }

                            let final_table = intermediate_table.unwrap_or(current_table);
                            let mut this_row_cols = Vec::with_capacity(v_cols.len());
                            for (v_i, v) in v_cols.iter().enumerate() {
                                let col_idx = col_indices[v_i];
                                let remote_col_opt = final_table.find_col_index(&v.remote_column_concept);

                                let val_str = if let Some(rc) = remote_col_opt {
                                    let raw_vals: Vec<&str> = intermediate_rows.iter()
                                        .map(|r| r.get(rc).map(|s| s.as_str().trim()).unwrap_or(""))
                                        .filter(|s| !s.is_empty())
                                        .collect();

                                    match v.multi_result_display.as_str() {
                                        "First" => raw_vals.first().copied().unwrap_or("").to_string(),
                                        "CountMerged" => intermediate_rows.len().to_string(),
                                        "CountSeparated" => {
                                            let distinct: HashSet<&str> = raw_vals.iter().copied().collect();
                                            distinct.len().to_string()
                                        }
                                        "CommaSeparated" => {
                                            let mut distinct: Vec<&str> = Vec::new();
                                            for val in raw_vals {
                                                if !distinct.contains(&val) {
                                                    distinct.push(val);
                                                }
                                            }
                                            distinct.join(", ")
                                        }
                                        "CommaSeparatedMerged" => raw_vals.join(", "),
                                        "Sum" => {
                                            let sum: f64 = raw_vals.iter().filter_map(|s| s.parse::<f64>().ok()).sum();
                                            if sum.fract() == 0.0 {
                                                format!("{:.0}", sum)
                                            } else {
                                                format!("{:.2}", sum)
                                            }
                                        }
                                        "Average" => {
                                            let nums: Vec<f64> = raw_vals.iter().filter_map(|s| s.parse::<f64>().ok()).collect();
                                            if !nums.is_empty() {
                                                let avg = nums.iter().sum::<f64>() / nums.len() as f64;
                                                if avg.fract() == 0.0 {
                                                    format!("{:.0}", avg)
                                                } else {
                                                    format!("{:.2}", avg)
                                                }
                                            } else {
                                                String::new()
                                            }
                                        }
                                        _ => raw_vals.first().copied().unwrap_or("").to_string(),
                                    }
                                } else {
                                    String::new()
                                };

                                if let Some(ref bk) = v.brush_key {
                                    if !bk.is_empty() && bk != "Keine" && !intermediate_rows.is_empty() {
                                        result.highlights.push(HighlightItem {
                                            row: r_idx,
                                            col: Some(col_idx),
                                            severity: HighlightSeverity::Info,
                                            message: format!("Virtuelle Spalte '{}': Treffer", v.virtual_column_header),
                                            rule_id: id.clone(),
                                        });
                                    }
                                }

                                this_row_cols.push(val_str);
                            }
                            row_updates.push(this_row_cols);
                        }

                        // Apply updates to current_table rows
                        for (r_idx, updates) in row_updates.into_iter().enumerate() {
                            for (v_i, val_str) in updates.into_iter().enumerate() {
                                let col_idx = col_indices[v_i];
                                current_table.rows[r_idx][col_idx] = val_str;
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: total_hits,
                        error_count: 0,
                        warning_count: 0,
                    });
                }

                RuleDefinition::CrossTableComparison {
                    id,
                    description,
                    key_concept,
                    target_table: target_table_name,
                    target_key_concept,
                    compare_concepts: _,
                    ..
                } => {
                    let mut hits = 0;
                    let mut warnings = 0;

                    if let Some(target_tbl) = all_tables.get(target_table_name) {
                        let cur_k = current_table.find_col_index(key_concept);
                        let tgt_k = target_tbl.find_col_index(target_key_concept);

                        if let (Some(ck), Some(tk)) = (cur_k, tgt_k) {
                            let cur_keys: HashSet<String> = current_table.rows.iter()
                                .map(|r| r.get(ck).map(|s| s.trim().to_string()).unwrap_or_default())
                                .filter(|s| !s.is_empty())
                                .collect();

                            let tgt_keys: HashSet<String> = target_tbl.rows.iter()
                                .map(|r| r.get(tk).map(|s| s.trim().to_string()).unwrap_or_default())
                                .filter(|s| !s.is_empty())
                                .collect();

                            // Rows in target that are missing in current
                            for (_t_idx, t_row) in target_tbl.rows.iter().enumerate() {
                                let key_val = t_row.get(tk).map(|s| s.trim()).unwrap_or("");
                                if !key_val.is_empty() && !cur_keys.contains(key_val) {
                                    hits += 1;
                                    warnings += 1;
                                    result.proposals.push(ProposalRow {
                                        source_table: target_table_name.clone(),
                                        target_table: current_table.display_name.clone(),
                                        key_value: key_val.to_string(),
                                        row_data: t_row.clone(),
                                    });
                                }
                            }

                            // Flag rows in current that are missing in target
                            for (c_idx, c_row) in current_table.rows.iter().enumerate() {
                                let key_val = c_row.get(ck).map(|s| s.trim()).unwrap_or("");
                                if !key_val.is_empty() && !tgt_keys.contains(key_val) {
                                    result.highlights.push(HighlightItem {
                                        row: c_idx,
                                        col: Some(ck),
                                        severity: HighlightSeverity::Info,
                                        message: format!("Datensatz mit Schlüssel '{}' existiert nicht in '{}'", key_val, target_table_name),
                                        rule_id: id.clone(),
                                    });
                                }
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: hits,
                        error_count: 0,
                        warning_count: warnings,
                    });
                }

                RuleDefinition::Highlight {
                    id,
                    description,
                    column_concept,
                    condition,
                    value_to_match,
                    multiple_values,
                    invert_condition,
                    brush_key: _,
                    target,
                    priority: _,
                    ..
                } => {
                    let mut hits = 0;
                    let col_idx = current_table.find_col_index(column_concept);
                    if let Some(col) = col_idx {
                        for (r_idx, row) in current_table.rows.iter().enumerate() {
                            let val = row.get(col).map(|s| s.trim()).unwrap_or("");
                            let mut matches = match condition.to_lowercase().as_str() {
                                "equals" => val.eq_ignore_ascii_case(value_to_match),
                                "contains" => val.to_lowercase().contains(&value_to_match.to_lowercase()),
                                "startswith" => val.to_lowercase().starts_with(&value_to_match.to_lowercase()),
                                "endswith" => val.to_lowercase().ends_with(&value_to_match.to_lowercase()),
                                "isempty" => val.is_empty(),
                                "isnotempty" => !val.is_empty(),
                                "inlist" => multiple_values.iter().any(|v| val.eq_ignore_ascii_case(v.trim())),
                                _ => val.to_lowercase().contains(&value_to_match.to_lowercase()),
                            };
                            if *invert_condition {
                                matches = !matches;
                            }

                            if matches {
                                hits += 1;
                                let c_opt = if target.eq_ignore_ascii_case("row") { None } else { Some(col) };
                                result.highlights.push(HighlightItem {
                                    row: r_idx,
                                    col: c_opt,
                                    severity: HighlightSeverity::Info,
                                    message: format!("Highlight: {}", description),
                                    rule_id: id.clone(),
                                });
                            }
                        }
                    }

                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: hits,
                        error_count: 0,
                        warning_count: 0,
                    });
                }

                RuleDefinition::Apply {
                    id,
                    description,
                    ..
                } => {
                    result.summaries.push(RuleGroupSummary {
                        rule_id: id.clone(),
                        description: description.clone(),
                        hit_count: 0,
                        error_count: 0,
                        warning_count: 0,
                    });
                }
            }
        }

        current_table.rule_highlights = result.highlights.clone();
        result
    }

    pub fn calculate_join_match_ratio(
        current_table: &TableData,
        target_table: &TableData,
        host_concept: &str,
        remote_concept: &str,
    ) -> (usize, usize, f64) {
        let host_col = match current_table.find_col_index(host_concept) {
            Some(c) => c,
            None => return (0, current_table.rows.len(), 0.0),
        };
        let remote_col = match target_table.find_col_index(remote_concept) {
            Some(c) => c,
            None => return (0, target_table.rows.len(), 0.0),
        };

        let target_keys: HashSet<String> = target_table.rows.iter()
            .map(|r| r.get(remote_col).map(|s| s.trim().to_lowercase()).unwrap_or_default())
            .filter(|s| !s.is_empty())
            .collect();

        let total = current_table.rows.len();
        if total == 0 {
            return (0, 0, 100.0);
        }

        let mut matched = 0;
        for row in &current_table.rows {
            let key = row.get(host_col).map(|s| s.trim().to_lowercase()).unwrap_or_default();
            if !key.is_empty() && target_keys.contains(&key) {
                matched += 1;
            }
        }

        let ratio = (matched as f64 / total as f64) * 100.0;
        (matched, total, ratio)
    }

    pub fn values_match(host_val: &str, remote_val: &str, is_case_sensitive: bool) -> bool {
        let h = host_val.trim();
        let r = remote_val.trim();
        // SQL-Null-Semantik: Zwei leere oder reine Whitespace-Zellen verbinden sich niemals.
        if h.is_empty() || r.is_empty() {
            return false;
        }
        // Numerische Normalisierung: Strings, die gueltige Dezimalzahlen darstellen, numerisch vergleichen
        if let (Ok(h_num), Ok(r_num)) = (h.parse::<f64>(), r.parse::<f64>()) {
            if !h_num.is_nan() && !r_num.is_nan() {
                return (h_num - r_num).abs() < f64::EPSILON;
            }
        }
        if is_case_sensitive {
            h == r
        } else {
            h.eq_ignore_ascii_case(r)
        }
    }

    pub fn resolve_table<'a>(
        table_key: &str,
        current_table: &'a TableData,
        all_tables: &'a HashMap<String, TableData>,
    ) -> Option<&'a TableData> {
        let key = table_key.trim();
        if key.is_empty() {
            return None;
        }
        if key == "$self" {
            return Some(current_table);
        }
        if current_table.display_name.eq_ignore_ascii_case(key)
            || current_table.id.eq_ignore_ascii_case(key)
            || current_table.file_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().eq_ignore_ascii_case(key)).unwrap_or(false)
        {
            return Some(current_table);
        }
        if let Some(t) = all_tables.get(key) {
            return Some(t);
        }
        all_tables.values().find(|t| {
            t.display_name.eq_ignore_ascii_case(key)
                || t.id.eq_ignore_ascii_case(key)
                || t.file_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().eq_ignore_ascii_case(key)).unwrap_or(false)
        })
    }

    pub fn calculate_step_hit_count(
        source_table: &TableData,
        target_table: &TableData,
        key_mappings: &[KeyMapping],
    ) -> (usize, usize, String) {
        let total = source_table.rows.len();
        if total == 0 {
            return (0, 0, "0 / 0 Quellzeilen gematcht".to_string());
        }
        if key_mappings.is_empty() {
            return (0, total, "Warte auf vollständige Bedingung...".to_string());
        }

        let mut resolved_conditions = Vec::new();
        for m in key_mappings {
            let host_col = source_table.find_col_index(&m.host_concept);
            let remote_col = target_table.find_col_index(&m.remote_concept);
            if host_col.is_none() && m.manual_remote_value.is_none() {
                return (0, total, "Warte auf vollständige Bedingung...".to_string());
            }
            if remote_col.is_none() {
                return (0, total, "Warte auf vollständige Bedingung...".to_string());
            }
            resolved_conditions.push((host_col, remote_col.unwrap(), m.manual_remote_value.clone(), m.is_case_sensitive));
        }

        let mut matched_source_count = 0;
        for s_row in &source_table.rows {
            let found = target_table.rows.iter().any(|t_row| {
                resolved_conditions.iter().all(|(h_col, r_col, manual_val, case_sens)| {
                    let r_val = t_row.get(*r_col).map(|s| s.as_str()).unwrap_or("");
                    if let Some(ref man_v) = manual_val {
                        RuleEngine::values_match(man_v, r_val, *case_sens)
                    } else if let Some(h_idx) = h_col {
                        let h_val = s_row.get(*h_idx).map(|s| s.as_str()).unwrap_or("");
                        RuleEngine::values_match(h_val, r_val, *case_sens)
                    } else {
                        false
                    }
                })
            });
            if found {
                matched_source_count += 1;
            }
        }

        let stat_str = format!("{} / {} Quellzeilen gematcht", matched_source_count, total);
        (matched_source_count, total, stat_str)
    }

    pub fn analyze_chain(
        host_table: &TableData,
        link_steps: &[LinkStep],
        all_tables: &HashMap<String, TableData>,
    ) -> ChainAnalysisResult {
        let total_host_rows = host_table.rows.len();
        if link_steps.is_empty() {
            return ChainAnalysisResult {
                step_stats: Vec::new(),
                relationship: JoinRelationshipType::Unknown,
                relationship_badge: "Keine Verknüpfungsschritte".to_string(),
                is_one_to_many: false,
                total_matched_rows: 0,
                host_total_rows: total_host_rows,
            };
        }

        let mut step_stats = Vec::new();
        let mut intermediate_table = host_table;

        for step in link_steps {
            let tgt_table = RuleEngine::resolve_table(&step.target_table_key, host_table, all_tables);
            if let Some(target) = tgt_table {
                let (_, _, stat) = RuleEngine::calculate_step_hit_count(intermediate_table, target, &step.key_mappings);
                step_stats.push(stat);
                intermediate_table = target;
            } else {
                step_stats.push("Zieltabelle nicht geöffnet".to_string());
                return ChainAnalysisResult {
                    step_stats,
                    relationship: JoinRelationshipType::Unknown,
                    relationship_badge: "Zieltabelle nicht geöffnet".to_string(),
                    is_one_to_many: false,
                    total_matched_rows: 0,
                    host_total_rows: total_host_rows,
                };
            }
        }

        let mut has_one_to_many = false;
        let mut total_matched = 0;

        for h_row in &host_table.rows {
            let mut current_rows: Vec<&Vec<String>> = vec![h_row];
            let mut curr_table = host_table;

            for step in link_steps {
                let tgt_table = match RuleEngine::resolve_table(&step.target_table_key, host_table, all_tables) {
                    Some(t) => t,
                    None => {
                        current_rows.clear();
                        break;
                    }
                };

                let mut next_rows: Vec<&Vec<String>> = Vec::new();
                for src_row in &current_rows {
                    for tgt_row in &tgt_table.rows {
                        let matched = step.key_mappings.iter().all(|m| {
                            let r_col = tgt_table.find_col_index(&m.remote_concept);
                            if let Some(rc) = r_col {
                                let r_val = tgt_row.get(rc).map(|s| s.as_str()).unwrap_or("");
                                if let Some(ref man_v) = m.manual_remote_value {
                                    RuleEngine::values_match(man_v, r_val, m.is_case_sensitive)
                                } else if let Some(hc) = curr_table.find_col_index(&m.host_concept) {
                                    let h_val = src_row.get(hc).map(|s| s.as_str()).unwrap_or("");
                                    RuleEngine::values_match(h_val, r_val, m.is_case_sensitive)
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        });
                        if matched {
                            next_rows.push(tgt_row);
                        }
                    }
                }

                current_rows = next_rows;
                curr_table = tgt_table;
                if current_rows.is_empty() {
                    break;
                }
            }

            if !current_rows.is_empty() {
                total_matched += 1;
            }
            if current_rows.len() > 1 {
                has_one_to_many = true;
            }
        }

        let relationship = if has_one_to_many {
            JoinRelationshipType::OneToMany
        } else {
            JoinRelationshipType::OneToOne
        };

        let relationship_badge = if has_one_to_many {
            "1:n Beziehung (Mehrere Treffer möglich)".to_string()
        } else {
            "1:1 Beziehung (Eindeutig)".to_string()
        };

        ChainAnalysisResult {
            step_stats,
            relationship,
            relationship_badge,
            is_one_to_many: has_one_to_many,
            total_matched_rows: total_matched,
            host_total_rows: total_host_rows,
        }
    }
}
