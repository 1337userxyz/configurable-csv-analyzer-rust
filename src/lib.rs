pub mod core;

use std::collections::HashMap;
use std::path::PathBuf;

/// Gibt den absoluten Pfad zur Slint-Bibliotheksdatei (`ui/index.slint`) zurück.
pub fn library_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join("index.slint")
}

/// Gibt die Library-Paths-Map für `slint_build::CompilerConfiguration::with_library_paths` zurück.
/// Kombiniert `@kit-theme` und `@csv-analyzer`, sodass Host-Apps beide Bibliotheken direkt importieren können:
/// ```slint
/// import { CsvTableView, InfoFileEditorWindow } from "@csv-analyzer";
/// ```
pub fn library_paths() -> HashMap<String, PathBuf> {
    let mut map = kit_theme::library_paths();
    map.insert("csv-analyzer".to_string(), library_path());
    map
}

/// Standardisierter Slint-Build für Host-Apps, die `@csv-analyzer` (und `@kit-theme`) einbetten:
/// - Registriert `@kit-theme` und `@csv-analyzer`
/// - Setzt `"fluent"` Style
/// - Setzt `cargo:rerun-if-changed` für `entry_slint`, `@kit-theme` und `@csv-analyzer`
/// - Linkt unter Linux `dbus-1`
#[cfg(feature = "build")]
pub fn compile_slint(entry_slint: impl AsRef<std::path::Path>) {
    let entry = entry_slint.as_ref();
    println!("cargo:rerun-if-changed={}", entry.display());
    if let Some(theme_dir) = kit_theme::library_path().parent() {
        println!("cargo:rerun-if-changed={}", theme_dir.display());
    }
    if let Some(ui_dir) = library_path().parent() {
        println!("cargo:rerun-if-changed={}", ui_dir.display());
    }

    let is_linux = std::env::var("CARGO_CFG_TARGET_OS")
        .map(|v| v == "linux")
        .unwrap_or(cfg!(target_os = "linux"));
    if is_linux {
        println!("cargo:rustc-link-lib=dbus-1");
    }

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths(library_paths())
        .with_style("fluent".into());
    slint_build::compile_with_config(entry, config).expect("Slint compilation failed");
}
