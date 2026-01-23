use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use lazy_static::lazy_static;
use chrono::Local;
use std::thread;

lazy_static! {
    static ref LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
    static ref DEBUG_MODE: Mutex<bool> = Mutex::new(false);
    static ref VERBOSE_LOGGING: Mutex<bool> = Mutex::new(false);
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

    // Write session start marker with detailed info
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    if let Some(ref mut f) = *log_file {
        let _ = writeln!(f, "\n\n╔════════════════════════════════════════════════════════════════════╗");
        let _ = writeln!(f, "║ NEURO SESSION STARTED");
        let _ = writeln!(f, "║ Timestamp: {}", timestamp);
        let _ = writeln!(f, "║ Log Path: {}", log_path.display());
        let _ = writeln!(f, "║ Thread: {:?}", thread::current().id());
        let _ = writeln!(f, "╚════════════════════════════════════════════════════════════════════╝\n", );
        let _ = f.flush();
    }

    // Enable verbose logging to file (always on, not affected by RUST_LOG)
    let mut verbose = VERBOSE_LOGGING.lock().unwrap();
    *verbose = true;

    Ok(())
}

/// Set debug mode for console logging
pub fn set_debug_mode(debug: bool) {
    let mut debug_mode = DEBUG_MODE.lock().unwrap();
    *debug_mode = debug;
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
    let thread_name = thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string();
    let thread_id = format!("{:?}", thread::current().id());

    // Detailed log format for file
    let formatted = format!(
        "[{}] [{}] [Thread: {:<12} ID: {}] {}: {}",
        timestamp,
        level,
        thread_name,
        thread_id,
        level,
        message
    );

    // Write to file (always enabled)
    let mut log_file = LOG_FILE.lock().unwrap();
    if let Some(ref mut f) = *log_file {
        let _ = writeln!(f, "{}", formatted);
        let _ = f.flush();
    }

    // Also log to console if in debug mode (less verbose)
    let debug_mode = DEBUG_MODE.lock().unwrap();
    if *debug_mode {
        let console_format = format!("[{}] {}: {}", timestamp, level, message);
        eprintln!("{}", console_format);
    }
}

/// Macros for easier logging
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        $crate::logging::log("INFO", &format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        $crate::logging::log("DEBUG", &format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        $crate::logging::log("WARN", &format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        $crate::logging::log("ERROR", &format!($($arg)*))
    }};
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {{
        $crate::logging::log("TRACE", &format!($($arg)*))
    }};
}

/// Log detailed timing information
pub fn log_timing(label: &str, elapsed_ms: u128) {
    log("TIMING", &format!("{}: {}ms", label, elapsed_ms));
}

/// Log event information
pub fn log_event(event_type: &str, details: &str) {
    log("EVENT", &format!("[{}] {}", event_type, details));
}

/// Get the current log file path for display
pub fn get_log_path_display() -> String {
    get_log_path().display().to_string()
}
