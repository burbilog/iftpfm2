use std::env;
use std::process;
use std::fs::File;
use std::io::{BufReader, BufRead, Error, ErrorKind};
use std::str::FromStr;
use std::fs::{OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use chrono::{Local};
use ftp::FtpStream;
use std::time::SystemTime;
use chrono::DateTime;
use regex::Regex;


fn print_usage() {
    println!(
        "Usage: {} [-h] [-v] [-d] [-x \".*\\.xml\"] [-l logfile] config_file",
        PROGRAM_NAME
    );
}

pub fn parse_args() -> (bool, Option<String>, Option<String>, Option<String>) {
    let mut log_file = None;
    let mut delete = false;
    let mut config_file = None;
    let mut ext = None;


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

    (delete, log_file, config_file, ext)
}


#[derive(Debug, PartialEq)]
pub struct Config {
    pub ip_address_from: String,
    pub port_from: u16,
    pub login_from: String,
    pub password_from: String,
    pub path_from: String,
    pub ip_address_to: String,
    pub port_to: u16,
    pub login_to: String,
    pub password_to: String,
    pub path_to: String,
    pub age: u64,
}

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
        let ip_address_from = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: ip_address_from"))?.to_string();
        let port_from = u16::from_str(fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: port_from"))?).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let login_from = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: login_from"))?.to_string();
        let password_from = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: password_from"))?.to_string();
        let path_from = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: path_from"))?.to_string();
        let ip_address_to = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: ip_address_to"))?.to_string();
        let port_to = u16::from_str(fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: port_to"))?).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let login_to = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: login_to"))?.to_string();
        let password_to = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: password_to"))?.to_string();
        let path_to = fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: path_to"))?.to_string();
        let age = u64::from_str(fields.next().ok_or(Error::new(ErrorKind::InvalidInput, "missing field: age"))?).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;

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
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use super::Config;

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

// Logs a message to a file or stdout.
//
// If the global `LOG_FILE` static variable is set, the message is appended to the specified file.
// If `LOG_FILE` is not set, the message is printed to stdout.


pub static mut LOG_FILE: Option<String> = None;

// Logs the given message to the file specified by the global `LOG_FILE` static variable.
// If `LOG_FILE` is not set, logs the message to stdout.
pub fn log(message: &str) -> io::Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_message = format!("{} {}\n", timestamp, message);

    unsafe {
        match &LOG_FILE {
            Some(log_file) => {
                let mut file = OpenOptions::new().create(true).append(true).open(log_file)?;
                file.write_all(log_message.as_bytes())?;
            }
            None => {
                println!("{}", log_message);
            }
        }
    }

    Ok(())
}

// Sets the global `LOG_FILE` static variable to the specified file path.
// set_log_file("/path/to/log/file");
pub fn set_log_file<P: AsRef<Path>>(path: P) {
    let path = path.as_ref().to_str().unwrap();
    unsafe {
        LOG_FILE = Some(path.to_string());
    }
}

#[cfg(test)]
use tempfile::tempdir;
#[cfg(test)]
use std::fs::remove_file;

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


