use std::path::{Path, PathBuf};
use calamine::{Reader, Data, Range};
use crate::core::model::{ColumnSetting, TableData};

pub fn list_ods_sheets(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let workbook = calamine::open_workbook_auto(path)?;
    Ok(workbook.sheet_names())
}

pub fn load_ods_sheet(path: &Path, sheet_name: Option<&str>) -> Result<TableData, Box<dyn std::error::Error>> {
    let mut workbook = calamine::open_workbook_auto(path)?;
    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err("Die ODS-Datei enthält keine Tabellenblätter".into());
    }

    let target_sheet = match sheet_name {
        Some(s) if sheet_names.iter().any(|name| name == s) => s.to_string(),
        _ => sheet_names[0].clone(),
    };

    let range: Range<Data> = workbook.worksheet_range(&target_sheet)?;

    let mut all_rows: Vec<Vec<String>> = Vec::new();
    for row in range.rows() {
        let row_vec: Vec<String> = row.iter().map(|cell| match cell {
            Data::Empty => String::new(),
            Data::String(s) => s.trim().to_string(),
            Data::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{:.0}", f)
                } else {
                    format!("{}", f)
                }
            }
            Data::Int(i) => format!("{}", i),
            Data::Bool(b) => format!("{}", b),
            Data::DateTime(d) => format!("{}", d),
            Data::DateTimeIso(s) => s.clone(),
            Data::DurationIso(s) => s.clone(),
            Data::Error(e) => format!("ERR: {:?}", e),
        }).collect();
        all_rows.push(row_vec);
    }

    let mut headers = Vec::new();
    let mut rows = Vec::new();

    if !all_rows.is_empty() {
        headers = all_rows.remove(0);
        rows = all_rows;
    }

    let max_cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    while headers.len() < max_cols {
        headers.push(format!("Column {}", headers.len() + 1));
    }

    for r in &mut rows {
        if r.len() < headers.len() {
            r.resize(headers.len(), String::new());
        }
    }

    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Table");
    let display_name = format!("{} [{}]", file_stem, target_sheet);

    let mut table = TableData::new(path.to_string_lossy().to_string(), display_name);
    table.file_path = Some(path.to_path_buf());
    table.headers = headers.clone();
    table.rows = rows;
    table.is_virtual_col = vec![false; headers.len()];
    table.is_ods = true;
    table.sheet_name = Some(target_sheet);
    table.available_sheets = sheet_names;

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

pub fn find_libreoffice_binary() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
    ];

    for path_str in candidates {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return Some(p);
        }
    }

    // Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join("soffice.exe");
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

pub fn open_in_libreoffice(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let soffice = find_libreoffice_binary()
        .ok_or("LibreOffice (soffice.exe) wurde nicht in Standardpfaden oder im PATH gefunden.")?;

    std::process::Command::new(soffice)
        .arg(path)
        .spawn()?;

    Ok(())
}
