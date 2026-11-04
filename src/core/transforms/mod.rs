use std::collections::HashSet;
use crate::core::model::{ColumnSetting, TableData};

#[derive(Debug, Clone, PartialEq)]
pub enum FillMode {
    Static(String),
    PrefixSuffix { prefix: String, suffix: String },
    Increment { start: i64, step: i64, prefix: String, pad_zeros: usize },
}

pub fn fill_column(
    table: &mut TableData,
    col_idx: usize,
    rows: &[usize],
    mode: FillMode,
) {
    if col_idx >= table.headers.len() {
        return;
    }

    let target_rows: Vec<usize> = if rows.is_empty() {
        (0..table.rows.len()).collect()
    } else {
        rows.to_vec()
    };

    match mode {
        FillMode::Static(val) => {
            for &r in &target_rows {
                table.set_cell(r, col_idx, val.clone());
            }
        }
        FillMode::PrefixSuffix { prefix, suffix } => {
            for &r in &target_rows {
                let current = table.rows[r].get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let new_val = format!("{}{}{}", prefix, current, suffix);
                table.set_cell(r, col_idx, new_val);
            }
        }
        FillMode::Increment { start, step, prefix, pad_zeros } => {
            for (i, &r) in target_rows.iter().enumerate() {
                let num = start + (i as i64 * step);
                let num_str = if pad_zeros > 0 {
                    format!("{:0width$}", num, width = pad_zeros)
                } else {
                    num.to_string()
                };
                let new_val = format!("{}{}", prefix, num_str);
                table.set_cell(r, col_idx, new_val);
            }
        }
    }
}

pub fn split_column(
    table: &mut TableData,
    col_idx: usize,
    split_delimiter: char,
    new_col_names: &[String],
) {
    if col_idx >= table.headers.len() || new_col_names.is_empty() {
        return;
    }

    let insert_pos = col_idx + 1;
    let count = new_col_names.len();

    // Insert headers
    for (i, name) in new_col_names.iter().enumerate() {
        let idx = insert_pos + i;
        if idx <= table.headers.len() {
            table.headers.insert(idx, name.clone());
        } else {
            table.headers.push(name.clone());
        }

        if idx <= table.is_virtual_col.len() {
            table.is_virtual_col.insert(idx, false);
        } else {
            table.is_virtual_col.push(false);
        }

        let setting = ColumnSetting {
            column_name: name.clone(),
            width: 150,
            is_visible: true,
            display_order: idx as u32,
        };
        if idx <= table.column_settings.len() {
            table.column_settings.insert(idx, setting);
        } else {
            table.column_settings.push(setting);
        }
    }

    // Split cell contents
    for row in &mut table.rows {
        let original_val = row.get(col_idx).cloned().unwrap_or_default();
        let parts: Vec<&str> = original_val.split(split_delimiter).collect();

        for (i, &part) in parts.iter().take(count).enumerate() {
            row.insert(insert_pos + i, part.trim().to_string());
        }
        for i in parts.len()..count {
            row.insert(insert_pos + i, String::new());
        }
    }

    table.is_dirty = true;
}

/// Smart Add: Inspects row values and increments trailing digits (e.g. "srv-01" -> "srv-02", "192.168.1.10" -> "192.168.1.11")
pub fn smart_add_row(table: &mut TableData, source_row_idx: Option<usize>) -> usize {
    let template_row = if let Some(idx) = source_row_idx {
        if idx < table.rows.len() {
            table.rows[idx].clone()
        } else {
            vec![String::new(); table.headers.len()]
        }
    } else if let Some(last) = table.rows.last() {
        last.clone()
    } else {
        vec![String::new(); table.headers.len()]
    };

    let mut new_row = template_row;
    while new_row.len() < table.headers.len() {
        new_row.push(String::new());
    }

    for cell in &mut new_row {
        if let Some(inc) = try_increment_string(cell) {
            *cell = inc;
        }
    }

    table.rows.push(new_row);
    table.is_dirty = true;
    table.rows.len() - 1
}

fn try_increment_string(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }

    // Check for IP address e.g. "192.168.1.10"
    let ip_parts: Vec<&str> = s.split('.').collect();
    if ip_parts.len() == 4 {
        if let Ok(last_num) = ip_parts[3].parse::<u32>() {
            return Some(format!("{}.{}.{}.{}", ip_parts[0], ip_parts[1], ip_parts[2], last_num + 1));
        }
    }

    // Find trailing digits
    let rev_chars: Vec<char> = s.chars().rev().collect();
    let digit_count = rev_chars.iter().take_while(|c| c.is_ascii_digit()).count();

    if digit_count > 0 {
        let split_pos = s.len() - digit_count;
        let prefix = &s[..split_pos];
        let num_str = &s[split_pos..];
        if let Ok(num) = num_str.parse::<u64>() {
            let next_num = num + 1;
            let formatted_num = format!("{:0width$}", next_num, width = digit_count);
            return Some(format!("{}{}", prefix, formatted_num));
        }
    }

    None
}

/// Clipboard helper: Formats selected or all visible rows as TSV or CSV
pub fn export_to_clipboard_string(
    table: &TableData,
    row_indices: &[usize],
    delimiter: char,
    include_headers: bool,
) -> String {
    let mut out = String::new();
    let rows_to_export: Vec<usize> = if row_indices.is_empty() {
        (0..table.rows.len()).collect()
    } else {
        row_indices.to_vec()
    };

    if include_headers {
        out.push_str(&table.headers.join(&delimiter.to_string()));
        out.push('\n');
    }

    for &r in &rows_to_export {
        if let Some(row) = table.rows.get(r) {
            out.push_str(&row.join(&delimiter.to_string()));
            out.push('\n');
        }
    }

    out
}

