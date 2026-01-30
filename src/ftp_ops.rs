use crate::config::Config;
use crate::logging::log_with_thread;
use crate::shutdown::is_shutdown_requested;
use ftp::FtpStream;
use regex::Regex;
use std::time::{Duration, SystemTime, UNIX_EPOCH}; // Moved Duration and UNIX_EPOCH here

/// Transfers files between FTP servers according to configuration
///
/// # Arguments
/// * `config` - FTP connection and transfer parameters
/// * `delete` - Whether to delete source files after transfer
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
/// ```text
/// // let count = transfer_files(&config, true, 1);
/// ```
pub fn transfer_files(config: &Config, delete: bool, thread_id: usize) -> i32 {
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
            // This return should be handled better, perhaps by propagating the error.
            // For now, matching original behavior of logging and returning.
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
            // Similar to above, error handling could be improved.
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
    let regex = match Regex::new(&config.filename_regexp) {
        Ok(re) => re,
        Err(e) => {
            log_with_thread(
                &format!("Invalid filename regex pattern '{}': {}", config.filename_regexp, e),
                Some(thread_id),
            ).unwrap();
            return 0;
        }
    };

    let mut successful_transfers = 0;
    for filename in file_list {
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

        // Get the modified time of the file on the FTP server.
        // ftp::FtpStream::mdtm returns Result<Option<ftp_chrono::DateTime<ftp_chrono::Utc>>>
        // Let type inference handle `datetime_utc_ftp_chrono`
        let datetime_utc_ftp_chrono = match ftp_from.mdtm(filename.as_str()) {
            Ok(Some(dt)) => dt, // dt is ftp_chrono::DateTime<Utc>
            Ok(None) => {
                log_with_thread(&format!(
                    "MDTM command not supported or file '{}' has no timestamp, skipping",
                    filename
                ), Some(thread_id))
                .unwrap();
                continue;
            }
            Err(e) => {
                log_with_thread(&format!(
                    "Error getting modified time for file '{}': {}, skipping",
                    filename,
                    e.to_string().replace("\n", "")
                ), Some(thread_id))
                .unwrap();
                continue;
            }
        };

        // Convert ftp_chrono::DateTime<Utc> to SystemTime for age calculation.
        // datetime_utc_ftp_chrono is from chrono 0.2.x via the ftp crate.
        let modified_system_time = {
            // Duration and UNIX_EPOCH are now imported at the top of the file.
            let secs = datetime_utc_ftp_chrono.timestamp();
            let nanos = datetime_utc_ftp_chrono.timestamp_subsec_nanos(); // This is u32 in chrono 0.2.x
            if secs < 0 {
                 // Handle pre-epoch times if necessary, though unlikely for FTP MDTM.
                 // For simplicity, we might log an error and skip, or use UNIX_EPOCH as a fallback.
                 // Here, we'll assume positive timestamps for typical FTP server file times.
                 // If secs is negative, UNIX_EPOCH + Duration::new(secs as u64, nanos) would panic.
                 // A more robust solution might involve conditional subtraction from UNIX_EPOCH.
                 // For now, let's proceed assuming MDTM gives times at or after epoch.
                 // If this assumption is wrong, this part needs more robust handling.
                log_with_thread(&format!(
                    "File '{}' has a pre-epoch modification time ({}), skipping",
                    filename, datetime_utc_ftp_chrono
                ), Some(thread_id)).unwrap();
                continue;
            }
            UNIX_EPOCH + Duration::new(secs as u64, nanos)
        };

        let file_age_seconds = match SystemTime::now().duration_since(modified_system_time) {
            Ok(duration) => duration.as_secs(),
            Err(_) => { // SystemTime::now() is earlier than modified_system_time (file from the future)
                log_with_thread(&format!(
                    "File '{}' has a modification time in the future ({} vs now), skipping",
                    filename, datetime_utc_ftp_chrono // Log the original ftp_chrono time
                ), Some(thread_id))
                .unwrap();
                continue;
            }
        };

        if file_age_seconds < config.age {
            log_with_thread(format!(
                "Skipping file {}, it is {} seconds old, less than specified age {} seconds",
                filename, file_age_seconds, config.age
            )
            .as_str(), Some(thread_id))
            .unwrap();
            continue;
        }

        match ftp_to.rm(filename.as_str()) {
            Ok(_) => {
                log_with_thread(format!("Deleted file {} at TARGET FTP server", filename).as_str(), Some(thread_id)).unwrap()
            }
            Err(_) => (), // Ignore error if file doesn't exist
        };

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
