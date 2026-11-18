use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use crate::core::model::{AppConfig, InfoFile, InfoFileStorageMode, TableData, ViewsFile};

static BASE_PROJECT_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn get_base_project_dir() -> PathBuf {
    BASE_PROJECT_DIR
        .get_or_init(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .clone()
}

pub fn default_config_path() -> PathBuf {
    kit_fs::appdirs::resolve_config("configurable-csv-analyzer", "config.json")
}

pub fn default_views_path() -> PathBuf {
    kit_fs::appdirs::resolve_config("configurable-csv-analyzer", "views.json")
}

pub fn load_app_config(path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let config: AppConfig = serde_json::from_str(&s)?;
    Ok(config)
}

pub fn save_app_config(path: &Path, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let s = serde_json::to_string_pretty(config)?;
    let mut f = File::create(path)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

pub fn default_info_path_for_table(table_path: &Path) -> PathBuf {
    let mut file_name = table_path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    file_name.push_str(".info");
    table_path.with_file_name(file_name)
}

pub fn load_info_file(path: &Path) -> Result<InfoFile, Box<dyn std::error::Error>> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let info: InfoFile = serde_json::from_str(&s)?;
    Ok(info)
}

pub fn save_info_file(path: &Path, info: &InfoFile) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let s = serde_json::to_string_pretty(info)?;
    let mut f = File::create(path)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

/// Steuert unabhängig von einer globalen Analyzer-Config, wo `.info`-Dateien gesucht und gespeichert werden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoStoragePolicy {
    /// Sucht und speichert `<csv_pfad>.info` direkt neben der jeweiligen CSV-Datei.
    BesideCsv,
    /// Sucht und speichert `<project_dir>/<csv_dateiname>.info` im angegebenen Projektordner.
    ProjectDir(PathBuf),
    /// Sucht zuerst lokal neben der CSV (`<csv_pfad>.info`), danach in `project_dir`.
    /// Speichert je nach `prefer_save_local` lokal neben der CSV oder in `project_dir`.
    ProjectWithLocalFallback {
        project_dir: PathBuf,
        prefer_save_local: bool,
    },
    /// Mehrstufige Suche für den Standalone CSV-Analyzer (Kollektions-Basispfade / explizite Mappings -> lokal -> Zentralordner).
    AnalyzerLookup {
        collection_base_dirs: Vec<PathBuf>,
        explicit_mappings: Vec<(PathBuf, PathBuf)>,
        central_dir: PathBuf,
    },
}

impl Default for InfoStoragePolicy {
    fn default() -> Self {
        Self::BesideCsv
    }
}

impl InfoStoragePolicy {
    fn info_file_name(csv_path: &Path) -> String {
        csv_path
            .file_name()
            .map(|f| format!("{}.info", f.to_string_lossy()))
            .unwrap_or_else(|| "table.csv.info".to_string())
    }

    pub fn resolve_existing_info_path(&self, csv_path: &Path) -> Option<PathBuf> {
        match self {
            InfoStoragePolicy::BesideCsv => {
                let p = default_info_path_for_table(csv_path);
                p.exists().then_some(p)
            }
            InfoStoragePolicy::ProjectDir(dir) => {
                let p = dir.join(Self::info_file_name(csv_path));
                p.exists().then_some(p)
            }
            InfoStoragePolicy::ProjectWithLocalFallback { project_dir, .. } => {
                let local = default_info_path_for_table(csv_path);
                if local.exists() {
                    return Some(local);
                }
                let proj = project_dir.join(Self::info_file_name(csv_path));
                proj.exists().then_some(proj)
            }
            InfoStoragePolicy::AnalyzerLookup {
                collection_base_dirs,
                explicit_mappings,
                central_dir,
            } => {
                let fname = Self::info_file_name(csv_path);
                for bp in collection_base_dirs {
                    let candidate = bp.join(&fname);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
                let csv_str = csv_path.to_string_lossy();
                for (mapped_csv, mapped_info) in explicit_mappings {
                    if (mapped_csv == csv_path
                        || mapped_csv
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&csv_str))
                        && mapped_info.exists()
                    {
                        return Some(mapped_info.clone());
                    }
                }
                let local = default_info_path_for_table(csv_path);
                if local.exists() {
                    return Some(local);
                }
                let central = central_dir.join(fname);
                central.exists().then_some(central)
            }
        }
    }

