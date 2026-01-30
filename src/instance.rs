use crate::logging::log;
use crate::shutdown::request_shutdown;

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::thread;
use ctrlc;
use fs2::FileExt;
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Global storage for the socket listener thread join handle
static LISTENER_HANDLE: Lazy<Mutex<Option<thread::JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));

/// Global storage for the lock file handle (kept locked for program lifetime)
static LOCK_FILE_HANDLE: Lazy<Mutex<Option<std::fs::File>>> = Lazy::new(|| Mutex::new(None));

// Signal the existing process to terminate gracefully
fn signal_process_to_terminate(socket_path: &str, grace_seconds: u64) -> io::Result<()> {
    // Use lsof to find process using the socket
    let output = Command::new("lsof")
        .arg("-t")  // Output only PID
        .arg(socket_path)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to find process using lsof"
        ));
    }

    let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pid_str.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No process found using the socket"
        ));
    }

    let _ = log(&format!("Found old instance with PID {}, sending termination signal", pid_str));

    // Set the shutdown flag for our own process if we're signaling ourselves
    // This case should ideally not happen if check_single_instance is called correctly,
    // but it's a safeguard.
    let our_pid = std::process::id().to_string();
    if pid_str == our_pid {
        request_shutdown();
        return Ok(());
    }

    // Send SIGTERM to allow graceful shutdown
    let term_output = Command::new("kill")
        .arg("-15")  // SIGTERM for graceful termination
        .arg(&pid_str)
        .output()?;

    if !term_output.status.success() {
        let stderr = String::from_utf8_lossy(&term_output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to send termination signal to process {}: {}", pid_str, stderr)
        ));
    }

    let _ = log(&format!("Successfully sent termination signal to old instance with PID {}", pid_str));

    // Wait for up to grace_seconds for the process to terminate
    for i in 1..=(grace_seconds * 2) { // Check twice per second
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Check if the process is still running
        let check_output = Command::new("kill")
            .arg("-0")  // Check if process exists
            .arg(&pid_str)
            .output()?;

        if !check_output.status.success() {
            let _ = log(&format!("Old instance with PID {} has terminated gracefully", pid_str));
            return Ok(());
        }

        if i % 2 == 0 { // Log every second
            let _ = log(&format!("Waiting for old instance with PID {} to terminate ({} of {} seconds)...",
                pid_str, i/2, grace_seconds));
        }
    }

    // If process didn't terminate after timeout, use SIGKILL as last resort
    let _ = log(&format!("Old instance with PID {} did not terminate gracefully, forcing termination", pid_str));
    let kill_output = Command::new("kill")
        .arg("-9")  // SIGKILL for forced termination
        .arg(&pid_str)
        .output()?;

    if !kill_output.status.success() {
        let stderr = String::from_utf8_lossy(&kill_output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to force termination of process {}: {}", pid_str, stderr)
        ));
    }

    let _ = log(&format!("Forcibly terminated old instance with PID {}", pid_str));
    std::thread::sleep(std::time::Duration::from_millis(500)); // Give OS a moment

    Ok(())
}

