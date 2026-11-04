use std::collections::{HashMap, HashSet};
use crate::core::model::{
    HighlightItem, HighlightSeverity, RuleDefinition, TableData
};
use evalexpr::{eval_with_context, ContextWithMutableVariables, DefaultNumericTypes, HashMapContext, Value};

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
                    ..
                } => {
                    let mut hits = 0;
                    if let Some(target_tbl) = all_tables.get(target_table_name) {
                        let current_key_col = current_table.find_col_index(key_concept);
                        let target_key_col = target_tbl.find_col_index(target_key_concept);

                        if let (Some(c_k), Some(t_k)) = (current_key_col, target_key_col) {
                            // Index target table by key
                            let mut target_index: HashMap<String, &Vec<String>> = HashMap::new();
                            for row in &target_tbl.rows {
                                let key = row.get(t_k).map(|s| s.trim()).unwrap_or("");
                                if !key.is_empty() {
                                    target_index.insert(key.to_string(), row);
                                }
                            }

                            // Inject import_concepts as virtual columns
                            for imp_concept in import_concepts {
                                let virt_header = format!("{}.{}", target_table_name, imp_concept);
                                let virt_col_idx = match current_table.headers.iter().position(|h| h.eq_ignore_ascii_case(&virt_header)) {
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

                                let tgt_concept_col = target_tbl.find_col_index(imp_concept);

                                for r_idx in 0..current_table.rows.len() {
                                    let cur_key = current_table.rows[r_idx].get(c_k).map(|s| s.trim()).unwrap_or("");
                                    if let Some(matched_row) = target_index.get(cur_key) {
                                        let val = tgt_concept_col.and_then(|c| matched_row.get(c)).cloned().unwrap_or_default();
                                        current_table.rows[r_idx][virt_col_idx] = val;
                                        hits += 1;
                                    } else {
                                        current_table.rows[r_idx][virt_col_idx] = String::new();
                                    }
                                }
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
}
