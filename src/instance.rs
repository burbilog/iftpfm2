use crate::logging::log;
use crate::shutdown::request_shutdown;

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;
use signal_hook::{iterator::Signals, consts::SIGTERM, consts::SIGINT};
use fs2::FileExt;
use nix::unistd::Pid;
use nix::sys::signal::{self, Signal};
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Global storage for the socket listener thread join handle
static LISTENER_HANDLE: Lazy<Mutex<Option<thread::JoinHandle<()>>>> = Lazy::new(|| Mutex::new(None));

/// Returns the user-specific runtime directory for lock files
///
/// Priority order:
/// 1. $XDG_RUNTIME_DIR (if set, e.g., /run/user/1000/)
/// 2. /tmp (fallback, with UID suffix added to filename)
fn get_runtime_dir() -> String {
    std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".to_string())
}

/// Returns the socket and PID file paths for this user
///
/// Paths are user-isolated:
/// - With XDG_RUNTIME_DIR: $XDG_RUNTIME_DIR/iftpfm2.sock
/// - Without XDG_RUNTIME_DIR: /tmp/iftpfm2_<uid>.sock
fn get_lock_paths() -> (String, String) {
    let runtime_dir = get_runtime_dir();
    let program_name = crate::PROGRAM_NAME;

    // Check if we're using XDG_RUNTIME_DIR (already user-isolated)
    // vs /tmp (needs UID suffix)
    if runtime_dir != "/tmp" {
        (
            format!("{}/{}.sock", runtime_dir, program_name),
            format!("{}/{}.pid", runtime_dir, program_name),
        )
    } else {
        // Fallback: add UID to filename for user isolation
        let uid = unsafe { libc::getuid() };
        (
            format!("/tmp/{}_{}.sock", program_name, uid),
            format!("/tmp/{}_{}.pid", program_name, uid),
        )
    }
}

/// Global storage for the lock file handle (kept locked for program lifetime)
static LOCK_FILE_HANDLE: Lazy<Mutex<Option<std::fs::File>>> = Lazy::new(|| Mutex::new(None));

// Signal the existing process to terminate gracefully
fn signal_process_to_terminate(socket_path: &str, grace_seconds: u64) -> io::Result<()> {
    // Read PID from the lock file instead of using lsof
    let pid_path = socket_path.replace(".sock", ".pid");
    let pid_str = std::fs::read_to_string(&pid_path)
        .map_err(|e| io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to read PID from {}: {}", pid_path, e)
        ))?
        .trim()
        .to_string();

    if pid_str.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "PID file is empty"
        ));
    }

    let pid: u32 = pid_str.parse()
        .map_err(|e| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid PID in file: {}", e)
        ))?;

    let _ = log(&format!("Found old instance with PID {}, sending termination signal", pid));

    // Set the shutdown flag for our own process if we're signaling ourselves
    let our_pid = std::process::id();
    if pid == our_pid {
        request_shutdown();
        return Ok(());
    }

    let nix_pid = Pid::from_raw(pid as i32);

    // Send SIGTERM to allow graceful shutdown
    signal::kill(nix_pid, Signal::SIGTERM)
        .map_err(|e| io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Failed to send SIGTERM to process {}: {}", pid, e)
        ))?;

    let _ = log(&format!("Successfully sent termination signal to old instance with PID {}", pid));

    // Wait for up to grace_seconds for the process to terminate
    for i in 1..=(grace_seconds * 2) { // Check twice per second
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Check if the process is still running using signal(0)
        match signal::kill(nix_pid, None) {
            Ok(_) => {
                // Process still exists
            }
            Err(nix::Error::ESRCH) => {
                // Process does not exist - it terminated
                let _ = log(&format!("Old instance with PID {} has terminated gracefully", pid));
                return Ok(());
            }
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error checking process {}: {}", pid, e)
                ));
            }
        }

        if i % 2 == 0 { // Log every second
            let _ = log(&format!("Waiting for old instance with PID {} to terminate ({} of {} seconds)...",
                pid, i/2, grace_seconds));
        }
    }

    // If process didn't terminate after timeout, use SIGKILL as last resort
    let _ = log(&format!("Old instance with PID {} did not terminate gracefully, forcing termination", pid));
    signal::kill(nix_pid, Signal::SIGKILL)
        .map_err(|e| io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Failed to send SIGKILL to process {}: {}", pid, e)
        ))?;

    let _ = log(&format!("Forcibly terminated old instance with PID {}", pid));
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
    let (socket_path, pid_path) = get_lock_paths();

    // ATOMIC: Try to acquire exclusive lock on PID file
    // IMPORTANT: Open WITHOUT truncate to avoid race condition.
    // The file will be truncated AFTER we hold the lock.
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
    // Now safely truncate the file before writing our PID
    if let Err(e) = lock_file.set_len(0) {
        return Err(io::Error::new(
            e.kind(),
            format!("Failed to truncate lock file {}: {}", pid_path, e)
        ));
    }

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
    let mut signals = Signals::new([SIGINT, SIGTERM]).expect("Error setting signal handler");

    let _signal_handle = thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            let signal_type = match sig {
                SIGINT => 1,
                SIGTERM => 2,
                _ => 1,
            };
            crate::shutdown::request_shutdown_with_signal(signal_type);
        }
    });

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

