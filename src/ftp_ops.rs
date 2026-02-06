use crate::config::{Config, Protocol};
use crate::logging::{log_debug, log_with_thread};
use secrecy::ExposeSecret;
use crate::protocols::Client;
use crate::shutdown::is_shutdown_requested;
use regex::Regex;
use std::io::{Cursor, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

/// Default RAM threshold for temp files (10MB)
/// Files below this size use RAM buffer, larger files use disk
const DEFAULT_RAM_THRESHOLD: u64 = 10 * 1024 * 1024;

/// Connect to FTP/FTPS/SFTP server, login, and change directory
///
/// Returns Ok(client) on success, Err(error_message) on failure
/// The error message is already formatted for logging
fn connect_and_login(
    proto: &Protocol,
    host: &str,
    port: u16,
    login: &str,
    password: Option<&str>,
    keyfile: Option<&str>,
    path: &str,
    timeout: Duration,
    insecure_skip_verify: bool,
    server_type: &str, // "SOURCE" or "TARGET" for logging
    thread_id: usize,
) -> Result<Client, String> {
    // For FTP/FTPS, password is required (validated during config parsing)
    // For SFTP with keyfile, password can be None
    let _ = log_with_thread(format!("[{}] Connecting to {}:{}...", proto, host, port), Some(thread_id));

    let password_for_login = match proto {
        Protocol::Sftp if keyfile.is_some() => password.unwrap_or(""),
        _ => password.ok_or_else(|| {
            format!(
                "BUG: Password required for {} but was None (should have been caught during config validation)",
                proto
            )
        })?,
    };

    let mut client = match Client::connect(proto, host, port, timeout, insecure_skip_verify, login, password, keyfile) {
        Ok(c) => {
            let _ = log_with_thread(format!("[{}] Connected successfully", proto), Some(thread_id));
            c
        }
        Err(e) => {
            return Err(format!(
                "Error connecting to {} FTP server {}:{} ({}s timeout): {}",
                server_type, host, port, timeout.as_secs(), e
            ));
        }
    };

    if let Err(e) = client.login(login, password_for_login) {
        let _ = client.quit();
        return Err(format!(
            "Error logging into {} FTP server {}: {}",
            server_type, host, e
        ));
    }

    if let Err(e) = client.cwd(path) {
        let _ = client.quit();
        return Err(format!(
            "Error changing directory on {} FTP server {} (user '{}', path '{}'): {}",
            server_type, host, login, path, e
        ));
    }

    Ok(client)
}

/// Check if file should be transferred based on age and regex
///
/// Returns Some(file_size) if file should be transferred, None if should skip
fn check_file_should_transfer(
    client: &mut Client,
    filename: &str,
    min_age_seconds: u64,
    regex: &Regex,
    thread_id: usize,
) -> Option<usize> {
    // Check regex match
    if !regex.is_match(filename) {
        let _ = log_with_thread(
            format!(
                "Skipping file {} as it did not match regex {}",
                filename, regex
            ),
            Some(thread_id),
        );
        return None;
    }

    // Get file modification time
    let datetime_naive = match client.mdtm(filename) {
        Ok(dt) => dt,
        Err(e) => {
            let _ = log_with_thread(
                format!(
                    "Error getting modified time for file '{}': {}, skipping",
                    filename,
                    e.to_string().replace("\n", "")
                ),
                Some(thread_id),
            );
            return None;
        }
    };

    // Convert to SystemTime for age calculation
    let modified_system_time = {
        let secs = datetime_naive.and_utc().timestamp();
        let nanos = datetime_naive.and_utc().timestamp_subsec_nanos();
        if secs < 0 {
            let _ = log_with_thread(
                format!(
                    "File '{}' has a pre-epoch modification time ({}), skipping",
                    filename, datetime_naive
                ),
                Some(thread_id),
            );
            return None;
        }
        UNIX_EPOCH + Duration::new(secs as u64, nanos)
    };

    // Calculate file age
    let file_age_seconds = match SystemTime::now().duration_since(modified_system_time) {
        Ok(duration) => duration.as_secs(),
        Err(_) => {
            let _ = log_with_thread(
                format!(
                    "File '{}' has a modification time in the future ({} vs now), skipping",
                    filename, datetime_naive
                ),
                Some(thread_id),
            );
            return None;
        }
    };

    // Check age threshold
    if file_age_seconds < min_age_seconds {
        let _ = log_with_thread(
            format!(
                "Skipping file {}, it is {} seconds old, less than specified age {} seconds",
                filename, file_age_seconds, min_age_seconds
            ),
            Some(thread_id),
        );
        return None;
    }

    // Get file size
    match client.size(filename) {
        Ok(size) => Some(size),
        Err(e) => {
            let _ = log_with_thread(
                format!(
                    "Error getting size for file '{}': {}, skipping",
                    filename,
                    e.to_string().replace("\n", "")
                ),
                Some(thread_id),
            );
            None
        }
    }
}

/// Transfer buffer storage strategy
/// Encapsulates either RAM (Vec<u8>) or disk (NamedTempFile) storage
enum TransferBuffer {
    Ram(Vec<u8>),
    Disk(NamedTempFile),
}

impl TransferBuffer {
    /// Get the size of the buffer in bytes
    fn size(&self) -> u64 {
        match self {
            TransferBuffer::Ram(vec) => vec.len() as u64,
            TransferBuffer::Disk(temp_file) => temp_file
                .as_file()
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0),
        }
    }

    /// Create a reader for the buffer
    /// Returns Box<dyn Read> for unified interface
    fn into_reader(self) -> Box<dyn Read + Send> {
        match self {
            TransferBuffer::Ram(vec) => Box::new(Cursor::new(vec)),
            TransferBuffer::Disk(temp_file) => {
                // reopen() creates a new handle to the same file
                match temp_file.reopen() {
                    Ok(reader) => Box::new(reader),
                    Err(_) => {
                        // Fallback: try to read from the original file path
                        // This shouldn't happen in practice as NamedTempFile persists until dropped
                        Box::new(std::fs::File::open(temp_file.path()).unwrap_or_else(|_| {
                            std::io::stderr()
                                .write_all(b"Critical error: failed to open temp file\n")
                                .ok();
                            std::process::exit(1);
                        }))
                    }
                }
            }
        }
    }
}

