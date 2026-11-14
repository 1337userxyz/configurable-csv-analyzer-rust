//! Logging-Modul für den CSV-Analyzer (basierend auf `kit-logging`).

pub use kit_logging::{log_debug, log_error, log_info, log_warn, AppLogger as LogBuffer, LogEntry, LogLevel};

use std::sync::Once;

static INIT: Once = Once::new();

pub fn get_global_logger() -> &'static kit_logging::AppLogger {
    INIT.call_once(|| {
        let log_file_path = crate::core::io::info::get_base_project_dir().join("analyzer.log");
        let _ = kit_logging::init_logger(Some(log_file_path), 1000, log::LevelFilter::Debug);
    });
    kit_logging::get_logger()
}
