//! FTP File Mover Utility - Main Binary Crate
//!
//! This crate serves as the entry point for the iftpfm2 executable.
//! It utilizes the `iftpfm2_lib` crate for all core logic.

// Use the library crate. This assumes `iftpfm2_lib` is correctly named in Cargo.toml
// or that Cargo.toml defines `iftpfm2` as the library name.
// If the library name is the same as the package, it's just `use iftpfm2;`
// For clarity, let's assume the library will be refered to by the project name `iftpfm2`.
use iftpfm2::*; // Import all re-exported items from lib.rs
use iftpfm2::CliError; // Import CliError for pattern matching

use std::sync::Arc; // Keep Arc for main's specific logic
use rayon::prelude::*; // Keep rayon for main's specific logic
use std::process; // For process::exit

// Removed most imports as they are now handled within the library modules.
// Kept imports that are directly used in the main function's logic,
// like Arc for config sharing and rayon for parallelism.

// All functions and structs previously defined here are now in their respective modules
// within the library (src/lib.rs and its submodules).

/// Main program entry point
///
/// # Behavior
/// 1. Parses command line arguments using `iftpfm2::cli::parse_args`.
/// 2. Sets up logging using `iftpfm2::logging::set_log_file` and `iftpfm2::logging::log`.
/// 3. Enforces single instance using `iftpfm2::instance::check_single_instance`.
/// 4. Reads configuration using `iftpfm2::config::parse_config`.
/// 5. Processes transfers in parallel using `iftpfm2::ftp_ops::transfer_files`.
/// 6. Handles graceful shutdown using `iftpfm2::shutdown::is_shutdown_requested`.
/// 7. Cleans up lock files using `iftpfm2::instance::cleanup_lock_file`.
///
/// # Exit Codes
/// - 0: Success
/// - 1: Error during initialization
fn main() {
    // Parse arguments first to setup logging
    // These functions are now part of the library, accessed via the use statement.
    let cli::CliArgs { delete, log_file: log_file_option,
                       config_file: config_file_option,
                       parallel, randomize, grace_seconds, connect_timeout, insecure_skip_verify,
                       temp_dir, debug, ram_threshold } =
        match parse_args() { // from iftpfm2::cli
            Ok(args) => args,
            Err(CliError::HelpRequested) => process::exit(0),
            Err(CliError::VersionRequested) => process::exit(0),
            Err(_e) => {
                // Error messages already printed by parse_args()
                process::exit(1);
            }
        };

    // Enable debug mode if requested
    if debug {
        set_debug_mode(true); // from iftpfm2::logging
    }

    // Set up logging destination
    // - If -l flag is used, log to the specified file
    // - Otherwise, log to stdout (default behavior)
    if let Some(lf) = log_file_option {
        set_log_file(lf); // from iftpfm2::logging
    }

    // Check for single instance after logging is configured
    if let Err(e) = check_single_instance(grace_seconds) { // from iftpfm2::instance
        // Ensure log function is available. It should be from iftpfm2::logging.
        let _ = log(&format!("Error checking single instance: {}", e));
        process::exit(1);
    }
    
    // Ensure lock file is removed on normal exit or panic
    // `cleanup_lock_file` is from `iftpfm2::instance`
    let _cleanup = scopeguard::guard((), |_| {
        join_listener_thread(); // Join the listener thread before cleanup
        cleanup_lock_file();
    });

    let _ = log(format!("{} version {} started", PROGRAM_NAME, PROGRAM_VERSION).as_str()); // PROGRAM_NAME & VERSION from lib.rs

    // Watch for shutdown signals and log them in the main thread (not in signal handler)
    // This is async-signal-safe: we only poll atomic flags here
    let signal_watch_thread = std::thread::spawn(|| {
        use crate::shutdown::{is_shutdown_requested, get_signal_type};
        loop {
            if is_shutdown_requested() {
                if let Some(signal_type) = get_signal_type() {
                    let signal_name = if signal_type == 1 { "SIGINT (Ctrl+C)" } else { "SIGTERM" };
                    let _ = log(&format!("Received termination signal ({}), PID {} shutting down gracefully",
                        signal_name, std::process::id()));
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    // Parse config file
    let config_file_path = config_file_option.expect("Config file path should be present due to parse_args validation");
    let configs_vec = match parse_config(&config_file_path) { // from iftpfm2::config
        Ok(cfgs) => cfgs,
        Err(e) => {
            let _ = log(&format!("Error parsing config file '{}': {}", config_file_path, e));
            process::exit(1);
        }
    };

    // Create thread pool with specified parallelism
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(parallel.max(1)) // Ensure at least 1 thread
        .build()
        .unwrap_or_else(|e| {
            let _ = log(&format!("Error creating thread pool: {}", e));
            process::exit(1);
        });

    // Process configs in parallel (randomize order if requested)
    let mut configs_to_process = configs_vec;
    if randomize {
        use rand::seq::SliceRandom;
        use rand::thread_rng;
        configs_to_process.shuffle(&mut thread_rng());
    }
    let configs_arc = Arc::new(configs_to_process);
    let delete_arc = Arc::new(delete);
    let temp_dir_arc = Arc::new(temp_dir.as_deref());
    let ram_threshold_arc = Arc::new(ram_threshold);

    let total_transfers: i32 = pool.install(|| {
        configs_arc
            .par_iter()
            .enumerate()
            .map(|(idx, cf_item)| { // cf_item is a reference to Config
                // Check for shutdown before starting each config
                if is_shutdown_requested() { // from iftpfm2::shutdown
                    return 0;
                }
                let thread_id = rayon::current_thread_index().unwrap_or(idx);
                // transfer_files is from iftpfm2::ftp_ops
                transfer_files(cf_item, *delete_arc, thread_id, connect_timeout, insecure_skip_verify, *temp_dir_arc, *ram_threshold_arc)
            })
            .sum()
    });

    let exit_message = if is_shutdown_requested() { // from iftpfm2::shutdown
        format!(
            "{} version {} terminated due to shutdown request, transferred {} file(s)",
            PROGRAM_NAME, PROGRAM_VERSION, total_transfers // Constants from lib.rs
        )
    } else {
        format!(
            "{} version {} finished, successfully transferred {} file(s)",
            PROGRAM_NAME, PROGRAM_VERSION, total_transfers // Constants from lib.rs
        )
    };

    // Signal the watcher thread to exit (if we completed normally)
    request_shutdown();

    // Wait for the signal watcher thread to finish
    let _ = signal_watch_thread.join();

    let _ = log(&exit_message);
}
