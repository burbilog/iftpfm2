use ftp::FtpStream;
use std::time::SystemTime;
use chrono::DateTime; 

use crate::log;
use crate::conf;

pub fn transfer_files(config: &conf::Config, delete: bool, ext: Option<String>) {
    log::log(format!("Transferring files from ftp://{}:{}{} to ftp://{}:{}{}",
        config.ip_address_from, config.port_from, config.path_from,
        config.ip_address_to, config.port_to, config.path_to).as_str()).unwrap();
    // Connect to the source FTP server
    let mut ftp_from = match FtpStream::connect((config.ip_address_from.as_str(), config.port_from)) {
        Ok(ftp) => ftp,
        Err(e) => {
            log::log(format!("Error connecting to SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        },
    };
    ftp_from.login(config.login_from.as_str(), config.password_from.as_str())
        .unwrap_or_else(|e| {
            log::log(format!("Error logging into SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        });
    match ftp_from.cwd(config.path_from.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log::log(format!("Error changing directory on SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        },
    }

    // Connect to the target FTP server
    let mut ftp_to = match FtpStream::connect((config.ip_address_to.as_str(), config.port_to)) {
        Ok(ftp) => ftp,
        Err(e) => {
            log::log(format!("Error connecting to TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        },
    };
    ftp_to.login(config.login_to.as_str(), config.password_to.as_str())
        .unwrap_or_else(|e| {
            log::log(format!("Error logging into TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        });
    match ftp_to.cwd(config.path_to.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log::log(format!("Error changing directory on TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        },
    }

    // Get the list of files in the source directory
    let file_list = match ftp_from.nlst(ext.as_ref().map(String::as_str)) {
        Ok(list) => list,
        Err(e) => {
            log::log(format!("Error getting file list from SOURCE FTP server: {}", e).as_str()).unwrap();
            return;
        },
    };
    log::log(format!("Number of files retrieved using pattern '{:?}': {}", ext, file_list.len()).as_str()).unwrap();

    // Transfer each file from the source to the target directory
    for filename in file_list {
        //log::log(format!("Working on file {}", filename).as_str()).unwrap();
        // Get the modified time of the file on the FTP server
        let modified_time_str = match ftp_from.mdtm(filename.as_str()) {
            Ok(time) => {
                // too noisy
                //log::log(&format!("Successfully retrieved modified time '{}' for file '{}'", time.unwrap(), filename)).unwrap();
                time.unwrap()
            },
            Err(e) => {
                //log::log(&format!("Error getting modified time for file(?) '{}': '{}', skipping", filename, e)).unwrap();
                log::log(&format!("Error getting modified time, skipping file(?) '{}': {}", filename, e.to_string().replace("\n", ""))).unwrap();
                continue;
            }
        };
        let modified_time_replaced_utc = modified_time_str.to_string().replace("UTC","+0000");
        let modified_time = match DateTime::parse_from_str(modified_time_replaced_utc.as_str(), "%Y-%m-%d %H:%M:%S %z") {
            Ok(time) => time.into(),
            Err(err) => {
                log::log(&format!("Error parsing modified time '{}': {}", modified_time_str, err)).unwrap();
                continue;
            }
        };

        //log::log(format!("modified_time: {:?}", modified_time).as_str()).unwrap();
        //log::log(format!("system time: {:?}", SystemTime::now()).as_str()).unwrap();

        // Calculate the age of the file
        let file_age = match SystemTime::now().duration_since(modified_time) {
            Ok(duration) => duration.as_secs(),
            Err(_) => {
                log::log(&format!("Error calculating age for file '{}', skipping", filename)).unwrap();
                continue;
            }
        };

        // Skip the file if it is younger than the specified age
        if file_age < (config.age as u64) {
            log::log(format!("Skipping file {}, it is {} seconds old, less than specified age {} seconds", filename, file_age, config.age).as_str()).unwrap();
            continue;
        }
        //log::log(format!("Transferring file {}", filename).as_str()).unwrap();
        match ftp_to.rm(filename.as_str()) {
            Ok(_) => log::log(format!("Deleted file {} at TARGET FTP server", filename).as_str()).unwrap(),
            Err(_) => (),
        };
        match ftp_from.simple_retr(filename.as_str()) {
            Ok(mut data) => {
                match ftp_to.put(filename.as_str(), &mut data) {
                    Ok(_) => {
                        log::log(format!("Successful transfer of file {}", filename).as_str()).unwrap();
                    },
                    Err(e) => {
                        log::log(format!("Error transferring file {} to TARGET FTP server: {}", filename, e).as_str()).unwrap();
                        continue;
                    }
                }
            },
            Err(e) => {
                log::log(format!("Error transferring file {} from SOURCE FTP server: {}", filename, e).as_str()).unwrap();
                continue;
            }
        }

        // Delete the source file if specified
        if delete {
            match ftp_from.rm(filename.as_str()) {
                Ok(_) => {
                    log::log(format!("Deleted SOURCE file {}", filename).as_str()).unwrap();
                },
                Err(e) => {
                    log::log(format!("Error deleting SOURCE file {}: {}", filename, e).as_str()).unwrap();
                }
            }
        }
    }
}

