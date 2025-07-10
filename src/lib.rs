//! FTP File Mover Utility Library
//!
//! This library contains the core logic for the iftpfm2 utility,
//! including configuration parsing, FTP operations, logging,
//! command-line interface handling, shutdown signaling, and
//! single-instance management.

// Module declarations
pub mod cli;
pub mod config;
pub mod ftp_ops;
pub mod instance;
pub mod logging;
pub mod shutdown;

// Re-export key items for easy use by the binary (main.rs)
pub use cli::parse_args;
pub use config::{parse_config, Config};
pub use ftp_ops::transfer_files;
pub use instance::{check_single_instance, cleanup_lock_file};
pub use logging::{log, log_with_thread, set_log_file};
pub use shutdown::{is_shutdown_requested, request_shutdown}; // Added request_shutdown

/// Name of the program used for:
/// - Process identification
/// - Lock files (/tmp/{PROGRAM_NAME}.pid)
/// - Unix domain socket (/tmp/{PROGRAM_NAME}.sock)
pub const PROGRAM_NAME: &str = "iftpfm2";

/// Current version of the program (from Cargo.toml)
/// Follows semantic versioning (MAJOR.MINOR.PATCH)
pub const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");

// Dependencies that were in main.rs and are used by multiple modules,
// or are fundamental to the library's operation, can be listed here
// or within the specific modules that use them.
// For now, each module handles its own specific imports.
// Common ones like `std::io`, `std::process` are used directly in modules.
// External crates like `ftp`, `regex`, `chrono`, `once_cell`, `rayon`, `ctrlc`, `scopeguard`
// will need to be listed in Cargo.toml and then `use`d in the modules that need them.

// Example of how PROGRAM_NAME and PROGRAM_VERSION might be used from within a module if not passed:
// use crate::{PROGRAM_NAME, PROGRAM_VERSION};
// This is now handled by making them pub const in lib.rs and modules using `crate::PROGRAM_NAME`.

// The `main` function in `src/main.rs` will now primarily use items from this library crate.
// e.g. `use iftpfm2::config::parse_config;` or `use iftpfm2::*;`
//
// We'll need to ensure that all necessary `pub` keywords are used within each module
// for items that need to be accessed by other modules or by `main.rs` via this lib.rs.
// For example, `Config` struct in `config.rs` must be `pub struct Config`.
// Functions like `parse_config` must be `pub fn parse_config`.
// Statics like `LOG_FILE` in `logging.rs` must be `pub static LOG_FILE`.

// Note: The temporary const declarations for PROGRAM_NAME in cli.rs and instance.rs
// were removed, and those modules now use `crate::PROGRAM_NAME` as intended.