/// Clipboard helper: Parses clipboard string (auto-detects Tab or Comma) and inserts/pastes into table
pub fn paste_clipboard_data(
    table: &mut TableData,
    text: &str,
    start_row: usize,
    start_col: usize,
) {
    let delim = if text.contains('\t') { '\t' } else { ',' };
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();

    for (row_offset, line) in lines.iter().enumerate() {
        let target_r = start_row + row_offset;
        let parts: Vec<&str> = line.split(delim).collect();

        if target_r >= table.rows.len() {
            table.rows.push(vec![String::new(); table.headers.len()]);
        }

        for (col_offset, &part) in parts.iter().enumerate() {
            let target_c = start_col + col_offset;
            if target_c < table.headers.len() {
                table.set_cell(target_r, target_c, part.trim().to_string());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableDiffCell {
    pub col_name: String,
    pub source_val: String,
    pub target_val: String,
}

#[derive(Debug, Clone)]
pub enum TableDiffRow {
    Added { row_idx: usize, row_data: Vec<String> },
    Deleted { row_idx: usize, row_data: Vec<String> },
    Modified { row_idx: usize, cell_diffs: Vec<TableDiffCell> },
    Unchanged { row_idx: usize },
}

#[derive(Debug, Clone)]
pub struct TableDiffReport {
    pub added_count: usize,
    pub deleted_count: usize,
    pub modified_count: usize,
    pub rows: Vec<TableDiffRow>,
}

pub fn compare_tables(source: &TableData, target: &TableData, key_col: Option<&str>) -> TableDiffReport {
    let mut rows = Vec::new();
    let mut added = 0;
    let mut deleted = 0;
    let mut modified = 0;

    let src_key_idx = key_col.and_then(|k| source.find_col_index(k));
    let tgt_key_idx = key_col.and_then(|k| target.find_col_index(k));

    if let (Some(sk), Some(tk)) = (src_key_idx, tgt_key_idx) {
        // Key-based matching
        let mut target_by_key: std::collections::HashMap<String, (usize, &Vec<String>)> = std::collections::HashMap::new();
        for (i, row) in target.rows.iter().enumerate() {
            let key = row.get(tk).map(|s| s.trim()).unwrap_or("");
            if !key.is_empty() {
                target_by_key.insert(key.to_string(), (i, row));
            }
        }

        let mut matched_targets: HashSet<usize> = HashSet::new();

        for (s_idx, s_row) in source.rows.iter().enumerate() {
            let s_key = s_row.get(sk).map(|s| s.trim()).unwrap_or("");
            if let Some(&(t_idx, t_row)) = target_by_key.get(s_key) {
                matched_targets.insert(t_idx);
                let mut cell_diffs = Vec::new();
                for (c_idx, h) in source.headers.iter().enumerate() {
                    let s_val = s_row.get(c_idx).map(|s| s.as_str()).unwrap_or("");
                    if let Some(t_col_idx) = target.find_col_index(h) {
                        let t_val = t_row.get(t_col_idx).map(|s| s.as_str()).unwrap_or("");
                        if s_val != t_val {
                            cell_diffs.push(TableDiffCell {
                                col_name: h.clone(),
                                source_val: s_val.to_string(),
                                target_val: t_val.to_string(),
                            });
                        }
                    }
                }

                if cell_diffs.is_empty() {
                    rows.push(TableDiffRow::Unchanged { row_idx: s_idx });
                } else {
                    modified += 1;
                    rows.push(TableDiffRow::Modified { row_idx: s_idx, cell_diffs });
                }
            } else {
                added += 1;
                rows.push(TableDiffRow::Added { row_idx: s_idx, row_data: s_row.clone() });
            }
        }

        for (t_idx, t_row) in target.rows.iter().enumerate() {
            if !matched_targets.contains(&t_idx) {
                deleted += 1;
                rows.push(TableDiffRow::Deleted { row_idx: t_idx, row_data: t_row.clone() });
            }
        }
    } else {
        // Line-by-line comparison
        let max_len = source.rows.len().max(target.rows.len());
        for i in 0..max_len {
            match (source.rows.get(i), target.rows.get(i)) {
                (Some(s_row), Some(t_row)) => {
                    let mut cell_diffs = Vec::new();
                    for (c_idx, h) in source.headers.iter().enumerate() {
                        let s_val = s_row.get(c_idx).map(|s| s.as_str()).unwrap_or("");
                        let t_val = t_row.get(c_idx).map(|s| s.as_str()).unwrap_or("");
                        if s_val != t_val {
                            cell_diffs.push(TableDiffCell {
                                col_name: h.clone(),
                                source_val: s_val.to_string(),
                                target_val: t_val.to_string(),
                            });
                        }
                    }
                    if cell_diffs.is_empty() {
                        rows.push(TableDiffRow::Unchanged { row_idx: i });
                    } else {
                        modified += 1;
                        rows.push(TableDiffRow::Modified { row_idx: i, cell_diffs });
                    }
                }
                (Some(s_row), None) => {
                    added += 1;
                    rows.push(TableDiffRow::Added { row_idx: i, row_data: s_row.clone() });
                }
                (None, Some(t_row)) => {
                    deleted += 1;
                    rows.push(TableDiffRow::Deleted { row_idx: i, row_data: t_row.clone() });
                }
                (None, None) => {}
            }
        }
    }

    TableDiffReport {
        added_count: added,
        deleted_count: deleted,
        modified_count: modified,
        rows,
    }
}
