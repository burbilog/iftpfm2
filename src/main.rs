//! FTP File Mover Utility
//!
//! A multi-threaded tool for transferring files between FTP servers with:
//! - Configurable parallel transfers
//! - Age-based file filtering  
//! - Graceful shutdown handling
//! - Comprehensive logging
//!
//! # Key Features
//! - Single instance enforcement
//! - Configurable file matching patterns
//! - Random processing order option
//! - 30-second graceful shutdown timeout
//!
//! See README.md for usage examples and configuration file format.

use chrono::DateTime;
use chrono::Local;
use ftp::FtpStream;
use regex::Regex;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::path::Path;
use std::process::{self};
use std::str::FromStr;
use std::time::SystemTime;
use once_cell::sync::Lazy;
use std::sync::{Mutex, Arc};
use rayon::prelude::*;

fn print_usage() {
    println!(
        "Usage: {} [-h] [-v] [-d] [-r] [-x \".*\\.xml\"] [-l logfile] [-p parallel] config_file",
        PROGRAM_NAME
    );
}

/// Parses command line arguments and returns configuration options
///
/// # Returns
/// A tuple containing:
/// - `bool`: Whether to delete source files after transfer
/// - `Option<String>`: Path to log file (None for stdout)
/// - `Option<String>`: Path to config file
/// - `Option<String>`: File matching regex pattern
/// - `usize`: Number of parallel threads
/// - `bool`: Whether to randomize processing order
///
/// # Panics
/// - If required arguments are missing
/// - If numeric arguments can't be parsed
///
/// # Example
/// ```
/// let (delete, log_file, config_file, ext, parallel, randomize) = parse_args();
/// ```
pub fn parse_args() -> (bool, Option<String>, Option<String>, Option<String>, usize, bool) {
    let mut log_file = None;
    let mut delete = false;
    let mut config_file = None;
    let mut ext = None;
    let mut parallel = 1;
    let mut randomize = false;

    let mut args = env::args();
    args.next(); // Skip program name

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                print_usage();
                process::exit(0);
            }
            "-v" => {
                println!("{} version {}", PROGRAM_NAME, PROGRAM_VERSION);
                process::exit(0);
            }
            "-d" => delete = true,
            "-l" => log_file = Some(args.next().expect("Missing log file argument")),
            "-x" => ext = Some(args.next().expect("Missing matching regexp argument")),
            "-p" => parallel = args.next().expect("Missing parallel count argument").parse().expect("Parallel count must be a number"),
            "-r" => randomize = true,
            _ => {
                config_file = Some(arg);
            }
        }
    }

    if config_file.is_none() {
        eprintln!("Missing config file argument");
        print_usage();
        process::exit(1);
    }

    if ext.is_none() {
        ext = Some(".*\\.xml".to_string());
    }

    (delete, log_file, config_file, ext, parallel, randomize)
}

/// FTP transfer configuration parameters
#[derive(Debug, PartialEq)]
pub struct Config {
    /// Source FTP server IP/hostname
    pub ip_address_from: String,
    /// Source FTP server port (typically 21)
    pub port_from: u16,
    /// Username for source FTP server
    pub login_from: String,
    /// Password for source FTP server
    pub password_from: String,
    /// Source directory path (must be literal path, no wildcards)
    pub path_from: String,
    /// Destination FTP server IP/hostname  
    pub ip_address_to: String,
    /// Destination FTP server port (typically 21)
    pub port_to: u16,
    /// Username for destination FTP server
    pub login_to: String,
    /// Password for destination FTP server
    pub password_to: String,
    /// Destination directory path
    pub path_to: String,
    /// Minimum file age to transfer (seconds)
    pub age: u64,
}

