use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use chrono::Local;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    pub target: String,
}

#[derive(Clone)]
pub struct LogBuffer {
    entries: Arc<Mutex<Vec<LogEntry>>>,
    max_entries: usize,
    log_file_path: PathBuf,
}

impl LogBuffer {
    pub fn new(max_entries: usize) -> Self {
        let log_file_path = crate::core::io::info::get_base_project_dir().join("analyzer.log");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_file_path) {
            let now = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "\n=== Session gestartet: {} ===", now);
        }
        Self {
            entries: Arc::new(Mutex::new(Vec::with_capacity(max_entries))),
            max_entries,
            log_file_path,
        }
    }

    pub fn add(&self, level: LogLevel, target: &str, message: &str) {
        let now = Local::now();
        let entry = LogEntry {
            timestamp: now.format("%H:%M:%S%.3f").to_string(),
            level: level.clone(),
            message: message.to_string(),
            target: target.to_string(),
        };

        // Write to analyzer.log in project directory
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.log_file_path) {
            let full_ts = now.format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] [{}] {}: {}", full_ts, level.as_str(), target, message);
        }

        if let Ok(mut lock) = self.entries.lock() {
            if lock.len() >= self.max_entries {
                lock.remove(0);
            }
            lock.push(entry);
        }
    }

    pub fn get_entries(&self, min_level: Option<LogLevel>) -> Vec<LogEntry> {
        if let Ok(lock) = self.entries.lock() {
            lock.iter()
                .filter(|e| match &min_level {
                    None => true,
                    Some(lvl) => match (lvl, &e.level) {
                        (LogLevel::Trace, _) => true,
                        (LogLevel::Debug, LogLevel::Trace) => false,
                        (LogLevel::Debug, _) => true,
                        (LogLevel::Info, LogLevel::Trace | LogLevel::Debug) => false,
                        (LogLevel::Info, _) => true,
                        (LogLevel::Warn, LogLevel::Warn | LogLevel::Error) => true,
                        (LogLevel::Warn, _) => false,
                        (LogLevel::Error, LogLevel::Error) => true,
                        (LogLevel::Error, _) => false,
                    },
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn format_text(&self, min_level: Option<LogLevel>) -> String {
        let entries = self.get_entries(min_level);
        entries
            .iter()
            .map(|e| format!("[{}] [{}] {}: {}", e.timestamp, e.level.as_str(), e.target, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn clear(&self) {
        if let Ok(mut lock) = self.entries.lock() {
            lock.clear();
        }
    }
}

static GLOBAL_LOG_BUFFER: OnceLock<LogBuffer> = OnceLock::new();

pub fn get_global_logger() -> &'static LogBuffer {
    GLOBAL_LOG_BUFFER.get_or_init(|| LogBuffer::new(1000))
}

pub fn log_info(target: &str, msg: &str) {
    get_global_logger().add(LogLevel::Info, target, msg);
}

pub fn log_warn(target: &str, msg: &str) {
    get_global_logger().add(LogLevel::Warn, target, msg);
}

pub fn log_error(target: &str, msg: &str) {
    get_global_logger().add(LogLevel::Error, target, msg);
}

pub fn log_debug(target: &str, msg: &str) {
    get_global_logger().add(LogLevel::Debug, target, msg);
}
