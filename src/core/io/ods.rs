use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use calamine::{Data, Range, Reader};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::core::model::{ColumnSetting, TableData};

/// Überprüft, ob die Datei existiert und nicht leer (0 Bytes) ist.
/// Falls die Datei fehlt oder 0 Bytes groß ist, wird sie als gültiges ODS-Archiv initialisiert.
pub fn ensure_valid_ods_file(path: &Path, tables: &[TableData]) -> Result<bool, Box<dyn std::error::Error>> {
    let needs_init = if !path.exists() {
        true
    } else {
        std::fs::metadata(path).map(|m| m.len() == 0).unwrap_or(true)
    };

    if needs_init {
        create_ods_file(path, tables)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn list_ods_sheets(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let _ = ensure_valid_ods_file(path, &[]);
    let workbook = calamine::open_workbook_auto(path)?;
    let sheets = workbook.sheet_names();
    if sheets.is_empty() {
        create_ods_file(path, &[])?;
        let workbook = calamine::open_workbook_auto(path)?;
        Ok(workbook.sheet_names())
    } else {
        Ok(sheets)
    }
}

pub fn load_ods_sheet(path: &Path, sheet_name: Option<&str>) -> Result<TableData, Box<dyn std::error::Error>> {
    let _ = ensure_valid_ods_file(path, &[]);
    let mut workbook = calamine::open_workbook_auto(path)?;
    let mut sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        create_ods_file(path, &[])?;
        workbook = calamine::open_workbook_auto(path)?;
        sheet_names = workbook.sheet_names();
    }
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
        let mut row_vec: Vec<String> = row.iter().map(|cell| match cell {
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

        // Leere Zellen am Zeilenende entfernen
        while let Some(last) = row_vec.last() {
            if last.is_empty() {
                row_vec.pop();
            } else {
                break;
            }
        }
        all_rows.push(row_vec);
    }

    // Komplett leere Zeilen am Tabellenende entfernen
    while let Some(last_row) = all_rows.last() {
        if last_row.is_empty() || last_row.iter().all(|c| c.is_empty()) {
            all_rows.pop();
        } else {
            break;
        }
    }

    let mut headers = Vec::new();
    let mut rows = Vec::new();

    if !all_rows.is_empty() {
        headers = all_rows.remove(0);
        rows = all_rows;
    }

    // Falls das Blatt komplett leer war, Standard-Spalten bereitstellen
    if headers.is_empty() {
        headers = vec!["Spalte1".to_string(), "Spalte2".to_string(), "Spalte3".to_string()];
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
    let table_id = format!("{}::{}", path.to_string_lossy(), target_sheet);

    let mut table = TableData::new(table_id, display_name);
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

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Erstellt eine gültige OpenDocument-Spreadsheet-Datei (.ods) auf der Festplatte.
/// Wenn `tables` übergeben werden, wird jede Tabelle als eigenes Tabellenblatt gespeichert;
/// andernfalls wird ein leeres Standardblatt "Tabelle1" angelegt.
pub fn create_ods_file(path: &Path, tables: &[TableData]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);

    // 1. mimetype (must be first and uncompressed)
    let stored_opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("mimetype", stored_opts)?;
    zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")?;

    // 2. META-INF/manifest.xml
    let deflate_opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("META-INF/manifest.xml", deflate_opts)?;
    let manifest_xml = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">"#,
        r#"<manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>"#,
        r#"<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>"#,
        r#"</manifest:manifest>"#
    );
    zip.write_all(manifest_xml.as_bytes())?;

    // 3. content.xml (compact XML without whitespace text nodes for calamine + LibreOffice compatibility)
    let mut content = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2">"#,
        r#"<office:body><office:spreadsheet>"#
    ));

    if tables.is_empty() {
        content.push_str(concat!(
            r#"<table:table table:name="Tabelle1">"#,
            r#"<table:table-row>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Spalte1</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Spalte2</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Spalte3</text:p></table:table-cell>"#,
            r#"</table:table-row>"#,
            r#"</table:table>"#
        ));
    } else {
        for (idx, tbl) in tables.iter().enumerate() {
            let raw_name = if tbl.display_name.trim().is_empty() {
                format!("Tabelle{}", idx + 1)
            } else {
                tbl.display_name
                    .trim_end_matches(".csv")
                    .trim_end_matches(".ods")
                    .replace(['[', ']', '*', '?', ':', '/', '\\'], "_")
            };
            let sheet_name = xml_escape(&raw_name);
            content.push_str(&format!("<table:table table:name=\"{}\">", sheet_name));

            // Header row
            if !tbl.headers.is_empty() {
                content.push_str("<table:table-row>");
                for h in &tbl.headers {
                    let escaped = xml_escape(h);
                    content.push_str(&format!(
                        "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        escaped
                    ));
                }
                content.push_str("</table:table-row>");
            }

            // Data rows
            for row in &tbl.rows {
                content.push_str("<table:table-row>");
                for cell in row {
                    let escaped = xml_escape(cell);
                    content.push_str(&format!(
                        "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                        escaped
                    ));
                }
                content.push_str("</table:table-row>");
            }

            content.push_str("</table:table>");
        }
    }

    content.push_str("</office:spreadsheet></office:body></office:document-content>");

    zip.start_file("content.xml", deflate_opts)?;
    zip.write_all(content.as_bytes())?;
    zip.finish()?;

    Ok(())
}

pub fn find_libreoffice_binary() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/libreoffice",
        "/usr/bin/soffice",
    ];
    kit_launcher::find_binary("soffice", &candidates)
        .or_else(|| kit_launcher::find_in_path("libreoffice"))
}

pub fn open_in_libreoffice(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let soffice = find_libreoffice_binary()
        .ok_or("LibreOffice (soffice.exe) wurde nicht in Standardpfaden oder im PATH gefunden.")?;

    kit_launcher::open_in_external_app(&soffice, path, None)?;
    Ok(())
}

pub fn find_notepad_plus_plus_binary() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\Notepad++\notepad++.exe",
        r"C:\Program Files (x86)\Notepad++\notepad++.exe",
        r"C:\ProgramData\chocolatey\bin\notepad++.exe",
    ];
    kit_launcher::find_binary("notepad++", &candidates)
}

pub fn open_in_external_app(
    app: &crate::core::model::ExternalAppConfig,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let raw_cmd = app.command_or_path.trim();

    let resolved_binary = if !raw_cmd.is_empty() && Path::new(raw_cmd).exists() {
        PathBuf::from(raw_cmd)
    } else if app.name.eq_ignore_ascii_case("LibreOffice") || raw_cmd.to_ascii_lowercase().contains("soffice") {
        find_libreoffice_binary()
            .ok_or_else(|| format!("LibreOffice wurde unter '{}' oder im System nicht gefunden.", raw_cmd))?
    } else if app.name.eq_ignore_ascii_case("Notepad++") || raw_cmd.to_ascii_lowercase().contains("notepad++") {
        find_notepad_plus_plus_binary()
            .ok_or_else(|| format!("Notepad++ wurde unter '{}' oder im System nicht gefunden.", raw_cmd))?
    } else if !raw_cmd.is_empty() {
        kit_launcher::find_in_path(raw_cmd).unwrap_or_else(|| PathBuf::from(raw_cmd))
    } else {
        return Err(format!("Kein Programmpfad für '{}' konfiguriert.", app.name).into());
    };

    let args_str = app.arguments.trim();
    kit_launcher::open_in_external_app(
        &resolved_binary,
        path,
        if args_str.is_empty() { None } else { Some(args_str) },
    )?;
    Ok(())
}
