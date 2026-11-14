use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use crate::core::model::{ColumnSetting, TableData};

pub use kit_csv::{decode_bytes, sniff_delimiter};

pub fn load_csv(
    path: &Path,
    delimiter: Option<char>,
    encoding: Option<&str>,
    has_header: bool,
) -> Result<TableData, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    let delim = delimiter.unwrap_or_else(|| sniff_delimiter(&bytes));
    let (text, detected_enc) = decode_bytes(&bytes, encoding);

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim as u8)
        .has_headers(has_header)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut headers = Vec::new();
    if has_header {
        let hdr_record = rdr.headers()?;
        for field in hdr_record {
            headers.push(field.to_string());
        }
    }

    let mut rows = Vec::new();
    let mut max_cols = headers.len();

    for result in rdr.records() {
        let record = result?;
        let row_vec: Vec<String> = record.iter().map(|s| s.to_string()).collect();
        if row_vec.len() > max_cols {
            max_cols = row_vec.len();
        }
        rows.push(row_vec);
    }

    // If no header, synthesize Column 1, Column 2, ...
    if headers.is_empty() {
        for i in 0..max_cols {
            headers.push(format!("Column {}", i + 1));
        }
    }

    // Pad rows so all rows have at least headers.len() cells
    for row in &mut rows {
        if row.len() < headers.len() {
            row.resize(headers.len(), String::new());
        }
    }

    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Table");
    let mut table = TableData::new(path.to_string_lossy().to_string(), file_stem.to_string());
    table.file_path = Some(path.to_path_buf());
    table.headers = headers.clone();
    table.rows = rows;
    table.delimiter = delim;
    table.encoding = detected_enc;
    table.is_virtual_col = vec![false; headers.len()];

    table.column_settings = headers
        .into_iter()
        .enumerate()
        .map(|(i, h)| ColumnSetting {
            column_name: h,
            width: 150,
            is_visible: true,
            display_order: i as u32,
        })
        .collect();

    Ok(table)
}

pub fn save_csv(
    path: &Path,
    table: &TableData,
    delimiter: char,
    encoding: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = Vec::new();
    {
        let mut wtr = csv::WriterBuilder::new()
            .delimiter(delimiter as u8)
            .from_writer(&mut buf);

        // Filter out virtual columns when saving raw CSV
        let export_cols: Vec<usize> = table
            .headers
            .iter()
            .enumerate()
            .filter(|(idx, _)| !table.is_virtual_col.get(*idx).copied().unwrap_or(false))
            .map(|(idx, _)| idx)
            .collect();

        let header_row: Vec<&str> = export_cols
            .iter()
            .map(|&idx| table.headers[idx].as_str())
            .collect();
        wtr.write_record(&header_row)?;

        for row in &table.rows {
            let row_record: Vec<&str> = export_cols
                .iter()
                .map(|&idx| row.get(idx).map(|s| s.as_str()).unwrap_or(""))
                .collect();
            wtr.write_record(&row_record)?;
        }
        wtr.flush()?;
    }

    let output_bytes = if encoding.eq_ignore_ascii_case("windows-1252") || encoding.eq_ignore_ascii_case("ansi") {
        let text = String::from_utf8_lossy(&buf);
        let (cow, _, _) = encoding_rs::WINDOWS_1252.encode(&text);
        cow.into_owned()
    } else if encoding.eq_ignore_ascii_case("utf-8-bom") {
        let mut out = vec![0xEF, 0xBB, 0xBF];
        out.extend_from_slice(&buf);
        out
    } else {
        buf
    };

    let mut f = File::create(path)?;
    f.write_all(&output_bytes)?;
    Ok(())
}

pub fn preview_csv(
    path: &Path,
    delimiter: Option<char>,
    encoding: Option<&str>,
    max_rows: usize,
) -> Result<(char, String, Vec<String>, Vec<Vec<String>>), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    let delim = delimiter.unwrap_or_else(|| sniff_delimiter(&bytes));
    let (text, detected_enc) = decode_bytes(&bytes, encoding);

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim as u8)
        .has_headers(true)
        .flexible(true)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut preview_rows = Vec::new();

    for result in rdr.records().take(max_rows) {
        let record = result?;
        preview_rows.push(record.iter().map(|s| s.to_string()).collect());
    }

    Ok((delim, detected_enc, headers, preview_rows))
}

pub fn find_all_csvs_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut res = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension() {
                    if ext.eq_ignore_ascii_case("csv") || ext.eq_ignore_ascii_case("tsv") {
                        res.push(p);
                    }
                }
            }
        }
    }
    res
}