    pub fn resolve_save_info_path(&self, csv_path: &Path) -> PathBuf {
        match self {
            InfoStoragePolicy::BesideCsv => default_info_path_for_table(csv_path),
            InfoStoragePolicy::ProjectDir(dir) => dir.join(Self::info_file_name(csv_path)),
            InfoStoragePolicy::ProjectWithLocalFallback {
                project_dir,
                prefer_save_local,
            } => {
                if *prefer_save_local {
                    default_info_path_for_table(csv_path)
                } else {
                    project_dir.join(Self::info_file_name(csv_path))
                }
            }
            InfoStoragePolicy::AnalyzerLookup {
                collection_base_dirs,
                explicit_mappings,
                ..
            } => {
                let csv_str = csv_path.to_string_lossy();
                for (mapped_csv, mapped_info) in explicit_mappings {
                    if mapped_csv == csv_path
                        || mapped_csv
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&csv_str)
                    {
                        return mapped_info.clone();
                    }
                }
                if let Some(existing) = self.resolve_existing_info_path(csv_path) {
                    return existing;
                }
                if let Some(first_base) = collection_base_dirs.first() {
                    return first_base.join(Self::info_file_name(csv_path));
                }
                default_info_path_for_table(csv_path)
            }
        }
    }

    pub fn load_info_for_table(
        &self,
        table: &mut TableData,
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        let Some(csv_path) = table.file_path.as_deref() else {
            return Ok(None);
        };
        let Some(info_path) = self.resolve_existing_info_path(csv_path) else {
            return Ok(None);
        };
        let info = load_info_file(&info_path)?;
        apply_info_to_table(table, &info);
        Ok(Some(info_path))
    }

    pub fn save_info_for_table(
        &self,
        table: &TableData,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let csv_path = table
            .file_path
            .as_deref()
            .ok_or("Tabelle hat keinen Dateipfad für die .info-Datei.")?;
        let target_path = self.resolve_save_info_path(csv_path);
        let info = build_info_from_table(table);
        save_info_file(&target_path, &info)?;
        Ok(target_path)
    }
}

pub fn apply_info_to_table(table: &mut TableData, info: &InfoFile) {
    table.column_mappings = info.column_mappings.clone();
    if !info.column_settings.is_empty() {
        table.column_settings = info.column_settings.clone();
    }
    table.saved_filter_presets = info.saved_filter_presets.clone();
    table.rules = info.rules.clone();
}

pub fn build_info_from_table(table: &TableData) -> InfoFile {
    InfoFile {
        info_file_version: 1,
        csv_settings: crate::core::model::CsvSettings {
            file_name: table.file_path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
            delimiter: table.delimiter.to_string(),
            has_header: true,
            encoding: table.encoding.clone(),
        },
        column_mappings: table.column_mappings.clone(),
        column_settings: table.column_settings.clone(),
        saved_filter_presets: table.saved_filter_presets.clone(),
        rules: table.rules.clone(),
        dirty_cell_brush_key: None,
        storage_mode_override: None,
    }
}

pub fn load_views_file(path: &Path) -> Result<ViewsFile, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(ViewsFile::default());
    }
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let mut views: ViewsFile = serde_json::from_str(&s)?;
    for v in &mut views.views {
        let mut seen = std::collections::HashSet::new();
        v.files.retain(|f| seen.insert(f.relative_path.to_lowercase()));
    }
    Ok(views)
}