pub fn transfer_files(config: &Config, delete: bool, ext: Option<String>) {
    log(format!("Transferring files from ftp://{}:{}{} to ftp://{}:{}{}",
        config.ip_address_from, config.port_from, config.path_from,
        config.ip_address_to, config.port_to, config.path_to).as_str()).unwrap();
    // Connect to the source FTP server
    let mut ftp_from = match FtpStream::connect((config.ip_address_from.as_str(), config.port_from)) {
        Ok(ftp) => ftp,
        Err(e) => {
            log(format!("Error connecting to SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        },
    };
    ftp_from.login(config.login_from.as_str(), config.password_from.as_str())
        .unwrap_or_else(|e| {
            log(format!("Error logging into SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        });
    match ftp_from.cwd(config.path_from.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log(format!("Error changing directory on SOURCE FTP server {}: {}", config.ip_address_from, e).as_str()).unwrap();
            return;
        },
    }

    // Connect to the target FTP server
    let mut ftp_to = match FtpStream::connect((config.ip_address_to.as_str(), config.port_to)) {
        Ok(ftp) => ftp,
        Err(e) => {
            log(format!("Error connecting to TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        },
    };
    ftp_to.login(config.login_to.as_str(), config.password_to.as_str())
        .unwrap_or_else(|e| {
            log(format!("Error logging into TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        });
    match ftp_to.cwd(config.path_to.as_str()) {
        Ok(_) => (),
        Err(e) => {
            log(format!("Error changing directory on TARGET FTP server {}: {}", config.ip_address_to, e).as_str()).unwrap();
            return;
        },
    }

    // Get the list of files in the source directory
    // Do not use NLST with paramter because pyftpdlib does not understand that
    let file_list = match ftp_from.nlst(None) {
        Ok(list) => list,
        Err(e) => {
            log(format!("Error getting file list from SOURCE FTP server: {}", e).as_str()).unwrap();
            return;
        },
    };
    log(format!("Number of files retrieved using pattern: {}", file_list.len()).as_str()).unwrap();
    let ext_regex = match ext.as_ref().map(String::as_str) {
        Some(ext) => Regex::new(ext),
        None => {
            // Handle the case where `ext` is None
            log(&format!("FUCK")).unwrap();
            return;
        }
    };
    let regex = ext_regex.unwrap();
    // Transfer each file from the source to the target directory
    for filename in file_list {
        if !regex.is_match(&filename) {
            log(format!("Skipping file {} as it did not match regex {}", filename, regex).as_str()).unwrap();
            continue;
        }
        //log(format!("Working on file {}", filename).as_str()).unwrap();
        // Get the modified time of the file on the FTP server
        let modified_time_str = match ftp_from.mdtm(filename.as_str()) {
            Ok(time) => {
                // too noisy
                //log(&format!("Successfully retrieved modified time '{}' for file '{}'", time.unwrap(), filename)).unwrap();
                time.unwrap()
            },
            Err(e) => {
                //log(&format!("Error getting modified time for file(?) '{}': '{}', skipping", filename, e)).unwrap();
                log(&format!("Error getting modified time, skipping file(?) '{}': {}", filename, e.to_string().replace("\n", ""))).unwrap();
                continue;
            }
        };
        let modified_time_replaced_utc = modified_time_str.to_string().replace("UTC","+0000");
        let modified_time = match DateTime::parse_from_str(modified_time_replaced_utc.as_str(), "%Y-%m-%d %H:%M:%S %z") {
            Ok(time) => time.into(),
            Err(err) => {
                log(&format!("Error parsing modified time '{}': {}", modified_time_str, err)).unwrap();
                continue;
            }
        };

        //log(format!("modified_time: {:?}", modified_time).as_str()).unwrap();
        //log(format!("system time: {:?}", SystemTime::now()).as_str()).unwrap();

        // Calculate the age of the file
        let file_age = match SystemTime::now().duration_since(modified_time) {
            Ok(duration) => duration.as_secs(),
            Err(_) => {
                log(&format!("Error calculating age for file '{}', skipping", filename)).unwrap();
                continue;
            }
        };

        // Skip the file if it is younger than the specified age
        if file_age < (config.age as u64) {
            log(format!("Skipping file {}, it is {} seconds old, less than specified age {} seconds", filename, file_age, config.age).as_str()).unwrap();
            continue;
        }
        //log(format!("Transferring file {}", filename).as_str()).unwrap();
        match ftp_to.rm(filename.as_str()) {
            Ok(_) => log(format!("Deleted file {} at TARGET FTP server", filename).as_str()).unwrap(),
            Err(_) => (),
        };
        match ftp_from.simple_retr(filename.as_str()) {
            Ok(mut data) => {
                match ftp_to.put(filename.as_str(), &mut data) {
                    Ok(_) => {
                        log(format!("Successful transfer of file {}", filename).as_str()).unwrap();
                    },
                    Err(e) => {
                        log(format!("Error transferring file {} to TARGET FTP server: {}", filename, e).as_str()).unwrap();
                        continue;
                    }
                }
            },
            Err(e) => {
                log(format!("Error transferring file {} from SOURCE FTP server: {}", filename, e).as_str()).unwrap();
                continue;
            }
        }

        // Delete the source file if specified
        if delete {
            match ftp_from.rm(filename.as_str()) {
                Ok(_) => {
                    log(format!("Deleted SOURCE file {}", filename).as_str()).unwrap();
                },
                Err(e) => {
                    log(format!("Error deleting SOURCE file {}: {}", filename, e).as_str()).unwrap();
                }
            }
        }
    }
}

const PROGRAM_NAME: &str = "iftpfm2";
const PROGRAM_VERSION: &str = "2.0.0";

fn main() {
    // Parse arguments and setup logging
    let (delete, log_file, config_file, ext) = parse_args();
    if let Some(log_file) = log_file {
            set_log_file(log_file);
    }

    log(format!("{} version {} started", PROGRAM_NAME, PROGRAM_VERSION).as_str()).unwrap();

    // Parse config file
    let config_file = config_file.unwrap();
    let configs = parse_config(&config_file).unwrap();

    // Loop over each line in config file
    for cf in configs {
        transfer_files(&cf, delete, ext.clone());
    }

    log(format!("{} version {} finished", PROGRAM_NAME, PROGRAM_VERSION).as_str()).unwrap();
}


