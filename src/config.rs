use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind}; // Removed 'self'
use std::str::FromStr;

/// FTP transfer configuration parameters
#[derive(Debug, PartialEq)]
pub struct Config {
    /// Source FTP server IP/hostname
    pub ip_address_from: String,
    /// Source FTP server port (typically 21)
    pub port_from: u16,
    /// Username for source FTP server
    pub login_from: String,
    /// Password for source FTP server
    pub password_from: String,
    /// Source directory path (must be literal path, no wildcards)
    pub path_from: String,
    /// Destination FTP server IP/hostname
    pub ip_address_to: String,
    /// Destination FTP server port (typically 21)
    pub port_to: u16,
    /// Username for destination FTP server
    pub login_to: String,
    /// Password for destination FTP server
    pub password_to: String,
    /// Destination directory path
    pub path_to: String,
    /// Minimum file age to transfer (seconds)
    pub age: u64,
    /// Regular expression pattern for filename matching
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
/// - Invalid field format (non-numeric where expected)
/// - Missing required fields
///
/// # File Format
/// CSV format with fields:
/// ip_from,port_from,login_from,password_from,path_from,
/// ip_to,port_to,login_to,password_to,path_to,min_age_secs
///
/// # Example
/// ```
/// // let configs = parse_config("settings.csv")?;
/// ```
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
        let host_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: host_from",
            ))?
            .to_string();
        let port_from = u16::from_str(fields.next().ok_or(Error::new(
            ErrorKind::InvalidInput,
            "missing field: port_from",
        ))?)
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let user_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: user_from",
            ))?
            .to_string();
        let pass_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: pass_from",
            ))?
            .to_string();
        let path_from = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: path_from",
            ))?
            .to_string();
        let host_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: host_to",
            ))?
            .to_string();
        let port_to = u16::from_str(fields.next().ok_or(Error::new(
            ErrorKind::InvalidInput,
            "missing field: port_to",
        ))?)
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let user_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: user_to",
            ))?
            .to_string();
        let pass_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: pass_to",
            ))?
            .to_string();
        let path_to = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: path_to",
            ))?
            .to_string();
        let age = u64::from_str(
            fields
                .next()
                .ok_or(Error::new(ErrorKind::InvalidInput, "missing field: age"))?,
        )
        .map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;

        let filename_regexp = fields
            .next()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                "missing field: filename_regexp",
            ))?
            .to_string();

        // Validate the regex pattern
        Regex::new(&filename_regexp).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid filename regex pattern: {}", e),
            )
        })?;

        configs.push(Config {
            ip_address_from: host_from,
            port_from,
            login_from: user_from,
            password_from: pass_from,
            path_from,
            ip_address_to: host_to,
            port_to,
            login_to: user_to,
            password_to: pass_to,
            path_to,
            age,
            filename_regexp,
        });
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*; // Imports Config and parse_config from the outer module
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_parse_config() {
        let config_string = "192.168.0.1,22,user1,password1,/path/to/files/*,192.168.0.2,22,user2,password2,/path/to/files2,30,.*\n192.168.0.3,22,user3,password3,/path/to/files3/*,192.168.0.4,22,user4,password4,/path/to/files4,60,.*";
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
                filename_regexp: ".*".to_string(),
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
                filename_regexp: ".*".to_string(),
            },
        ];

        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.csv");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let configs = parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs, expected);
    }
}