/// Verify final file size after rename
///
/// Returns true if verification passed, false otherwise
fn verify_final_file(
    ftp_to: &mut Client,
    filename: &str,
    file_size: usize,
    thread_id: usize,
) -> bool {
    match ftp_to.size(filename) {
        Ok(actual_size) => {
            if actual_size == file_size {
                let _ = log_with_thread(
                    format!(
                        "Final file verification passed: '{}' is {} bytes",
                        filename, actual_size
                    ),
                    Some(thread_id),
                );
                true
            } else {
                let _ = log_with_thread(format!(
                    "ERROR: Final file verification FAILED: '{}' expected {} bytes, got {} bytes - transfer aborted",
                    filename, file_size, actual_size
                ), Some(thread_id));
                false
            }
        }
        Err(e) => {
            let _ = log_with_thread(format!(
                "ERROR: Final file verification error for '{}': {} - transfer aborted",
                filename, e
            ), Some(thread_id));
            false
        }
    }
}

/// Handle actions after successful rename (verification, logging, optional delete)
///
/// Returns true if all post-rename actions completed successfully
fn handle_successful_rename(
    ftp_to: &mut Client,
    ftp_from: &mut Client,
    filename: &str,
    file_size: usize,
    thread_id: usize,
    delete: bool,
) -> bool {
    let final_verified = verify_final_file(ftp_to, filename, file_size, thread_id);

    if final_verified {
        let _ = log_with_thread(
            format!("Successful transfer of file {}", filename),
            Some(thread_id),
        );

        // Delete source file only after successful transfer and verification
        if delete {
            match ftp_from.rm(filename) {
                Ok(_) => {
                    let _ = log_with_thread(
                        format!("Deleted SOURCE file {}", filename),
                        Some(thread_id),
                    );
                }
                Err(e) => {
                    let _ = log_with_thread(
                        format!("Error deleting SOURCE file {}: {}", filename, e),
                        Some(thread_id),
                    );
                }
            }
        }
        true
    } else {
        false
    }
}

