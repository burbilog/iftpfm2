use chrono::Local;
use once_cell::sync::Lazy;
use std::fs::OpenOptions; // Removed 'File'
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

// LOG_FILE is a thread-safe, lazily initialized global variable
// It holds an Option<String> representing the path to the log file (if set)
// The Mutex ensures thread-safe access to this value
/// Global log file path protected by Mutex
///
/// Thread-safe storage for optional log file path.
/// When None, logs go to stdout.
pub static LOG_FILE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Logs a message to either a file or stdout
///
/// This function takes a message as input and logs it with a timestamp.
/// If a log file has been set (using set_log_file), the message is appended to that file.
/// Otherwise, the message is printed to stdout.
///
/// # Arguments
///
/// * `message` - The message to be logged
///
/// # Returns
///
/// * `io::Result<()>` - Ok if the logging was successful, Err otherwise
/// Logs a message with timestamp to configured output
///
/// # Arguments
/// * `message` - The message to log
///
/// # Returns
/// * `io::Result<()>` - Ok on success, Err if writing fails
///
/// # Example
/// ```text
/// // log("Starting transfer").unwrap();
/// ```
pub fn log(message: &str) -> io::Result<()> {
    log_with_thread(message, None)
}

/// Logs a message with timestamp and optional thread ID
///
/// Used when running in parallel mode to distinguish threads
///
/// # Arguments
/// * `message` - The message to log
/// * `thread_id` - Optional thread identifier
///
/// # Returns
/// * `io::Result<()>` - Ok on success, Err if writing fails
///
/// # Example
/// ```text
/// // log_with_thread("Thread started", Some(1)).unwrap();
/// ```
pub fn log_with_thread(message: &str, thread_id: Option<usize>) -> io::Result<()> {
    // Generate a timestamp for the log message
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_message = match thread_id {
        Some(tid) => format!("{} [T{}] {}\n", timestamp, tid, message),
        None => format!("{} {}\n", timestamp, message),
    };

    // Lock the mutex and check if a log file has been set
    match &*LOG_FILE.lock().unwrap() {
        Some(log_file) => {
            // If a log file is set, append the message to the file
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)?;
            file.write_all(log_message.as_bytes())?;
        }
        None => {
            // If no log file is set, print the message to stdout.
            // The original code used println!() with a message already ending in \n,
            // resulting in a double newline. Restoring that behavior.
            println!("{}", log_message);
        }
    }

    Ok(())
}

/// Sets the path for the log file
///
/// This function updates the global LOG_FILE variable with the provided path.
/// Subsequent calls to the log function will write to this file.
///
/// # Arguments
///
/// * `path` - A path-like object representing the location of the log file
pub fn set_log_file<P: AsRef<Path>>(path: P) {
    // Convert the path to a string and update the LOG_FILE
    let path_str = path.as_ref().to_str().expect("Path is not valid UTF-8");
    *LOG_FILE.lock().unwrap() = Some(path_str.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    #[serial]
    fn test_log_to_file() {
        // Reset LOG_FILE before test to ensure clean state
        *LOG_FILE.lock().unwrap() = None;

        let dir = tempdir().unwrap();
        let log_file_path = dir.path().join("test.log");

        set_log_file(&log_file_path);
        log("test message 1").unwrap();
        log_with_thread("test message 2", Some(1)).unwrap();

        let log_contents = fs::read_to_string(&log_file_path).unwrap();
        assert!(log_contents.contains("test message 1"));
        assert!(log_contents.contains("[T1] test message 2"));

        // Reset LOG_FILE for other tests
        *LOG_FILE.lock().unwrap() = None;
        // tempdir is automatically cleaned up when it goes out of scope
    }

    #[test]
    #[serial]
    fn test_log_to_stdout() {
        // Reset LOG_FILE before test to ensure clean state
        *LOG_FILE.lock().unwrap() = None;

        // This test is harder to verify automatically without capturing stdout.
        // For now, we'll just call it to ensure it doesn't panic.
        // Manual verification or a more sophisticated test setup would be needed.
        log("test stdout message 1").unwrap();
        log_with_thread("test stdout message 2", Some(2)).unwrap();
        // If we reach here, it means no panic occurred.
    }
}
