use regex::Regex;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::fmt;
use zeroize::Zeroize;

/// FTP/FTPS protocol type
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Standard FTP (unencrypted)
    #[default]
    Ftp,
    /// FTP over TLS/SSL (encrypted)
    Ftps,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Ftp => write!(f, "ftp"),
            Protocol::Ftps => write!(f, "ftps"),
        }
    }
}

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
    /// Source protocol (ftp or ftps, default: ftp) (JSON field: proto_from)
    #[serde(rename = "proto_from", default)]
    pub proto_from: Protocol,
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
    /// Destination protocol (ftp or ftps, default: ftp) (JSON field: proto_to)
    #[serde(rename = "proto_to", default)]
    pub proto_to: Protocol,
    /// Minimum file age to transfer (seconds) (JSON field: age)
    #[serde(rename = "age")]
    pub age: u64,
    /// Regular expression pattern for filename matching (JSON field: filename_regexp)
    #[serde(rename = "filename_regexp")]
    pub filename_regexp: String,
}

impl Drop for Config {
    fn drop(&mut self) {
        // Zeroize passwords when Config is dropped to protect sensitive data in memory
        self.password_from.zeroize();
        self.password_to.zeroize();
    }
}

impl Config {
    /// Validates configuration field values
    ///
    /// # Returns
    /// * `Ok(())` if all fields are valid
    /// * `Err(Error)` if any field is invalid
    ///
    /// # Validation Rules
    /// - Host addresses must be non-empty
    /// - Host addresses must not contain invalid characters (spaces, slashes)
    /// - Ports must be non-zero
    /// - Logins must be non-empty
    /// - Passwords must be non-empty
    /// - Paths must be non-empty
    /// - Age must be reasonable (> 0)
    /// - Regex pattern must be valid
    pub fn validate(&self) -> Result<(), Error> {
        // Validate host addresses
        if self.ip_address_from.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "host_from cannot be empty"
            ));
        }
        if self.ip_address_to.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "host_to cannot be empty"
            ));
        }

        // Validate host addresses for invalid characters
        for (host, field_name) in [
            (&self.ip_address_from, "host_from"),
            (&self.ip_address_to, "host_to"),
        ] {
            // Check for invalid characters that shouldn't appear in hostnames/IPs
            if host.contains('/') || host.contains('\\') || host.contains(' ') {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("{} contains invalid characters", field_name)
                ));
            }
        }

        // Validate ports
        if self.port_from == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "port_from cannot be 0"
            ));
        }
        if self.port_to == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "port_to cannot be 0"
            ));
        }

        // Validate logins
        if self.login_from.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "login_from cannot be empty"
            ));
        }
        if self.login_to.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "login_to cannot be empty"
            ));
        }

        // Validate passwords
        if self.password_from.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "password_from cannot be empty"
            ));
        }
        if self.password_to.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "password_to cannot be empty"
            ));
        }

        // Validate paths
        if self.path_from.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "path_from cannot be empty"
            ));
        }
        if self.path_to.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "path_to cannot be empty"
            ));
        }

        // Validate age
        if self.age == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "age cannot be 0"
            ));
        }

        // Validate regex pattern (already done in parse_config but double-check here)
        if let Err(e) = Regex::new(&self.filename_regexp) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid filename_regexp pattern '{}': {}", self.filename_regexp, e)
            ));
        }

        Ok(())
    }
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

        // Validate all field values
        config.validate().map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid config values on line {}: {}", line_num + 1, e),
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
                proto_from: Protocol::Ftp,
                ip_address_to: "192.168.0.2".to_string(),
                port_to: 22,
                login_to: "user2".to_string(),
                password_to: "password2".to_string(),
                path_to: "/path/to/files2".to_string(),
                proto_to: Protocol::Ftp,
                age: 30,
                filename_regexp: ".*".to_string(),
            },
            Config {
                ip_address_from: "192.168.0.3".to_string(),
                port_from: 22,
                login_from: "user3".to_string(),
                password_from: "password3".to_string(),
                path_from: "/path/to/files3/".to_string(),
                proto_from: Protocol::Ftp,
                ip_address_to: "192.168.0.4".to_string(),
                port_to: 22,
                login_to: "user4".to_string(),
                password_to: "password4".to_string(),
                path_to: "/path/to/files4".to_string(),
                proto_to: Protocol::Ftp,
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

    #[test]
    fn test_config_validate_empty_host_from() {
        let config = Config {
            ip_address_from: "".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_zero_port_from() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 0,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_login() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_password() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_path() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_zero_age() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 0,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_regex() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: "(invalid[".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_valid() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_host_characters() {
        let config = Config {
            ip_address_from: "192.168.1.1/invalid".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_host_characters_backslash() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168\\1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_host_characters_space() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: "pass".to_string(),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2 invalid".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: "pass2".to_string(),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }
}
