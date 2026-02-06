use regex::Regex;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::fmt;
use secrecy::{Secret, ExposeSecret};

/// FTP/FTPS/SFTP protocol type
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Standard FTP (unencrypted)
    #[default]
    Ftp,
    /// FTP over TLS/SSL (encrypted)
    Ftps,
    /// SSH File Transfer Protocol
    Sftp,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Ftp => write!(f, "ftp"),
            Protocol::Ftps => write!(f, "ftps"),
            Protocol::Sftp => write!(f, "sftp"),
        }
    }
}

/// FTP transfer configuration parameters
#[derive(Debug, Deserialize)]
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
    #[serde(rename = "password_from", default)]
    pub password_from: Option<Secret<String>>,
    /// Path to private SSH key for SFTP source auth (JSON field: keyfile_from)
    #[serde(rename = "keyfile_from", default)]
    pub keyfile_from: Option<String>,
    /// Passphrase for SSH private key for SFTP source auth (JSON field: keyfile_pass_from)
    #[serde(rename = "keyfile_pass_from", default)]
    pub keyfile_pass_from: Option<Secret<String>>,
    /// Source directory path (must be literal path, no wildcards) (JSON field: path_from)
    #[serde(rename = "path_from")]
    pub path_from: String,
    /// Source protocol (ftp, ftps, or sftp, default: ftp) (JSON field: proto_from)
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
    #[serde(rename = "password_to", default)]
    pub password_to: Option<Secret<String>>,
    /// Path to private SSH key for SFTP destination auth (JSON field: keyfile_to)
    #[serde(rename = "keyfile_to", default)]
    pub keyfile_to: Option<String>,
    /// Passphrase for SSH private key for SFTP destination auth (JSON field: keyfile_pass_to)
    #[serde(rename = "keyfile_pass_to", default)]
    pub keyfile_pass_to: Option<Secret<String>>,
    /// Destination directory path (JSON field: path_to)
    #[serde(rename = "path_to")]
    pub path_to: String,
    /// Destination protocol (ftp, ftps, or sftp, default: ftp) (JSON field: proto_to)
    #[serde(rename = "proto_to", default)]
    pub proto_to: Protocol,
    /// Minimum file age to transfer (seconds) (JSON field: age)
    #[serde(rename = "age")]
    pub age: u64,
    /// Regular expression pattern for filename matching (JSON field: filename_regexp)
    #[serde(rename = "filename_regexp")]
    pub filename_regexp: String,
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
    /// - For FTP/FTPS: password is required
    /// - For SFTP: password OR keyfile is required (but not both)
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

        // Validate authentication (password or keyfile, but not both)
        use std::path::Path;

        // Validate from authentication
        let has_password_from = self.password_from.as_ref().map_or(false, |p| !p.expose_secret().is_empty());
        let has_keyfile_from = self.keyfile_from.as_ref().map_or(false, |k| !k.is_empty());
        let has_keyfile_pass_from = self.keyfile_pass_from.as_ref().map_or(false, |p| !p.expose_secret().is_empty());

        if self.proto_from == Protocol::Sftp {
            if !has_password_from && !has_keyfile_from {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "from_auth: password_from or keyfile_from is required for SFTP"
                ));
            }
            if has_password_from && has_keyfile_from {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "from_auth: password_from and keyfile_from are mutually exclusive"
                ));
            }
            if has_keyfile_pass_from && !has_keyfile_from {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "from_auth: keyfile_pass_from requires keyfile_from"
                ));
            }
            if has_keyfile_from {
                let keyfile = self.keyfile_from.as_ref().unwrap();
                if !Path::new(keyfile).exists() {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("from_auth: keyfile_from '{}' does not exist", keyfile)
                    ));
                }
            }
        } else {
            // For FTP/FTPS, password is required
            if !has_password_from {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "password_from is required for FTP/FTPS"
                ));
            }
        }

        // Validate to authentication
        let has_password_to = self.password_to.as_ref().map_or(false, |p| !p.expose_secret().is_empty());
        let has_keyfile_to = self.keyfile_to.as_ref().map_or(false, |k| !k.is_empty());
        let has_keyfile_pass_to = self.keyfile_pass_to.as_ref().map_or(false, |p| !p.expose_secret().is_empty());

        if self.proto_to == Protocol::Sftp {
            if !has_password_to && !has_keyfile_to {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "to_auth: password_to or keyfile_to is required for SFTP"
                ));
            }
            if has_password_to && has_keyfile_to {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "to_auth: password_to and keyfile_to are mutually exclusive"
                ));
            }
            if has_keyfile_pass_to && !has_keyfile_to {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "to_auth: keyfile_pass_to requires keyfile_to"
                ));
            }
            if has_keyfile_to {
                let keyfile = self.keyfile_to.as_ref().unwrap();
                if !Path::new(keyfile).exists() {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("to_auth: keyfile_to '{}' does not exist", keyfile)
                    ));
                }
            }
        } else {
            // For FTP/FTPS, password is required
            if !has_password_to {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "password_to is required for FTP/FTPS"
                ));
            }
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

        // Validate age - age 0 is valid and disables age filtering
        // (all files will be transferred regardless of modification time)
        // Note: age is u64, so negative values are impossible at type level

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
    use secrecy::Secret;

    #[test]
    fn test_parse_config() {
        let config_string = r#"{"host_from":"192.168.0.1","port_from":22,"login_from":"user1","password_from":"password1","path_from":"/path/to/files/","host_to":"192.168.0.2","port_to":22,"login_to":"user2","password_to":"password2","path_to":"/path/to/files2","age":30,"filename_regexp":".*"}
{"host_from":"192.168.0.3","port_from":22,"login_from":"user3","password_from":"password3","path_from":"/path/to/files3/","host_to":"192.168.0.4","port_to":22,"login_to":"user4","password_to":"password4","path_to":"/path/to/files4","age":60,"filename_regexp":".*"}"#;

        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let configs = parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs.len(), 2);

        // Check first config fields (excluding passwords which are Secret)
        assert_eq!(configs[0].ip_address_from, "192.168.0.1");
        assert_eq!(configs[0].port_from, 22);
        assert_eq!(configs[0].login_from, "user1");
        assert_eq!(configs[0].keyfile_from, None);
        assert_eq!(configs[0].path_from, "/path/to/files/");
        assert_eq!(configs[0].proto_from, Protocol::Ftp);
        assert_eq!(configs[0].ip_address_to, "192.168.0.2");
        assert_eq!(configs[0].port_to, 22);
        assert_eq!(configs[0].login_to, "user2");
        assert_eq!(configs[0].keyfile_to, None);
        assert_eq!(configs[0].path_to, "/path/to/files2");
        assert_eq!(configs[0].proto_to, Protocol::Ftp);
        assert_eq!(configs[0].age, 30);
        assert_eq!(configs[0].filename_regexp, ".*");
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 0,
            filename_regexp: ".*".to_string(),
        };
        // age 0 is valid and disables age filtering
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_regex() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168\\1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
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
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2 invalid".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_no_auth() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: None,
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_both_auth_methods() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: Some("/path/to/key".to_string()),
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_nonexistent_keyfile() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: None,
            keyfile_from: Some("/nonexistent/keyfile".to_string()),
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_password_auth_valid() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: Some(Secret::new("pass".to_string())),
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_ftp_requires_password() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 21,
            login_from: "user".to_string(),
            password_from: None,
            keyfile_from: None,
            keyfile_pass_from: None,
            path_from: "/path/".to_string(),
            proto_from: Protocol::Ftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 21,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Ftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_keyfile_passphrase_requires_keyfile() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: None,
            keyfile_from: None,
            keyfile_pass_from: Some(Secret::new("passphrase".to_string())),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 22,
            login_to: "user2".to_string(),
            password_to: Some(Secret::new("pass2".to_string())),
            keyfile_to: None,
            keyfile_pass_to: None,
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Sftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        // Passphrase without keyfile should fail validation
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_keyfile_with_passphrase_valid() {
        let config = Config {
            ip_address_from: "192.168.1.1".to_string(),
            port_from: 22,
            login_from: "user".to_string(),
            password_from: None,
            // Note: using a nonexistent keyfile path - validation will check existence
            // For this test, we only check that the structure allows passphrase
            keyfile_from: Some("/nonexistent/keyfile".to_string()),
            keyfile_pass_from: Some(Secret::new("passphrase".to_string())),
            path_from: "/path/".to_string(),
            proto_from: Protocol::Sftp,
            ip_address_to: "192.168.1.2".to_string(),
            port_to: 22,
            login_to: "user2".to_string(),
            password_to: None,
            keyfile_to: Some("/nonexistent/keyfile2".to_string()),
            keyfile_pass_to: Some(Secret::new("passphrase2".to_string())),
            path_to: "/path2/".to_string(),
            proto_to: Protocol::Sftp,
            age: 100,
            filename_regexp: ".*".to_string(),
        };
        // Passphrase with keyfile should pass structural validation
        // (will fail on nonexistent file, but that's a different error)
        let result = config.validate();
        // Should fail due to nonexistent keyfile, NOT due to passphrase validation
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("does not exist"));
        assert!(!err_msg.contains("passphrase"));
    }
}
