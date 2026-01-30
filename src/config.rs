use regex::Regex;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind};

/// FTP transfer configuration parameters
#[derive(Debug, PartialEq, Deserialize)]
pub struct Config {
    /// Source FTP server IP/hostname (JSON field: host_from)
    #[serde(rename = "host_from")]
    pub ip_address_from: String,
    /// Source FTP server port (typically 21) (JSON field: port_from)
    #[serde(rename = "port_from")]
    pub port_from: u16,
    /// Username for source FTP server (JSON field: login_from)
    #[serde(rename = "login_from")]
    pub login_from: String,
    /// Password for source FTP server (JSON field: password_from)
    #[serde(rename = "password_from")]
    pub password_from: String,
    /// Source directory path (must be literal path, no wildcards) (JSON field: path_from)
    #[serde(rename = "path_from")]
    pub path_from: String,
    /// Destination FTP server IP/hostname (JSON field: host_to)
    #[serde(rename = "host_to")]
    pub ip_address_to: String,
    /// Destination FTP server port (typically 21) (JSON field: port_to)
    #[serde(rename = "port_to")]
    pub port_to: u16,
    /// Username for destination FTP server (JSON field: login_to)
    #[serde(rename = "login_to")]
    pub login_to: String,
    /// Password for destination FTP server (JSON field: password_to)
    #[serde(rename = "password_to")]
    pub password_to: String,
    /// Destination directory path (JSON field: path_to)
    #[serde(rename = "path_to")]
    pub path_to: String,
    /// Minimum file age to transfer (seconds) (JSON field: age)
    #[serde(rename = "age")]
    pub age: u64,
    /// Regular expression pattern for filename matching (JSON field: filename_regexp)
    #[serde(rename = "filename_regexp")]
    pub filename_regexp: String,
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
/// - Invalid JSON format
/// - Missing required fields
/// - Invalid regex pattern
///
/// # File Format
/// JSONL format - one JSON object per line with fields:
/// host_from, port_from, login_from, password_from, path_from,
/// host_to, port_to, login_to, password_to, path_to, age, filename_regexp
///
/// # Example
/// ```text
/// // let configs = parse_config("settings.jsonl")?;
/// ```
pub fn parse_config(filename: &str) -> Result<Vec<Config>, Error> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let mut configs = Vec::new();
    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse JSON line
        let config: Config = serde_json::from_str(line).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid JSON on line {}: {}", line_num + 1, e),
            )
        })?;

        // Validate the regex pattern
        Regex::new(&config.filename_regexp).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid filename regex pattern on line {}: {}", line_num + 1, e),
            )
        })?;

        configs.push(config);
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_parse_config() {
        let config_string = r#"{"host_from":"192.168.0.1","port_from":22,"login_from":"user1","password_from":"password1","path_from":"/path/to/files/","host_to":"192.168.0.2","port_to":22,"login_to":"user2","password_to":"password2","path_to":"/path/to/files2","age":30,"filename_regexp":".*"}
{"host_from":"192.168.0.3","port_from":22,"login_from":"user3","password_from":"password3","path_from":"/path/to/files3/","host_to":"192.168.0.4","port_to":22,"login_to":"user4","password_to":"password4","path_to":"/path/to/files4","age":60,"filename_regexp":".*"}"#;
        let expected = vec![
            Config {
                ip_address_from: "192.168.0.1".to_string(),
                port_from: 22,
                login_from: "user1".to_string(),
                password_from: "password1".to_string(),
                path_from: "/path/to/files/".to_string(),
                ip_address_to: "192.168.0.2".to_string(),
                port_to: 22,
                login_to: "user2".to_string(),
                password_to: "password2".to_string(),
                path_to: "/path/to/files2".to_string(),
                age: 30,
                filename_regexp: ".*".to_string(),
            },
            Config {
                ip_address_from: "192.168.0.3".to_string(),
                port_from: 22,
                login_from: "user3".to_string(),
                password_from: "password3".to_string(),
                path_from: "/path/to/files3/".to_string(),
                ip_address_to: "192.168.0.4".to_string(),
                port_to: 22,
                login_to: "user4".to_string(),
                password_to: "password4".to_string(),
                path_to: "/path/to/files4".to_string(),
                age: 60,
                filename_regexp: ".*".to_string(),
            },
        ];

        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let configs = parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs, expected);
    }

    #[test]
    fn test_parse_config_with_comments() {
        let config_string = r#"# This is a comment
# Another comment
{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":86400,"filename_regexp":".*"}
# Comment after a line
{"host_from":"192.168.0.3","port_from":21,"login_from":"user3","password_from":"password3","path_from":"/path3/","host_to":"192.168.0.4","port_to":21,"login_to":"user4","password_to":"password4","path_to":"/path4","age":3600,"filename_regexp":".*\\.txt$"}
"#;
        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let configs = parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].ip_address_from, "192.168.0.1");
        assert_eq!(configs[1].ip_address_from, "192.168.0.3");
    }

    #[test]
    fn test_parse_config_invalid_regex() {
        let config_string = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":86400,"filename_regexp":"(invalid["}"#;
        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let result = parse_config(config_path.to_str().unwrap());
        assert!(result.is_err());
    }
}
