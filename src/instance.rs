use crate::logging::log;
use crate::shutdown::request_shutdown;

use std::fs::File;
use std::io::{self, Write, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use ctrlc;

// This will be moved to lib.rs later and accessed via crate::
// For now, define it here to avoid compilation errors during refactoring steps.
// const PROGRAM_NAME: &str = "iftpfm2"; // Will use crate::PROGRAM_NAME from lib.rs

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

    log(&format!("Found old instance with PID {}, sending termination signal", pid_str)).unwrap();

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

    log(&format!("Successfully sent termination signal to old instance with PID {}", pid_str)).unwrap();

    // Wait for up to grace_seconds for the process to terminate
    for i in 1..=(grace_seconds * 2) { // Check twice per second
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Check if the process is still running
        let check_output = Command::new("kill")
            .arg("-0")  // Check if process exists
            .arg(&pid_str)
            .output()?;

        if !check_output.status.success() {
            log(&format!("Old instance with PID {} has terminated gracefully", pid_str)).unwrap();
            return Ok(());
        }

        if i % 2 == 0 { // Log every second
            log(&format!("Waiting for old instance with PID {} to terminate ({} of {} seconds)...",
                pid_str, i/2, grace_seconds)).unwrap();
        }
    }

    // If process didn't terminate after timeout, use SIGKILL as last resort
    log(&format!("Old instance with PID {} did not terminate gracefully, forcing termination", pid_str)).unwrap();
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

    log(&format!("Forcibly terminated old instance with PID {}", pid_str)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500)); // Give OS a moment

    Ok(())
}

/// Ensures only one instance runs at a time
///
/// # Behavior
/// - Creates lock file with PID
/// - Listens on Unix socket for shutdown requests
/// - Handles cleanup on exit
///
/// # Errors
/// - If socket creation fails
/// - If PID file can't be written
///
/// # Panics
/// If signal handler registration fails
pub fn check_single_instance(grace_seconds: u64) -> io::Result<()> {
    let socket_path = format!("/tmp/{}.sock", crate::PROGRAM_NAME); // Using PROGRAM_NAME from lib.rs

    // Try to connect to existing socket
    if UnixStream::connect(&socket_path).is_ok() {
        log(&format!("Another instance is running, new instance PID {} requesting graceful termination of old one.",
            std::process::id())).unwrap();

        // Try to signal the process to terminate gracefully
        if let Err(e) = signal_process_to_terminate(&socket_path, grace_seconds) {
            log(&format!("Failed to signal old process: {}. Stale socket/pid files might exist.", e)).unwrap();
            // Even if signaling fails, we might be able to remove the socket if it's stale.
        }

        // Attempt to clean up the socket file after signaling (or if signaling failed but socket was stale)
        // This is important so the new instance can bind to it.
        let _ = std::fs::remove_file(&socket_path);
        log(&format!("Removed old socket file: {}", socket_path)).unwrap();

    } else {
        // If connection failed, it might be because the socket file is stale (no one listening).
        // Clean it up before trying to bind.
        let _ = std::fs::remove_file(&socket_path);
    }

    // Create a new socket file for this instance
    let listener = UnixListener::bind(&socket_path)?;
    log(&format!("Created new socket file: {}", socket_path)).unwrap();

    // Write our PID to a common PID file location
    let pid_path = format!("/tmp/{}.pid", crate::PROGRAM_NAME); // Using PROGRAM_NAME from lib.rs
    let mut pid_file = File::create(&pid_path)?;
    pid_file.write_all(std::process::id().to_string().as_bytes())?;
    log(&format!("Written current PID {} to {}", std::process::id(), pid_path)).unwrap();

    // Set up signal handler for SIGINT (Ctrl+C) and SIGTERM
    let current_pid = std::process::id();
    ctrlc::set_handler(move || {
        log(&format!("Received termination signal (Ctrl+C or SIGTERM), PID {} shutting down gracefully", current_pid)).unwrap();
        request_shutdown();
        // The cleanup_lock_file will be called via scopeguard in main.
        // Consider if additional cleanup is needed here or if it's robust enough.
    }).expect("Error setting signal handler");

    // Spawn a thread to listen on the socket for shutdown commands from new instances.
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buffer = [0; 8]; // Expect "SHUTDOWN"
                    if let Ok(size) = stream.read(&mut buffer) {
                        if size == 8 && &buffer[..] == b"SHUTDOWN" {
                            log(&format!("Received 'SHUTDOWN' command on socket. PID {} initiating self-shutdown.",
                                std::process::id())).unwrap();
                            request_shutdown();
                            break; // Exit listener thread
                        }
                    }
                }
                Err(e) => {
                    log(&format!("Error accepting incoming connection on socket: {}", e)).unwrap();
                    // Depending on the error, might want to break or continue.
                    // For now, continue to try accepting more connections.
                }
            }
        }
        log("Socket listener thread exiting.").unwrap();
    });

    Ok(())
}

/// Cleans up single instance lock files
///
/// Removes:
/// - Unix domain socket (/tmp/{PROGRAM_NAME}.sock)
/// - PID file (/tmp/{PROGRAM_NAME}.pid)
///
/// Called automatically on program exit (e.g., via scopeguard in main)
pub fn cleanup_lock_file() {
    let socket_path = format!("/tmp/{}.sock", crate::PROGRAM_NAME); // Using PROGRAM_NAME from lib.rs
    let pid_path = format!("/tmp/{}.pid", crate::PROGRAM_NAME); // Using PROGRAM_NAME from lib.rs

    log(&format!("Cleaning up lock files: {} and {}", socket_path, pid_path)).unwrap();

    if let Err(e) = std::fs::remove_file(&socket_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log(&format!("Failed to remove socket file {}: {}", socket_path, e)).unwrap();
        }
    }
    if let Err(e) = std::fs::remove_file(&pid_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log(&format!("Failed to remove pid file {}: {}", pid_path, e)).unwrap();
        }
    }
}