/// Transfers files between FTP/FTPS servers according to configuration
///
/// # Arguments
/// * `config` - FTP connection and transfer parameters
/// * `delete` - Whether to delete source files after transfer
/// * `thread_id` - Identifier for logging in parallel mode
/// * `connect_timeout` - Connection timeout in seconds (None = 30s default)
/// * `insecure_skip_verify` - Whether to skip TLS certificate verification for FTPS
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
/// - Supports both FTP and FTPS protocols via proto_from/proto_to fields
/// - ALWAYS verifies upload size using SIZE command - transfer fails if verification fails
///
/// # Example
/// ```text
/// // let count = transfer_files(&config, true, 1, None, false, None, None);
/// ```
pub fn transfer_files(
    config: &Config,
    delete: bool,
    thread_id: usize,
    connect_timeout: Option<u64>,
    insecure_skip_verify: bool,
    temp_dir: Option<&str>,
    ram_threshold: Option<u64>,
) -> i32 {
    // Check for shutdown request before starting
    if is_shutdown_requested() {
        let _ = log_with_thread("Shutdown requested, skipping transfer", Some(thread_id));
        return 0;
    }

    let _ = log_with_thread(
        format!(
            "Transferring files from {}://{}@{}:{}{} to {}://{}@{}:{}{}",
            config.proto_from,
            config.login_from,
            config.ip_address_from,
            config.port_from,
            config.path_from,
            config.proto_to,
            config.login_to,
            config.ip_address_to,
            config.port_to,
            config.path_to
        ),
        Some(thread_id),
    );

    let timeout = Duration::from_secs(connect_timeout.unwrap_or(30));

    // Connect to source server
    let mut ftp_from = match connect_and_login(
        &config.proto_from,
        &config.ip_address_from,
        config.port_from,
        &config.login_from,
        config.password_from.as_ref().map(|s| s.expose_secret().as_str()),
        config.keyfile_from.as_deref(),
        &config.path_from,
        timeout,
        insecure_skip_verify,
        "SOURCE",
        thread_id,
    ) {
        Ok(client) => client,
        Err(e) => {
            let _ = log_with_thread(e, Some(thread_id));
            return 0;
        }
    };

    // Connect to target server
    let mut ftp_to = match connect_and_login(
        &config.proto_to,
        &config.ip_address_to,
        config.port_to,
        &config.login_to,
        config.password_to.as_ref().map(|s| s.expose_secret().as_str()),
        config.keyfile_to.as_deref(),
        &config.path_to,
        timeout,
        insecure_skip_verify,
        "TARGET",
        thread_id,
    ) {
        Ok(client) => client,
        Err(e) => {
            let _ = log_with_thread(e, Some(thread_id));
            let _ = ftp_from.quit();
            return 0;
        }
    };

    // Set binary mode once for both connections (outside the file loop)
    use crate::protocols::TransferMode;
    if let Err(e) = ftp_from.transfer_type(TransferMode::Binary) {
        let _ = log_with_thread(
            format!("Error setting binary mode on SOURCE FTP server: {}", e),
            Some(thread_id),
        );
        let _ = ftp_to.quit();
        let _ = ftp_from.quit();
        return 0;
    }

    if let Err(e) = ftp_to.transfer_type(TransferMode::Binary) {
        let _ = log_with_thread(
            format!("Error setting binary mode on TARGET FTP server: {}", e),
            Some(thread_id),
        );
        let _ = ftp_to.quit();
        let _ = ftp_from.quit();
        return 0;
    }

    let _ = log_with_thread(
        format!("TARGET {} binary mode set successfully", config.proto_to),
        Some(thread_id),
    );

    // Get the list of files in the source directory
    let file_list = match ftp_from.nlst(None) {
        Ok(list) => list,
        Err(e) => {
            let _ = log_with_thread(
                format!("Error getting file list from SOURCE FTP server: {}", e),
                Some(thread_id),
            );
            let _ = ftp_to.quit();
            let _ = ftp_from.quit();
            return 0;
        }
    };
    let number_of_files = file_list.len();
    let _ = log_with_thread(
        format!(
            "Number of files retrieved from SOURCE FTP server: {}",
            file_list.len()
        ),
        Some(thread_id),
    );

    // Compile regex once for all files (config parser already validated it)
    let regex = Regex::new(&config.filename_regexp)
        .expect("Regex pattern should be valid (validated in config parser)");

    let mut successful_transfers = 0;
    for filename in file_list {
        if is_shutdown_requested() {
            let _ = log_with_thread(
                "Shutdown requested, aborting remaining transfers",
                Some(thread_id),
            );
            break;
        }

        // Check if file should be transferred (regex, age, size)
        let Some(file_size) = check_file_should_transfer(
            &mut ftp_from,
            &filename,
            config.age,
            &regex,
            thread_id,
        ) else {
            continue;
        };

        // Determine actual threshold (default: 10MB)
        let actual_threshold = ram_threshold.unwrap_or(DEFAULT_RAM_THRESHOLD);

        // Determine storage method: RAM or disk
        // file_size is usize from SIZE command, actual_threshold is u64
        let use_ram = if actual_threshold == 0 {
            true // Force RAM for all files when threshold is 0
        } else {
            file_size as u64 <= actual_threshold
        };

        // Log the storage decision
        let storage = if use_ram { "RAM" } else { "disk" };
        let _ = log_with_thread(
            format!(
                "Using {} buffer for {} ({} bytes, threshold: {})",
                storage, filename, file_size, actual_threshold
            ),
            Some(thread_id),
        );

        // Use temporary filename for atomic transfer: .filename.{PID}.tmp
        let tmp_filename = format!(".{}.{}.tmp", filename, std::process::id());

        // Transfer with conditional storage (RAM or disk)
        let transfer_result = ftp_from.retr(filename.as_str(), |stream| {
            if use_ram {
                // RAM path: Vec<u8> buffer
                let mut buffer = Vec::with_capacity(file_size as usize);
                std::io::copy(stream, &mut buffer)
                    .map_err(suppaftp::FtpError::ConnectionError)?;
                Ok(TransferBuffer::Ram(buffer))
            } else {
                // Disk path: NamedTempFile
                let mut temp_file = match temp_dir {
                    Some(dir) => NamedTempFile::new_in(dir).map_err(|e| {
                        suppaftp::FtpError::ConnectionError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("tempfile create in {}: {}", dir, e),
                        ))
                    })?,
                    None => NamedTempFile::new().map_err(|e| {
                        suppaftp::FtpError::ConnectionError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("tempfile create: {}", e),
                        ))
                    })?,
                };
                // Log temp file path in debug mode
                let _ = log_debug(
                    format!("Using temp file: {}", temp_file.path().display()),
                    Some(thread_id),
                );
                std::io::copy(stream, &mut temp_file)
                    .map_err(suppaftp::FtpError::ConnectionError)?;
                Ok(TransferBuffer::Disk(temp_file))
            }
        });

        match transfer_result {
            Ok(buffer) => {
                let file_size = buffer.size();
                let file_size = usize::try_from(file_size).unwrap_or(usize::MAX);
                let _ = log_with_thread(
                    format!("Uploading file {} ({} bytes)", filename, file_size),
                    Some(thread_id),
                );

                // Upload the data to target server using put_file() with a reader
                // TransferBuffer::into_reader() returns Box<dyn Read + Send>
                let mut reader = buffer.into_reader();
                match ftp_to.put_file(tmp_filename.as_str(), &mut reader) {
                    Ok(bytes_written) => {
                        let _ = log_with_thread(
                            format!(
                                "Uploaded {} / {} bytes to TARGET as '{}'",
                                bytes_written, file_size, tmp_filename
                            ),
                            Some(thread_id),
                        );
                        // Sanity check: verify bytes_written matches expected size
                        if bytes_written != file_size as u64 {
                            let _ = log_with_thread(format!(
                                "WARNING: Size mismatch! Expected {} bytes, but put_file() reported {} bytes written",
                                file_size, bytes_written
                            ), Some(thread_id));
                        }

                        // Verify upload using SIZE command (MANDATORY - transfer fails if verification fails)
                        let _ = log_with_thread(
                            format!(
                                "Verifying upload of '{}' (expected {} bytes)...",
                                tmp_filename, file_size
                            ),
                            Some(thread_id),
                        );
                        let upload_verified = match ftp_to.size(tmp_filename.as_str()) {
                            Ok(actual_size) => {
                                if actual_size == file_size {
                                    let _ = log_with_thread(
                                        format!(
                                            "Upload verification passed: '{}' is {} bytes",
                                            tmp_filename, actual_size
                                        ),
                                        Some(thread_id),
                                    );
                                    true
                                } else {
                                    let _ = log_with_thread(format!(
                                        "ERROR: Upload verification FAILED: '{}' expected {} bytes, got {} bytes - transfer aborted",
                                        tmp_filename, file_size, actual_size
                                    ), Some(thread_id));
                                    false
                                }
                            }
                            Err(e) => {
                                let _ = log_with_thread(format!(
                                    "ERROR: Upload verification error for '{}': {} - transfer aborted",
                                    tmp_filename, e
                                ), Some(thread_id));
                                false
                            }
                        };

                        // Only proceed with rename if upload verification passed
                        if upload_verified {
                            // Upload successful, now rename the temporary file
                            // Atomic rename: first try to rename directly
                            let rename_result =
                                ftp_to.rename(tmp_filename.as_str(), filename.as_str());

                            match rename_result {
                                Ok(_) => {
                                    if handle_successful_rename(
                                        &mut ftp_to,
                                        &mut ftp_from,
                                        filename.as_str(),
                                        file_size,
                                        thread_id,
                                        delete,
                                    ) {
                                        successful_transfers += 1;
                                    }
                                }
                                Err(_) => {
                                    // Rename failed, likely because target file exists
                                    //
                                    // RENAME FALLBACK - DATA LOSS RISK:
                                    // ===================================
                                    // The FTP protocol does NOT provide an atomic "replace" operation.
                                    // When the target file exists, we must fall back to a non-atomic sequence:
                                    //
                                    // 1. First rename() fails (target file exists)
                                    // 2. rm() deletes the target file
                                    // 3. [DATA LOSS WINDOW] If crash/disconnect happens here:
                                    //    - Temp file (.filename.PID.tmp) remains on server
                                    //    - Target file is already deleted
                                    //    - Original source file still exists (not deleted yet)
                                    // 4. Second rename() completes
                                    //
                                    // Known limitation: This is an inherent constraint of the FTP protocol
                                    // (RFC 3659) which does not define an atomic replace operation.
                                    // After crashes, orphaned .*.tmp files may remain on the server
                                    // and require manual cleanup.
                                    //
                                    // Alternative protocols like SFTP may have different semantics,
                                    // but we implement consistent behavior across all protocols.
                                    if ftp_to.rm(filename.as_str()).is_ok() {
                                        let _ = log_with_thread(
                                            format!("Replaced existing file {}", filename),
                                            Some(thread_id),
                                        );
                                    }

                                    match ftp_to.rename(tmp_filename.as_str(), filename.as_str()) {
                                        Ok(_) => {
                                            if handle_successful_rename(
                                                &mut ftp_to,
                                                &mut ftp_from,
                                                filename.as_str(),
                                                file_size,
                                                thread_id,
                                                delete,
                                            ) {
                                                successful_transfers += 1;
                                            }
                                        }
                                        Err(e) => {
                                            let _ = log_with_thread(
                                                format!(
                                                    "Error renaming temporary file {} to {}: {}",
                                                    tmp_filename, filename, e
                                                ),
                                                Some(thread_id),
                                            );
                                            // Cleanup: try to remove the temporary file
                                            let _ = ftp_to.rm(tmp_filename.as_str());
                                        }
                                    }
                                }
                            }
                        } else {
                            // Upload verification failed - cleanup temp file and continue with next file
                            let _ = log_with_thread(
                                format!(
                                    "Cleaning up temporary file '{}' after failed verification",
                                    tmp_filename
                                ),
                                Some(thread_id),
                            );
                            let _ = ftp_to.rm(tmp_filename.as_str());
                        }
                    }
                    Err(e) => {
                        let _ = log_with_thread(format!(
                            "Error uploading file {} ({} bytes) to TARGET {}://{} (path '{}', user '{}'): {}",
                            filename, file_size, config.proto_to, config.ip_address_to, config.path_to, config.login_to, e
                        ), Some(thread_id));
                        // Cleanup: try to remove the temporary file
                        let _ = ftp_to.rm(tmp_filename.as_str());
                    }
                }
            }
            Err(e) => {
                let _ = log_with_thread(
                    format!(
                        "Error transferring file {} from SOURCE {}://{} server (user '{}'): {}",
                        filename, config.proto_from, config.ip_address_from, config.login_from, e
                    ),
                    Some(thread_id),
                );
            }
        }
    }
    let _ = ftp_to.quit();
    let _ = ftp_from.quit();
    let _ = log_with_thread(
        format!(
            "Successfully transferred {} files out of {} from {}://{}@{}:{}{} to {}://{}@{}:{}{}",
            successful_transfers,
            number_of_files,
            config.proto_from,
            config.login_from,
            config.ip_address_from,
            config.port_from,
            config.path_from,
            config.proto_to,
            config.login_to,
            config.ip_address_to,
            config.port_to,
            config.path_to
        ),
        Some(thread_id),
    );
    successful_transfers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Protocol;
    use crate::shutdown::{request_shutdown, reset_shutdown_for_tests};
    use serial_test::serial;
    use secrecy::Secret;

    #[test]
    #[serial]
    fn test_transfer_files_shutdown_before_start() {
        // Reset shutdown state first
        reset_shutdown_for_tests();

        // Request shutdown before calling transfer_files
        request_shutdown();

        let config = Config {
            ip_address_from: "127.0.0.1".to_string(),
            port_from: 21,
            login_from: "test".to_string(),
            password_from: Some(Secret::new("test".to_string())),
            keyfile_from: None,
            path_from: "/test/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "127.0.0.2".to_string(),
            port_to: 21,
            login_to: "test".to_string(),
            password_to: Some(Secret::new("test".to_string())),
            keyfile_to: None,
            path_to: "/test/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };

        let result = transfer_files(&config, false, 1, None, false, None, None);
        assert_eq!(
            result, 0,
            "Should return 0 when shutdown requested before start"
        );

        // Reset shutdown flag for other tests
        reset_shutdown_for_tests();
    }

    #[test]
    #[serial]
    fn test_regex_compilation() {
        // Test that regex compiles correctly
        let config = Config {
            ip_address_from: "127.0.0.1".to_string(),
            port_from: 21,
            login_from: "test".to_string(),
            password_from: Some(Secret::new("test".to_string())),
            keyfile_from: None,
            path_from: "/test/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "127.0.0.2".to_string(),
            port_to: 21,
            login_to: "test".to_string(),
            password_to: Some(Secret::new("test".to_string())),
            keyfile_to: None,
            path_to: "/test/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: r".*\.txt$".to_string(),
        };

        // This should not panic - regex should compile
        let regex = Regex::new(&config.filename_regexp);
        assert!(regex.is_ok());

        let re = regex.unwrap();
        assert!(re.is_match("file.txt"));
        assert!(!re.is_match("file.csv"));
    }

    #[test]
    #[serial]
    fn test_regex_special_patterns() {
        // Test various regex patterns that should compile
        let patterns = vec![
            r".*",
            r"^[A-Z].*\.log$",
            r"\d{4}-\d{2}-\d{2}.*",
            r"test_.*\.csv",
        ];

        for pattern in patterns {
            let regex = Regex::new(pattern);
            assert!(regex.is_ok(), "Pattern '{}' should compile", pattern);
        }
    }
}
