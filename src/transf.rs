//use std::io;

use ftp::FtpStream;

use crate::log;
use crate::conf;

pub fn transfer_files(config: &conf::Config, delete: bool) {
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
    //let file_list = ftp_from.nlst(None).unwrap();
    let file_list = match ftp_from.nlst(None) {
        Ok(list) => list,
        Err(e) => {
            log::log(format!("Error getting file list from SOURCE FTP server: {}", e).as_str()).unwrap();
            return;
        },
    };

    // Transfer each file from the source to the target directory
    for filename in file_list {
        log::log(format!("Transferring file {}", filename).as_str()).unwrap();
        match ftp_to.rm(filename.as_str()) {
            Ok(_) => log::log(format!("Deleted file {} at TARGET FTP server", filename).as_str()).unwrap(),
            Err(_) => (),
        };
        match ftp_from.simple_retr(filename.as_str()) {
            Ok(mut data) => {
                match ftp_to.put(filename.as_str(), &mut data) {
                    Ok(_) => {},
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