/// Parses configuration file into a vector of Config structs
///
/// # Arguments
/// * `filename` - Path to configuration file
///
/// # Returns
/// * `Result<Vec<Config>, Error>` - Vector of parsed configs or error
///
/// # Errors
/// - File not found or unreadable
/// - Invalid field format (non-numeric where expected)
/// - Missing required fields
///
/// # File Format
/// CSV format with fields:
/// ip_from,port_from,login_from,password_from,path_from,
/// ip_to,port_to,login_to,password_to,path_to,min_age_secs
///
/// # Example
/// ```
/// let configs = parse_config("settings.csv")?;
/// ```
pub fn parse_config(filename: &str) -> Result<Vec<Config>, Error> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let mut configs = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let mut fields = line.split(',');
        let ip_address_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: ip_address_from",
            ))?
            .to_string();
        let port_from = u16::from_str(fields.next().ok_or(Error::new(
            ErrorKind::InvalidInput,
            "missing field: port_from",
        ))?)
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let login_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: login_from",
            ))?
            .to_string();
        let password_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: password_from",
            ))?
            .to_string();
        let path_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: path_from",
            ))?
            .to_string();
        let ip_address_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: ip_address_to",
            ))?
            .to_string();
        let port_to = u16::from_str(fields.next().ok_or(Error::new(
            ErrorKind::InvalidInput,
            "missing field: port_to",
        ))?)
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let login_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: login_to",
            ))?
            .to_string();
        let password_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: password_to",
            ))?
            .to_string();
        let path_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: path_to",
            ))?
            .to_string();
        let age = u64::from_str(
            fields
                .next()
                .ok_or(Error::new(ErrorKind::InvalidInput, "missing field: age"))?,
        )
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;

        configs.push(Config {
            ip_address_from,
            port_from,
            login_from,
            password_from,
            path_from,
            ip_address_to,
            port_to,
            login_to,
            password_to,
            path_to,
            age,
        });
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::Config;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_parse_config() {
        let config_string = "192.168.0.1,22,user1,password1,/path/to/files/*,192.168.0.2,22,user2,password2,/path/to/files2,30\n192.168.0.3,22,user3,password3,/path/to/files3/*,192.168.0.4,22,user4,password4,/path/to/files4,60";
        let expected = vec![
            Config {
                ip_address_from: "192.168.0.1".to_string(),
                port_from: 22,
                login_from: "user1".to_string(),
                password_from: "password1".to_string(),
                path_from: "/path/to/files/*".to_string(),
                ip_address_to: "192.168.0.2".to_string(),
                port_to: 22,
                login_to: "user2".to_string(),
                password_to: "password2".to_string(),
                path_to: "/path/to/files2".to_string(),
                age: 30,
            },
            Config {
                ip_address_from: "192.168.0.3".to_string(),
                port_from: 22,
                login_from: "user3".to_string(),
                password_from: "password3".to_string(),
                path_from: "/path/to/files3/*".to_string(),
                ip_address_to: "192.168.0.4".to_string(),
                port_to: 22,
                login_to: "user4".to_string(),
                password_to: "password4".to_string(),
                path_to: "/path/to/files4".to_string(),
                age: 60,
            },
        ];

        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.csv");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let configs = super::parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs, expected);
    }
}
// LOG_FILE is a thread-safe, lazily initialized global variable
// It holds an Option<String> representing the path to the log file (if set)
// The Mutex ensures thread-safe access to this value
/// Global log file path protected by Mutex
///
/// Thread-safe storage for optional log file path.
/// When None, logs go to stdout.
static LOG_FILE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

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
/// ```
/// log("Starting transfer").unwrap();
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
/// ```
/// log_with_thread("Thread started", Some(1)).unwrap();
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
            // If no log file is set, print the message to stdout
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
    let path = path.as_ref().to_str().unwrap();
    *LOG_FILE.lock().unwrap() = Some(path.to_string());
}

#[cfg(test)]
use std::fs::remove_file;
#[cfg(test)]
use tempfile::tempdir;