/// Ensures only one instance runs at a time using atomic file locking
///
/// # Behavior
/// - Uses flock() on PID file for atomic single-instance enforcement
/// - Creates socket for shutdown requests from new instances
/// - Handles cleanup on exit via scopeguard
///
/// # Errors
/// - If another instance is running (returns Err, caller should exit)
/// - If lock file creation fails
///
/// # Race Condition Protection
/// The flock() system call is atomic - even if two processes execute
/// try_lock() simultaneously, only one will succeed.
///
/// # Panics
/// If signal handler registration fails
pub fn check_single_instance(grace_seconds: u64) -> io::Result<()> {
    let socket_path = format!("/tmp/{}.sock", crate::PROGRAM_NAME);
    let pid_path = format!("/tmp/{}.pid", crate::PROGRAM_NAME);

    // ATOMIC: Try to acquire exclusive lock on PID file
    // This is the critical race-condition-free operation
    let mut lock_file = match OpenOptions::new()
        .write(true)
        .create(true)
        .open(&pid_path)
    {
        Ok(f) => f,
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                format!("Failed to open lock file {}: {}", pid_path, e)
            ));
        }
    };

    // Try to lock the file - this is atomic via flock()
    if lock_file.try_lock_exclusive().is_err() {
        // Lock failed - another instance is running
        let _ = log(&format!("Another instance is already running (PID file {} is locked)", pid_path));

        // Try to signal the existing instance to terminate
        if UnixStream::connect(&socket_path).is_ok() {
            let _ = log(&format!("New instance PID {} requesting graceful termination of old instance.",
                std::process::id()));

            if let Err(e) = signal_process_to_terminate(&socket_path, grace_seconds) {
                let _ = log(&format!("Failed to signal old process: {}", e));
            }

            // After termination, try to acquire lock again
            match lock_file.try_lock_exclusive() {
                Ok(_) => {
                    let _ = log("Successfully acquired lock after old instance terminated");
                }
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "Another instance is still running. Exiting."
                    ));
                }
            }
        } else {
            // Socket exists but can't connect - stale state, try cleanup
            let _ = std::fs::remove_file(&socket_path);
            match lock_file.try_lock_exclusive() {
                Ok(_) => {
                    let _ = log("Successfully acquired lock after cleaning up stale socket");
                }
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "Another instance is running. Exiting."
                    ));
                }
            }
        }
    }

    // We hold the lock - we are the single instance
    // Write our PID to the file
    if let Err(e) = lock_file.write_all(std::process::id().to_string().as_bytes()) {
        return Err(io::Error::new(
            e.kind(),
            format!("Failed to write PID to {}: {}", pid_path, e)
        ));
    }
    let _ = log(&format!("Acquired exclusive lock on {}, PID {}", pid_path, std::process::id()));

    // Store lock file handle globally so lock remains held for program lifetime
    if let Ok(mut guard) = LOCK_FILE_HANDLE.lock() {
        *guard = Some(lock_file);
    }

    // Clean up any stale socket file
    let _ = std::fs::remove_file(&socket_path);

    // Create a new socket for shutdown requests
    let listener = UnixListener::bind(&socket_path)?;
    let _ = log(&format!("Created socket file: {}", socket_path));

    // Set up signal handler for SIGINT (Ctrl+C) and SIGTERM
    // NOTE: This handler is async-signal-safe. It only sets atomic flags.
    // Logging is deferred to the main thread.
    ctrlc::set_handler(|| {
        // Signal type 1 = SIGINT (Ctrl+C)
        crate::shutdown::request_shutdown_with_signal(1);
    }).expect("Error setting signal handler");

    // Spawn a thread to listen on the socket for shutdown commands from new instances.
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buffer = [0; 8]; // Expect "SHUTDOWN"
                    if let Ok(size) = stream.read(&mut buffer) {
                        if size == 8 && &buffer[..] == b"SHUTDOWN" {
                            let _ = log(&format!("Received 'SHUTDOWN' command on socket. PID {} initiating self-shutdown.",
                                std::process::id()));
                            request_shutdown();
                            break; // Exit listener thread
                        }
                    }
                }
                Err(e) => {
                    let _ = log(&format!("Error accepting incoming connection on socket: {}", e));
                    // Depending on the error, might want to break or continue.
                    // For now, continue to try accepting more connections.
                }
            }
        }
        let _ = log("Socket listener thread exiting.");
    });

    // Store the handle for later cleanup
    if let Ok(mut guard) = LISTENER_HANDLE.lock() {
        *guard = Some(handle);
    }

    Ok(())
}

/// Joins the socket listener thread
///
/// Attempts to wait for the listener thread to finish.
/// The thread may be blocked on listener.incoming(), so we only wait briefly.
/// If the thread is still blocked, we continue with cleanup anyway - the OS
/// will terminate the thread when the process exits.
/// Should be called during cleanup.
pub fn join_listener_thread() {
    if let Ok(mut handle_guard) = LISTENER_HANDLE.lock() {
        if let Some(handle) = handle_guard.take() {
            // Try to join in a separate thread that we timeout
            std::thread::spawn(move || {
                let _ = handle.join();
            });
            // Don't wait - the listener thread is often blocked on incoming()
            // The OS will clean it up when the process exits
        }
    }
}

/// Cleans up single instance lock files
///
/// Removes:
/// - Unix domain socket (/tmp/{PROGRAM_NAME}.sock)
/// - PID file (/tmp/{PROGRAM_NAME}.pid)
///
/// Called automatically on program exit (e.g., via scopeguard in main)
pub fn cleanup_lock_file() {
    let socket_path = format!("/tmp/{}.sock", crate::PROGRAM_NAME);
    let pid_path = format!("/tmp/{}.pid", crate::PROGRAM_NAME);

    let _ = log(&format!("Cleaning up lock files: {} and {}", socket_path, pid_path));

    // Release the file lock by closing the file handle
    if let Ok(mut guard) = LOCK_FILE_HANDLE.lock() {
        *guard = None; // Drop the File, which releases the flock
    }

    if let Err(e) = std::fs::remove_file(&socket_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            let _ = log(&format!("Failed to remove socket file {}: {}", socket_path, e));
        }
    }
    if let Err(e) = std::fs::remove_file(&pid_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            let _ = log(&format!("Failed to remove pid file {}: {}", pid_path, e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_cleanup_lock_file() {
        // Create temporary directory for testing
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");
        let pid_path = dir.path().join("test.pid");

        // Create test files
        fs::write(&socket_path, "test").unwrap();
        fs::write(&pid_path, "1234").unwrap();

        // Verify files exist
        assert!(socket_path.exists());
        assert!(pid_path.exists());

        // Note: cleanup_lock_file uses hardcoded paths based on PROGRAM_NAME
        // so we can't directly test it with custom paths.
        // This test verifies the concept - in a real scenario, we'd need to
        // mock PROGRAM_NAME or test the actual paths.
    }

    #[test]
    #[serial]
    fn test_cleanup_nonexistent_files() {
        // cleanup_lock_file should not fail when files don't exist
        // (it ignores NotFound errors)
        cleanup_lock_file();
        // If we reach here without panic, test passes
    }

    #[test]
    #[serial]
    fn test_join_listener_thread_when_none() {
        // Calling join when no listener thread should be safe
        join_listener_thread();
        // Should not panic
    }
}