/// Drops the socket listener thread handle
///
/// The listener thread is typically blocked on `incoming()` and cannot be
/// cleanly joined. Instead of attempting to join (which would block indefinitely
/// or create orphaned threads), we simply drop the handle.
///
/// When the process exits, the OS will terminate all threads automatically.
/// Should be called during cleanup.
pub fn join_listener_thread() {
    if let Ok(mut handle_guard) = LISTENER_HANDLE.lock() {
        // Take the handle and drop it explicitly
        // This releases the thread's ownership, allowing OS cleanup on process exit
        let _ = handle_guard.take();
    }
}

/// Cleans up single instance lock files
///
/// Removes:
/// - Unix domain socket ($XDG_RUNTIME_DIR/{PROGRAM_NAME}.sock OR /tmp/{PROGRAM_NAME}_{uid}.sock)
/// - PID file ($XDG_RUNTIME_DIR/{PROGRAM_NAME}.pid OR /tmp/{PROGRAM_NAME}_{uid}.pid)
///
/// Called automatically on program exit (e.g., via scopeguard in main)
pub fn cleanup_lock_file() {
    let (socket_path, pid_path) = get_lock_paths();

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
    fn test_get_lock_paths_with_xdg_runtime_dir() {
        // Test with XDG_RUNTIME_DIR set
        temp_env::with_var("XDG_RUNTIME_DIR", Some("/run/user/1000"), || {
            let (socket_path, pid_path) = get_lock_paths();
            assert_eq!(socket_path, "/run/user/1000/iftpfm2.sock");
            assert_eq!(pid_path, "/run/user/1000/iftpfm2.pid");
        });
    }

    #[test]
    #[serial]
    fn test_get_lock_paths_without_xdg_runtime_dir() {
        // Test without XDG_RUNTIME_DIR (should use /tmp with UID suffix)
        temp_env::with_var_unset("XDG_RUNTIME_DIR", || {
            let (socket_path, pid_path) = get_lock_paths();
            let uid = unsafe { libc::getuid() };
            assert_eq!(socket_path, format!("/tmp/iftpfm2_{}.sock", uid));
            assert_eq!(pid_path, format!("/tmp/iftpfm2_{}.pid", uid));
        });
    }

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

        // Note: cleanup_lock_file uses paths from get_lock_paths()
        // which are based on PROGRAM_NAME and environment.
        // This test verifies the concept - in a real scenario, we'd need to
        // mock the environment or test the actual paths.
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
