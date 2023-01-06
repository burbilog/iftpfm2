// Logs a message to a file or stdout.
//
// If the global `LOG_FILE` static variable is set, the message is appended to the specified file.
// If `LOG_FILE` is not set, the message is printed to stdout.

use std::fs::{OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use chrono::{Local};

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

