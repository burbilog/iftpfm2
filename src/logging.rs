use chrono::Local;
use once_cell::sync::Lazy;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
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

/// Global cached file handle protected by Mutex
///
/// Thread-safe storage for optional buffered writer to log file.
/// When None, either no log file is set or we haven't opened it yet.
static LOG_FILE_HANDLE: Lazy<Mutex<Option<BufWriter<File>>>> = Lazy::new(|| Mutex::new(None));

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
    // Handle poisoned mutex by recovering or using a fallback
    let log_file_result = LOG_FILE.lock();
    let log_file_guard = match log_file_result {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Recover from poisoned mutex, taking the value
            poisoned.into_inner()
        }
    };

    // Clone the log_file path so we can drop the guard before locking LOG_FILE_HANDLE
    let log_file_clone = log_file_guard.as_ref().cloned();
    drop(log_file_guard);

    if let Some(log_file) = log_file_clone {
        // Lock the file handle mutex, handling poisoning
        let handle_result = LOG_FILE_HANDLE.lock();
        let mut handle_guard: std::sync::MutexGuard<'_, Option<BufWriter<File>>> = match handle_result {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // If handle is not yet opened or was closed, open it
        if handle_guard.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)?;
            *handle_guard = Some(BufWriter::new(file));
        }

        // Write to the cached handle
        if let Some(ref mut writer) = *handle_guard {
            writer.write_all(log_message.as_bytes())?;
            writer.flush()?;
        }
    } else {
        // If no log file is set, print the message to stdout.
        // The original code used println!() with a message already ending in \n,
        // resulting in a double newline. Restoring that behavior.
        println!("{}", log_message);
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

    // Update the log file path, handling poisoned mutex
    let result = LOG_FILE.lock();
    let mut guard = match result {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = Some(path_str.to_string());
    drop(guard);

    // Clear any cached file handle since the path has changed
    let result = LOG_FILE_HANDLE.lock();
    let mut handle_guard: std::sync::MutexGuard<'_, Option<BufWriter<File>>> = match result {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *handle_guard = None;
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
        // Reset LOG_FILE and LOG_FILE_HANDLE before test to ensure clean state
        *LOG_FILE.lock().unwrap() = None;
        *LOG_FILE_HANDLE.lock().unwrap() = None;

        let dir = tempdir().unwrap();
        let log_file_path = dir.path().join("test.log");

        set_log_file(&log_file_path);
        log("test message 1").unwrap();
        log_with_thread("test message 2", Some(1)).unwrap();

        let log_contents = fs::read_to_string(&log_file_path).unwrap();
        assert!(log_contents.contains("test message 1"));
        assert!(log_contents.contains("[T1] test message 2"));

        // Reset LOG_FILE and LOG_FILE_HANDLE for other tests
        *LOG_FILE.lock().unwrap() = None;
        *LOG_FILE_HANDLE.lock().unwrap() = None;
        // tempdir is automatically cleaned up when it goes out of scope
    }

    #[test]
    #[serial]
    fn test_log_to_stdout() {
        // Reset LOG_FILE and LOG_FILE_HANDLE before test to ensure clean state
        *LOG_FILE.lock().unwrap() = None;
        *LOG_FILE_HANDLE.lock().unwrap() = None;

        // This test is harder to verify automatically without capturing stdout.
        // For now, we'll just call it to ensure it doesn't panic.
        // Manual verification or a more sophisticated test setup would be needed.
        log("test stdout message 1").unwrap();
        log_with_thread("test stdout message 2", Some(2)).unwrap();
        // If we reach here, it means no panic occurred.
    }
}
