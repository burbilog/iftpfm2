use crate::config::Config;
use crate::logging::log_with_thread;
use crate::protocols::Client;
use crate::shutdown::is_shutdown_requested;
use regex::Regex;
use std::io::Cursor;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
/// // let count = transfer_files(&config, true, 1, None, false);
/// ```
pub fn transfer_files(
    config: &Config,
    delete: bool,
    thread_id: usize,
    connect_timeout: Option<u64>,
    insecure_skip_verify: bool,
) -> i32 {
    // Check for shutdown request before starting
    if is_shutdown_requested() {
        let _ = log_with_thread("Shutdown requested, skipping transfer", Some(thread_id));
        return 0;
    }

    let _ = log_with_thread(
        format!(
            "Transferring files from {}://{}:{}{} to {}://{}:{}{}",
            config.proto_from,
            config.ip_address_from,
            config.port_from,
            config.path_from,
            config.proto_to,
            config.ip_address_to,
            config.port_to,
            config.path_to
        ),
        Some(thread_id),
    );

    let timeout = Duration::from_secs(connect_timeout.unwrap_or(30));

    // Connect to the source FTP server
    let mut ftp_from = match Client::connect(
        &config.proto_from,
        &config.ip_address_from,
        config.port_from,
        timeout,
        insecure_skip_verify,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            let _ = log_with_thread(
                format!(
                    "Error connecting to SOURCE FTP server {}:{} ({}s timeout): {}",
                    config.ip_address_from,
                    config.port_from,
                    connect_timeout.unwrap_or(30),
                    e
                ),
                Some(thread_id),
            );
            return 0;
        }
    };

    if let Err(e) = ftp_from.login(config.login_from.as_str(), config.password_from.as_str()) {
        let _ = log_with_thread(
            format!(
                "Error logging into SOURCE FTP server {}: {}",
                config.ip_address_from, e
            ),
            Some(thread_id),
        );
        let _ = ftp_from.quit();
        return 0;
    }
    if let Err(e) = ftp_from.cwd(config.path_from.as_str()) {
        let _ = log_with_thread(
            format!(
                "Error changing directory on SOURCE FTP server {} (user '{}', path '{}'): {}",
                config.ip_address_from, config.login_from, config.path_from, e
            ),
            Some(thread_id),
        );
        let _ = ftp_from.quit();
        return 0;
    }

    // Connect to the target FTP server
    let mut ftp_to = match Client::connect(
        &config.proto_to,
        &config.ip_address_to,
        config.port_to,
        timeout,
        insecure_skip_verify,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            let _ = log_with_thread(
                format!(
                    "Error connecting to TARGET FTP server {}:{} ({}s timeout): {}",
                    config.ip_address_to,
                    config.port_to,
                    connect_timeout.unwrap_or(30),
                    e
                ),
                Some(thread_id),
            );
            let _ = ftp_from.quit();
            return 0;
        }
    };

    if let Err(e) = ftp_to.login(config.login_to.as_str(), config.password_to.as_str()) {
        let _ = log_with_thread(
            format!(
                "Error logging into TARGET FTP server {}: {}",
                config.ip_address_to, e
            ),
            Some(thread_id),
        );
        let _ = ftp_to.quit();
        let _ = ftp_from.quit();
        return 0;
    }

    let _ = log_with_thread(
        format!("TARGET FTPS login successful, proto_to={}", config.proto_to),
        Some(thread_id),
    );
    if let Err(e) = ftp_to.cwd(config.path_to.as_str()) {
        let _ = log_with_thread(
            format!(
                "Error changing directory on TARGET FTP server {} (user '{}', path '{}'): {}",
                config.ip_address_to, config.login_to, config.path_to, e
            ),
            Some(thread_id),
        );
        let _ = ftp_to.quit();
        let _ = ftp_from.quit();
        return 0;
    }

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

    let _ = log_with_thread("TARGET FTPS binary mode set successfully", Some(thread_id));

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

        if !regex.is_match(&filename) {
            let _ = log_with_thread(
                format!(
                    "Skipping file {} as it did not match regex {}",
                    filename, regex
                ),
                Some(thread_id),
            );
            continue;
        }

        // Get the modified time of the file on the FTP server.
        // FileTransferClient::mdtm returns Result<chrono::NaiveDateTime, FtpError>
        let datetime_naive = match ftp_from.mdtm(filename.as_str()) {
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
                continue;
            }
        };

        // Convert NaiveDateTime to SystemTime for age calculation.
        // NaiveDateTime has no timezone, so we assume it's UTC for MDTM purposes.
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
                continue;
            }
            UNIX_EPOCH + Duration::new(secs as u64, nanos)
        };

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
                continue;
            }
        };

        if file_age_seconds < config.age {
            let _ = log_with_thread(
                format!(
                    "Skipping file {}, it is {} seconds old, less than specified age {} seconds",
                    filename, file_age_seconds, config.age
                ),
                Some(thread_id),
            );
            continue;
        }

        // Use temporary filename for atomic transfer: .filename.{PID}.tmp
        let tmp_filename = format!(".{}.{}.tmp", filename, std::process::id());

        // Transfer to temporary file first for atomicity
        // FileTransferClient uses retr() with a reader callback for download
        let transfer_result = ftp_from.retr(filename.as_str(), |stream| {
            let mut data = Vec::new();
            let reader = stream;
            reader
                .read_to_end(&mut data)
                .map_err(suppaftp::FtpError::ConnectionError)?;
            Ok(data)
        });

        match transfer_result {
            Ok(data) => {
                let file_size = data.len();
                let _ = log_with_thread(
                    format!("Uploading file {} ({} bytes)", filename, file_size),
                    Some(thread_id),
                );
                // Upload the data to target server using put_file() with a reader
                let mut reader = Cursor::new(data);
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
                                    // Verify final file after rename (MANDATORY)
                                    let final_verified = match ftp_to.size(filename.as_str()) {
                                        Ok(actual_size) => {
                                            if actual_size == file_size {
                                                let _ = log_with_thread(format!(
                                                    "Final file verification passed: '{}' is {} bytes",
                                                    filename, actual_size
                                                ), Some(thread_id));
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
                                    };

                                    if final_verified {
                                        let _ = log_with_thread(
                                            format!("Successful transfer of file {}", filename),
                                            Some(thread_id),
                                        );
                                        successful_transfers += 1;
                                        // Delete source file only after successful transfer and verification
                                        if delete {
                                            match ftp_from.rm(filename.as_str()) {
                                                Ok(_) => {
                                                    let _ = log_with_thread(
                                                        format!("Deleted SOURCE file {}", filename),
                                                        Some(thread_id),
                                                    );
                                                }
                                                Err(e) => {
                                                    let _ = log_with_thread(
                                                        format!(
                                                            "Error deleting SOURCE file {}: {}",
                                                            filename, e
                                                        ),
                                                        Some(thread_id),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Rename failed, likely because target file exists
                                    // Delete old file and retry rename
                                    if ftp_to.rm(filename.as_str()).is_ok() {
                                        let _ = log_with_thread(
                                            format!("Replaced existing file {}", filename),
                                            Some(thread_id),
                                        );
                                    }

                                    match ftp_to.rename(tmp_filename.as_str(), filename.as_str()) {
                                        Ok(_) => {
                                            // Verify final file after rename (MANDATORY)
                                            let final_verified = match ftp_to
                                                .size(filename.as_str())
                                            {
                                                Ok(actual_size) => {
                                                    if actual_size == file_size {
                                                        let _ = log_with_thread(format!(
                                                            "Final file verification passed: '{}' is {} bytes",
                                                            filename, actual_size
                                                        ), Some(thread_id));
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
                                            };

                                            if final_verified {
                                                let _ = log_with_thread(
                                                    format!(
                                                        "Successful transfer of file {}",
                                                        filename
                                                    ),
                                                    Some(thread_id),
                                                );
                                                successful_transfers += 1;
                                                // Delete source file only after successful transfer and verification
                                                if delete {
                                                    match ftp_from.rm(filename.as_str()) {
                                                        Ok(_) => {
                                                            let _ = log_with_thread(
                                                                format!(
                                                                    "Deleted SOURCE file {}",
                                                                    filename
                                                                ),
                                                                Some(thread_id),
                                                            );
                                                        }
                                                        Err(e) => {
                                                            let _ = log_with_thread(format!("Error deleting SOURCE file {}: {}", filename, e), Some(thread_id));
                                                        }
                                                    }
                                                }
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
            "Successfully transferred {} files out of {}",
            successful_transfers, number_of_files
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
            password_from: "test".to_string(),
            path_from: "/test/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "127.0.0.2".to_string(),
            port_to: 21,
            login_to: "test".to_string(),
            password_to: "test".to_string(),
            path_to: "/test/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };

        let result = transfer_files(&config, false, 1, None, false);
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
            password_from: "test".to_string(),
            path_from: "/test/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "127.0.0.2".to_string(),
            port_to: 21,
            login_to: "test".to_string(),
            password_to: "test".to_string(),
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