#[test]
fn test_log_to_file() {
    let dir = tempdir().unwrap();
    println!("tempdir {}", std::env::temp_dir().display());
    let log_file = dir.path().join("log.txt");

    set_log_file(log_file.as_path());
    log("test message 1").unwrap();
    log("test message 2").unwrap();

    let log_contents = std::fs::read_to_string(log_file.clone()).unwrap();
    assert!(log_contents.contains("test message 1"));
    assert!(log_contents.contains("test message 2"));
    remove_file(log_file).unwrap();
}

/// Transfers files between FTP servers according to configuration
///
/// # Arguments
/// * `config` - FTP connection and transfer parameters  
/// * `delete` - Whether to delete source files after transfer
/// * `ext` - Optional regex pattern for file matching
/// * `thread_id` - Identifier for logging in parallel mode
///
/// # Returns
/// Number of files successfully transferred
///
/// # Errors
/// Logs errors but doesn't fail - returns count of successful transfers
///
/// # Behavior
/// - Skips files younger than config.age seconds
/// - Respects shutdown requests
/// - Logs detailed transfer progress
///
/// # Example
/// ```
/// let count = transfer_files(&config, true, Some(".*\.xml".into()), 1);
/// ```
pub fn transfer_files(config: &Config, delete: bool, ext: Option<String>, thread_id: usize) -> i32 {
    // Check for shutdown request before starting
    if is_shutdown_requested() {
        log_with_thread("Shutdown requested, skipping transfer", Some(thread_id)).unwrap();
        return 0;
    }
    
    log_with_thread(format!(
        "Transferring files from ftp://{}:{}{} to ftp://{}:{}{}",
        config.ip_address_from,
        config.port_from,
        config.path_from,
        config.ip_address_to,
        config.port_to,
        config.path_to
    )
    .as_str(), Some(thread_id))
    .unwrap();
    // Connect to the source FTP server
    let mut ftp_from = match FtpStream::connect((config.ip_address_from.as_str(), config.port_from))
    {
        Ok(ftp) => ftp,
        Err(e) => {
            log_with_thread(format!(
                "Error connecting to SOURCE FTP server {}: {}",
                config.ip_address_from, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return 0;
        }
    };
    ftp_from
        .login(config.login_from.as_str(), config.password_from.as_str())
        .unwrap_or_else(|e| {
            log_with_thread(format!(
                "Error logging into SOURCE FTP server {}: {}",
                config.ip_address_from, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return;
        });
    match ftp_from.cwd(config.path_from.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log_with_thread(format!(
                "Error changing directory on SOURCE FTP server {}: {}",
                config.ip_address_from, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return 0;
        }
    }

    // Connect to the target FTP server
    let mut ftp_to = match FtpStream::connect((config.ip_address_to.as_str(), config.port_to)) {
        Ok(ftp) => ftp,
        Err(e) => {
            log_with_thread(format!(
                "Error connecting to TARGET FTP server {}: {}",
                config.ip_address_to, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return 0;
        }
    };
    ftp_to
        .login(config.login_to.as_str(), config.password_to.as_str())
        .unwrap_or_else(|e| {
            log_with_thread(format!(
                "Error logging into TARGET FTP server {}: {}",
                config.ip_address_to, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return;
        });
    match ftp_to.cwd(config.path_to.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log_with_thread(format!(
                "Error changing directory on TARGET FTP server {}: {}",
                config.ip_address_to, e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            return 0;
        }
    }

    // Get the list of files in the source directory
    // Do not use NLST with paramter because pyftpdlib does not understand that
    let file_list = match ftp_from.nlst(None) {
        Ok(list) => list,
        Err(e) => {
            log_with_thread(format!("Error getting file list from SOURCE FTP server: {}", e).as_str(), Some(thread_id)).unwrap();
            return 0;
        }
    };
    let number_of_files = file_list.len();
    log_with_thread(format!(
        "Number of files retrieved from SOURCE FTP server: {}",
        file_list.len()
    )
    .as_str(), Some(thread_id))
    .unwrap();
    let ext_regex = match ext.as_ref().map(String::as_str) {
        Some(ext) => Regex::new(ext),
        None => {
            // Handle the case where `ext` is None
            log_with_thread(&format!("FUCK"), Some(thread_id)).unwrap();
            return 0;
        }
    };
    let regex = ext_regex.unwrap();
    // Transfer each file from the source to the target directory
    let mut successful_transfers = 0;
    for filename in file_list {
        // Check for shutdown request before each file
        if is_shutdown_requested() {
            log_with_thread("Shutdown requested, aborting remaining transfers", Some(thread_id)).unwrap();
            break;
        }
        
        if !regex.is_match(&filename) {
            log_with_thread(format!(
                "Skipping file {} as it did not match regex {}",
                filename, regex
            )
            .as_str(), Some(thread_id))
            .unwrap();
            continue;
        }
        //log(format!("Working on file {}", filename).as_str()).unwrap();
        // Get the modified time of the file on the FTP server
        let modified_time_str = match ftp_from.mdtm(filename.as_str()) {
            Ok(time) => {
                // too noisy
                //log(&format!("Successfully retrieved modified time '{}' for file '{}'", time.unwrap(), filename)).unwrap();
                time.unwrap()
            }
            Err(e) => {
                //log(&format!("Error getting modified time for file(?) '{}': '{}', skipping", filename, e)).unwrap();
                log_with_thread(&format!(
                    "Error getting modified time, skipping file(?) '{}': {}",
                    filename,
                    e.to_string().replace("\n", "")
                ), Some(thread_id))
                .unwrap();
                continue;
            }
        };
        let modified_time_replaced_utc = modified_time_str.to_string().replace("UTC", "+0000");
        let modified_time = match DateTime::parse_from_str(
            modified_time_replaced_utc.as_str(),
            "%Y-%m-%d %H:%M:%S %z",
        ) {
            Ok(time) => time.into(),
            Err(err) => {
                log_with_thread(&format!(
                    "Error parsing modified time '{}': {}",
                    modified_time_str, err
                ), Some(thread_id))
                .unwrap();
                continue;
            }
        };

        //log(format!("modified_time: {:?}", modified_time).as_str()).unwrap();
        //log(format!("system time: {:?}", SystemTime::now()).as_str()).unwrap();

        // Calculate the age of the file
        let file_age = match SystemTime::now().duration_since(modified_time) {
            Ok(duration) => duration.as_secs(),
            Err(_) => {
                log_with_thread(&format!(
                    "Error calculating age for file '{}', skipping",
                    filename
                ), Some(thread_id))
                .unwrap();
                continue;
            }
        };

        // Skip the file if it is younger than the specified age
        if file_age < (config.age as u64) {
            log_with_thread(format!(
                "Skipping file {}, it is {} seconds old, less than specified age {} seconds",
                filename, file_age, config.age
            )
            .as_str(), Some(thread_id))
            .unwrap();
            continue;
        }
        //log(format!("Transferring file {}", filename).as_str()).unwrap();
        match ftp_to.rm(filename.as_str()) {
            Ok(_) => {
                log_with_thread(format!("Deleted file {} at TARGET FTP server", filename).as_str(), Some(thread_id)).unwrap()
            }
            Err(_) => (),
        };

        // Set binary mode for both FTP connections
        if let Err(e) = ftp_from.transfer_type(ftp::types::FileType::Binary) {
            log_with_thread(format!(
                "Error setting binary mode on SOURCE FTP server: {}",
                e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            continue;
        }

        if let Err(e) = ftp_to.transfer_type(ftp::types::FileType::Binary) {
            log_with_thread(format!(
                "Error setting binary mode on TARGET FTP server: {}",
                e
            )
            .as_str(), Some(thread_id))
            .unwrap();
            continue;
        }

        match ftp_from.simple_retr(filename.as_str()) {
            Ok(mut data) => match ftp_to.put(filename.as_str(), &mut data) {
                Ok(_) => {
                    log_with_thread(format!("Successful transfer of file {}", filename).as_str(), Some(thread_id)).unwrap();
                    successful_transfers += 1;
                }
                Err(e) => {
                    log_with_thread(format!(
                        "Error transferring file {} to TARGET FTP server: {}",
                        filename, e
                    )
                    .as_str(), Some(thread_id))
                    .unwrap();
                    continue;
                }
            },
            Err(e) => {
                log_with_thread(format!(
                    "Error transferring file {} from SOURCE FTP server: {}",
                    filename, e
                )
                .as_str(), Some(thread_id))
                .unwrap();
                continue;
            }
        }

        // Delete the source file if specified
        if delete {
            match ftp_from.rm(filename.as_str()) {
                Ok(_) => {
                    log_with_thread(format!("Deleted SOURCE file {}", filename).as_str(), Some(thread_id)).unwrap();
                }
                Err(e) => {
                    log_with_thread(format!("Error deleting SOURCE file {}: {}", filename, e).as_str(), Some(thread_id))
                        .unwrap();
                }
            }
        }
    }
    log_with_thread(format!(
        "Successfully transferred {} files out of {}",
        successful_transfers, number_of_files
    )
    .as_str(), Some(thread_id))
    .unwrap();
    successful_transfers
}

/// Name of the program used for:
/// - Process identification
/// - Lock files (/tmp/{PROGRAM_NAME}.pid)
/// - Unix domain socket (/tmp/{PROGRAM_NAME}.sock)
const PROGRAM_NAME: &str = "iftpfm2";

/// Current version of the program (from Cargo.toml)
/// Follows semantic versioning (MAJOR.MINOR.PATCH)
const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::Command;
use std::io::Read;
use ctrlc;

// Global flag to indicate if shutdown was requested
/// Global shutdown flag (atomic bool)
///
/// Set to true when shutdown is requested via signal.
/// Threads should check this flag regularly and exit cleanly.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

// Check if shutdown was requested
/// Checks if graceful shutdown has been requested
///
/// Threads should call this regularly and exit cleanly if true
///
/// # Returns
/// `true` if shutdown was requested via signal
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

// Signal that shutdown is requested
/// Signals all threads to shutdown gracefully
///
/// Sets global flag that threads should check via is_shutdown_requested()
fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

// Signal the existing process to terminate gracefully
fn signal_process_to_terminate(socket_path: &str) -> io::Result<()> {
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
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to send termination signal to process {}", pid_str)
        ));
    }
    
    log(&format!("Successfully sent termination signal to old instance with PID {}", pid_str)).unwrap();
    
    // Wait for up to 30 seconds for the process to terminate
    for i in 1..=60 {
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
        
        if i % 2 == 0 {
            log(&format!("Waiting for old instance with PID {} to terminate ({} seconds)...", 
                pid_str, i/2)).unwrap();
        }
    }
    
    // If process didn't terminate after timeout, use SIGKILL as last resort
    log(&format!("Old instance with PID {} did not terminate gracefully, forcing termination", pid_str)).unwrap();
    let kill_output = Command::new("kill")
        .arg("-9")  // SIGKILL for forced termination
        .arg(&pid_str)
        .output()?;
    
    if !kill_output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to force termination of process {}", pid_str)
        ));
    }
    
    log(&format!("Forcibly terminated old instance with PID {}", pid_str)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    
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
fn check_single_instance() -> io::Result<()> {
    let socket_path = format!("/tmp/{}.sock", PROGRAM_NAME);
    
    // Try to connect to existing socket
    if UnixStream::connect(&socket_path).is_ok() {
        log(&format!("Another instance is running, new instance PID {} requesting graceful termination", 
            std::process::id())).unwrap();
        
        // Try to signal the process to terminate gracefully
        if let Err(e) = signal_process_to_terminate(&socket_path) {
            log(&format!("Failed to signal old process: {}", e)).unwrap();
        }
        
        // Clean up the socket file regardless
        let _ = std::fs::remove_file(&socket_path);
    } else {
        // Clean up any stale socket file
        let _ = std::fs::remove_file(&socket_path);
    }
    
    // Create a new socket file with our PID
    let listener = UnixListener::bind(&socket_path)?;
    
    // Write our PID to the socket for future reference
    let pid = std::process::id().to_string();
    let mut pid_file = File::create(format!("/tmp/{}.pid", PROGRAM_NAME))?;
    pid_file.write_all(pid.as_bytes())?;
    
    // Set up signal handler for SIGTERM
    let our_pid = std::process::id();
    ctrlc::set_handler(move || {
        log(&format!("Received termination signal, PID {} shutting down gracefully", our_pid)).unwrap();
        request_shutdown();
    }).expect("Error setting signal handler");
    
    // Spawn a thread to keep the socket alive and listen for signals
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = [0; 8];
                if let Ok(size) = stream.read(&mut buffer) {
                    if size >= 8 && &buffer[0..8] == b"SHUTDOWN" {
                        log(&format!("Received shutdown signal from new instance")).unwrap();
                        request_shutdown();
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// Cleans up single instance lock files
///
/// Removes:
/// - Unix domain socket (/tmp/{PROGRAM_NAME}.sock)
/// - PID file (/tmp/{PROGRAM_NAME}.pid)
///
/// Called automatically on program exit
fn cleanup_lock_file() {
    let socket_path = format!("/tmp/{}.sock", PROGRAM_NAME);
    let pid_path = format!("/tmp/{}.pid", PROGRAM_NAME);
    let _ = std::fs::remove_file(socket_path);
    let _ = std::fs::remove_file(pid_path);
}

/// Main program entry point
///
/// # Behavior
/// 1. Parses command line arguments
/// 2. Sets up logging
/// 3. Enforces single instance
/// 4. Reads configuration
/// 5. Processes transfers in parallel
/// 6. Handles graceful shutdown
///
/// # Exit Codes
/// - 0: Success
/// - 1: Error during initialization
fn main() {
    // Parse arguments first to setup logging
    let (delete, log_file, config_file, ext, parallel, randomize) = parse_args();
    if let Some(log_file) = log_file {
        set_log_file(log_file);
    }

    // Check for single instance after logging is configured
    if let Err(e) = check_single_instance() {
        log(&format!("Error checking single instance: {}", e)).unwrap();
        process::exit(1);
    }
    
    // Ensure lock file is removed on normal exit
    let _cleanup = scopeguard::guard((), |_| cleanup_lock_file());

    log(format!("{} version {} started", PROGRAM_NAME, PROGRAM_VERSION).as_str()).unwrap();

    // Parse config file
    let config_file = config_file.unwrap();
    let configs = parse_config(&config_file).unwrap();

    // Create thread pool with specified parallelism
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(parallel.max(1)) // Ensure at least 1 thread
        .build()
        .unwrap();

    // Process configs in parallel (randomize order if requested)
    let mut configs = configs;
    if randomize {
        use rand::seq::SliceRandom;
        use rand::thread_rng;
        configs.shuffle(&mut thread_rng());
    }
    let configs_arc = Arc::new(configs);
    let delete_arc = Arc::new(delete);
    let ext_arc = Arc::new(ext);

    let total_transfers: i32 = pool.install(|| {
        configs_arc
            .par_iter()
            .enumerate()
            .map(|(idx, cf)| {
                // Check for shutdown before starting each config
                if is_shutdown_requested() {
                    return 0;
                }
                let thread_id = rayon::current_thread_index().unwrap_or(idx);
                transfer_files(cf, *delete_arc, ext_arc.as_ref().clone(), thread_id)
            })
            .sum()
    });

    let exit_message = if is_shutdown_requested() {
        format!(
            "{} version {} terminated due to shutdown request, transferred {} file(s)",
            PROGRAM_NAME, PROGRAM_VERSION, total_transfers
        )
    } else {
        format!(
            "{} version {} finished, successfully transferred {} file(s)",
            PROGRAM_NAME, PROGRAM_VERSION, total_transfers
        )
    };
    
    log(&exit_message).unwrap();
}
