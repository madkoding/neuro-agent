use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use lazy_static::lazy_static;
use chrono::Local;

lazy_static! {
    static ref LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
}

/// Initialize the log file
pub fn init_logger() -> anyhow::Result<()> {
    let log_path = get_log_path();
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    
    let mut log_file = LOG_FILE.lock().unwrap();
    *log_file = Some(file);
    
    // Write session start marker
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Some(ref mut f) = *log_file {
        let _ = writeln!(f, "\n=== Neuro Session Started at {} ===\n", timestamp);
    }
    
    Ok(())
}

/// Get the log file path
fn get_log_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("neuro").join("neuro.log")
    } else {
        PathBuf::from("neuro.log")
    }
}

/// Log a message to file
pub fn log(level: &str, message: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let formatted = format!("[{}] {}: {}", timestamp, level, message);
    
    let mut log_file = LOG_FILE.lock().unwrap();
    if let Some(ref mut f) = *log_file {
        let _ = writeln!(f, "{}", formatted);
        let _ = f.flush();
    }
}

/// Macros for easier logging
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::log("INFO", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logging::log("DEBUG", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logging::log("WARN", &format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::log("ERROR", &format!($($arg)*));
    };
}

/// Get the current log file path for display
pub fn get_log_path_display() -> String {
    get_log_path().display().to_string()
}