pub fn save_views_file(path: &Path, views: &ViewsFile) -> Result<(), Box<dyn std::error::Error>> {
    let s = serde_json::to_string_pretty(views)?;
    let mut f = File::create(path)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

/// Check file path integrity for CSV and Info
pub fn check_file_integrity(csv_path_str: &str, info_path_str: Option<&str>, base_dir: Option<&Path>) -> String {
    let resolve = |p_str: &str| -> PathBuf {
        let p = PathBuf::from(p_str);
        if p.is_absolute() || base_dir.is_none() {
            p
        } else {
            base_dir.unwrap().join(p)
        }
    };

    let csv_p = resolve(csv_path_str);
    if !csv_p.exists() {
        return "CSV fehlt".to_string();
    }

    let info_p = if let Some(ip) = info_path_str {
        if ip.trim().is_empty() {
            default_info_path_for_table(&csv_p)
        } else {
            resolve(ip)
        }
    } else {
        default_info_path_for_table(&csv_p)
    };

    if !info_p.exists() {
        return "Info fehlt".to_string();
    }

    "Gültig".to_string()
}

/// Move/migrate an info file between central folder, collection base path, and decentralized beside CSV
pub fn move_info_file(
    csv_path: &Path,
    current_info_path: &Path,
    target_mode: &InfoFileStorageMode,
    central_dir: &Path,
    collection_base_dir: Option<&Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let target_path = match target_mode {
        InfoFileStorageMode::KollektionsBasispfad => {
            if let Some(base) = collection_base_dir {
                if !base.exists() {
                    let _ = std::fs::create_dir_all(base);
                }
                let file_name = csv_path
                    .file_name()
                    .map(|f| format!("{}.info", f.to_string_lossy()))
                    .unwrap_or_else(|| "unnamed.csv.info".to_string());
                base.join(file_name)
            } else {
                default_info_path_for_table(csv_path)
            }
        }
        InfoFileStorageMode::Dateipfad => default_info_path_for_table(csv_path),
        InfoFileStorageMode::Stammverzeichnis => {
            if !central_dir.exists() {
                std::fs::create_dir_all(central_dir)?;
            }
            let file_name = csv_path
                .file_name()
                .map(|f| format!("{}.info", f.to_string_lossy()))
                .unwrap_or_else(|| "unnamed.csv.info".to_string());
            central_dir.join(file_name)
        }
        _ => current_info_path.to_path_buf(),
    };

    if target_path != current_info_path && current_info_path.exists() {
        if let Some(parent) = target_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::copy(current_info_path, &target_path)?;
        let _ = std::fs::remove_file(current_info_path);
    }

    Ok(target_path)
}

/// Compare two .info files and return a human-readable diff
pub fn diff_info_files(path_a: &Path, path_b: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let info_a = load_info_file(path_a)?;
    let info_b = load_info_file(path_b)?;

    let mut diff = Vec::new();
    diff.push(format!("Vergleich: {:?} <-> {:?}", path_a.file_name().unwrap_or_default(), path_b.file_name().unwrap_or_default()));
    diff.push("--------------------------------------------------".to_string());

    if info_a.csv_settings.delimiter != info_b.csv_settings.delimiter {
        diff.push(format!("Trennzeichen: '{}' vs '{}'", info_a.csv_settings.delimiter, info_b.csv_settings.delimiter));
    }
    if info_a.csv_settings.encoding != info_b.csv_settings.encoding {
        diff.push(format!("Encoding: '{}' vs '{}'", info_a.csv_settings.encoding, info_b.csv_settings.encoding));
    }

    // Mappings diff
    let keys_a: std::collections::HashSet<_> = info_a.column_mappings.keys().collect();
    let keys_b: std::collections::HashSet<_> = info_b.column_mappings.keys().collect();

    for k in keys_a.difference(&keys_b) {
        diff.push(format!("Nur in A Mapped: '{}' -> '{}'", k, info_a.column_mappings.get(*k).unwrap()));
    }
    for k in keys_b.difference(&keys_a) {
        diff.push(format!("Nur in B Mapped: '{}' -> '{}'", k, info_b.column_mappings.get(*k).unwrap()));
    }
    for k in keys_a.intersection(&keys_b) {
        let va = info_a.column_mappings.get(*k).unwrap();
        let vb = info_b.column_mappings.get(*k).unwrap();
        if va != vb {
            diff.push(format!("Mapping-Unterschied für '{}': A='{}', B='{}'", k, va, vb));
        }
    }

    // Rules count diff
    diff.push(format!("Regeln Anzahl: A={}, B={}", info_a.rules.len(), info_b.rules.len()));
    let rules_a_ids: Vec<_> = info_a.rules.iter().map(|r| r.id()).collect();
    let rules_b_ids: Vec<_> = info_b.rules.iter().map(|r| r.id()).collect();
    diff.push(format!("Regeln in A: {:?}", rules_a_ids));
    diff.push(format!("Regeln in B: {:?}", rules_b_ids));

    Ok(diff.join("\n"))
}
