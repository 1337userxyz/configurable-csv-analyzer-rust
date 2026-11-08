//! Logging-Modul für den CSV-Analyzer (basierend auf `kit-logging`).

pub use kit_logging::{log_debug, log_error, log_info, log_warn, AppLogger as LogBuffer, LogEntry, LogLevel};

use std::sync::OnceLock;

static GLOBAL_LOG_BUFFER: OnceLock<kit_logging::AppLogger> = OnceLock::new();

pub fn get_global_logger() -> &'static kit_logging::AppLogger {
    GLOBAL_LOG_BUFFER.get_or_init(|| {
        let log_file_path = crate::core::io::info::get_base_project_dir().join("analyzer.log");
        let _ = kit_logging::init_logger(Some(log_file_path.clone()), 1000, log::LevelFilter::Debug);
        kit_logging::AppLogger::new(Some(log_file_path), 1000)
    })
}
