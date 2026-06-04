use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use std::fs::File;
use std::io::{BufRead, BufReader, Write, Error, ErrorKind};
use std::fmt;
use std::net::IpAddr;
use secrecy::{Secret, ExposeSecret};

/// Deserialize an optional IpAddr from a JSON string or null
/// Missing/null → None, string → parsed IpAddr
fn deserialize_optional_ip<'de, D>(deserializer: D) -> Result<Option<IpAddr>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) => s.parse::<IpAddr>()
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// FTP/FTPS/SFTP protocol type
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

/// Timezone offset for interpreting MDTM timestamps from FTP servers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TzOffset {
    /// UTC (no conversion needed)
    Utc,
    /// Fixed offset in seconds from UTC (e.g., +10800 = +03:00)
    Fixed(i32),
}

impl Default for TzOffset {
    fn default() -> Self {
        TzOffset::Utc
    }
}

impl fmt::Display for TzOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TzOffset::Utc => write!(f, "utc"),
            TzOffset::Fixed(secs) => {
                let sign = if *secs >= 0 { '+' } else { '-' };
                let total = secs.abs();
                let hours = total / 3600;
                let minutes = (total % 3600) / 60;
                write!(f, "{}{:02}:{:02}", sign, hours, minutes)
            }
        }
    }
}

/// Parse a timezone offset string
///
/// Supported formats:
/// - "utc" (case-insensitive) → Utc
/// - "+HH:MM", "-HH:MM" → Fixed (e.g., "+03:00" → Fixed(10800))
/// - "+HHMM", "-HHMM" → Fixed (e.g., "+0300" → Fixed(10800))
/// - "+H", "+HH", "-H", "-HH" → Fixed (e.g., "+3" → Fixed(10800))
fn parse_tz_offset(s: &str) -> Result<TzOffset, String> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("utc") {
        return Ok(TzOffset::Utc);
    }
    if s.is_empty() {
        return Err("Invalid timezone offset '': must start with '+' or '-' or be 'utc'".to_string());
    }

    let (sign, rest) = match s.as_bytes()[0] {
        b'+' => (1i32, &s[1..]),
        b'-' => (-1i32, &s[1..]),
        _ => return Err(format!(
            "Invalid timezone offset '{}': must start with '+' or '-' or be 'utc'", s
        )),
    };

    if rest.is_empty() {
        return Err(format!(
            "Invalid timezone offset '{}': expected digits after sign", s
        ));
    }

    // Parse hours and optionally minutes
    let (hours_str, minutes_str) = if rest.contains(':') {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid timezone offset '{}': expected format +HH:MM", s));
        }
        (parts[0], Some(parts[1]))
    } else if rest.len() > 2 {
        // +HHMM format
        (&rest[..rest.len()-2], Some(&rest[rest.len()-2..]))
    } else {
        // +H or +HH format (no minutes)
        (rest, None)
    };

    let hours: i32 = hours_str.parse().map_err(|_| format!(
        "Invalid timezone offset '{}': invalid hours '{}'", s, hours_str
    ))?;

    let minutes: i32 = if let Some(ms) = minutes_str {
        ms.parse().map_err(|_| format!(
            "Invalid timezone offset '{}': invalid minutes '{}'", s, ms
        ))?
    } else {
        0
    };

    if hours > 23 || minutes > 59 {
        return Err(format!(
            "Invalid timezone offset '{}': hours must be 0-23 and minutes must be 0-59", s
        ));
    }

    Ok(TzOffset::Fixed(sign * (hours * 3600 + minutes * 60)))
}

impl TryFrom<String> for TzOffset {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        parse_tz_offset(&s)
    }
}

impl Serialize for TzOffset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for TzOffset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.try_into().map_err(serde::de::Error::custom)
    }
}

/// FTP transfer configuration parameters
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// Timezone offset for source server MDTM timestamps (JSON field: tz_from, default: utc)
    #[serde(rename = "tz_from", default)]
    pub tz_from: TzOffset,
    /// Timezone offset for target server timestamps (JSON field: tz_to, default: utc)
    #[serde(rename = "tz_to", default)]
    pub tz_to: TzOffset,
    /// Local IP address to bind for source server connections (JSON field: bind_from, default: none)
    #[serde(rename = "bind_from", default, deserialize_with = "deserialize_optional_ip")]
    pub bind_from: Option<IpAddr>,
    /// Local IP address to bind for target server connections (JSON field: bind_to, default: none)
    #[serde(rename = "bind_to", default, deserialize_with = "deserialize_optional_ip")]
    pub bind_to: Option<IpAddr>,
}

impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Use serde_json to build a JSON value, then serialize it
        // This avoids the complexity of manual serialize_struct with custom serializers
        let mut map = serde_json::Map::new();
        map.insert("host_from".into(), serde_json::Value::String(self.ip_address_from.clone()));
        map.insert("port_from".into(), serde_json::Value::Number(self.port_from.into()));
        map.insert("login_from".into(), serde_json::Value::String(self.login_from.clone()));
        if let Some(ref p) = self.password_from {
            map.insert("password_from".into(), serde_json::Value::String(p.expose_secret().clone()));
        }
        if let Some(ref k) = self.keyfile_from {
            map.insert("keyfile_from".into(), serde_json::Value::String(k.clone()));
        }
        if let Some(ref p) = self.keyfile_pass_from {
            map.insert("keyfile_pass_from".into(), serde_json::Value::String(p.expose_secret().clone()));
        }
        map.insert("path_from".into(), serde_json::Value::String(self.path_from.clone()));
        map.insert("proto_from".into(), serde_json::to_value(&self.proto_from).map_err(serde::ser::Error::custom)?);
        map.insert("host_to".into(), serde_json::Value::String(self.ip_address_to.clone()));
        map.insert("port_to".into(), serde_json::Value::Number(self.port_to.into()));
        map.insert("login_to".into(), serde_json::Value::String(self.login_to.clone()));
        if let Some(ref p) = self.password_to {
            map.insert("password_to".into(), serde_json::Value::String(p.expose_secret().clone()));
        }
        if let Some(ref k) = self.keyfile_to {
            map.insert("keyfile_to".into(), serde_json::Value::String(k.clone()));
        }
        if let Some(ref p) = self.keyfile_pass_to {
            map.insert("keyfile_pass_to".into(), serde_json::Value::String(p.expose_secret().clone()));
        }
        map.insert("path_to".into(), serde_json::Value::String(self.path_to.clone()));
        map.insert("proto_to".into(), serde_json::to_value(&self.proto_to).map_err(serde::ser::Error::custom)?);
        map.insert("age".into(), serde_json::Value::Number(self.age.into()));
        map.insert("filename_regexp".into(), serde_json::Value::String(self.filename_regexp.clone()));
        map.insert("tz_from".into(), serde_json::to_value(&self.tz_from).map_err(serde::ser::Error::custom)?);
        map.insert("tz_to".into(), serde_json::to_value(&self.tz_to).map_err(serde::ser::Error::custom)?);
        if let Some(ref ip) = self.bind_from {
            map.insert("bind_from".into(), serde_json::Value::String(ip.to_string()));
        }
        if let Some(ref ip) = self.bind_to {
            map.insert("bind_to".into(), serde_json::Value::String(ip.to_string()));
        }
        serde_json::Value::Object(map).serialize(serializer)
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

    /// Validate config for web editing — skips filesystem checks (keyfile existence)
    pub fn validate_for_edit(&self) -> Result<(), Error> {
        if self.ip_address_from.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "host_from cannot be empty"));
        }
        if self.ip_address_to.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "host_to cannot be empty"));
        }
        for (host, field_name) in [
            (&self.ip_address_from, "host_from"),
            (&self.ip_address_to, "host_to"),
        ] {
            if host.contains('/') || host.contains('\\') || host.contains(' ') {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("{} contains invalid characters", field_name),
                ));
            }
        }
        if self.port_from == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "port_from cannot be 0"));
        }
        if self.port_to == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "port_to cannot be 0"));
        }
        if self.login_from.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "login_from cannot be empty"));
        }
        if self.login_to.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "login_to cannot be empty"));
        }

        let has_password_from = self.password_from.as_ref().map_or(false, |p| !p.expose_secret().is_empty());
        let has_keyfile_from = self.keyfile_from.as_ref().map_or(false, |k| !k.is_empty());
        let has_keyfile_pass_from = self.keyfile_pass_from.as_ref().map_or(false, |p| !p.expose_secret().is_empty());

        if self.proto_from == Protocol::Sftp {
            if !has_password_from && !has_keyfile_from {
                return Err(Error::new(ErrorKind::InvalidInput, "from_auth: password_from or keyfile_from is required for SFTP"));
            }
            if has_password_from && has_keyfile_from {
                return Err(Error::new(ErrorKind::InvalidInput, "from_auth: password_from and keyfile_from are mutually exclusive"));
            }
            if has_keyfile_pass_from && !has_keyfile_from {
                return Err(Error::new(ErrorKind::InvalidInput, "from_auth: keyfile_pass_from requires keyfile_from"));
            }
            // Note: no filesystem check for keyfile existence in validate_for_edit
        } else {
            if !has_password_from {
                return Err(Error::new(ErrorKind::InvalidInput, "password_from is required for FTP/FTPS"));
            }
        }

        let has_password_to = self.password_to.as_ref().map_or(false, |p| !p.expose_secret().is_empty());
        let has_keyfile_to = self.keyfile_to.as_ref().map_or(false, |k| !k.is_empty());
        let has_keyfile_pass_to = self.keyfile_pass_to.as_ref().map_or(false, |p| !p.expose_secret().is_empty());

        if self.proto_to == Protocol::Sftp {
            if !has_password_to && !has_keyfile_to {
                return Err(Error::new(ErrorKind::InvalidInput, "to_auth: password_to or keyfile_to is required for SFTP"));
            }
            if has_password_to && has_keyfile_to {
                return Err(Error::new(ErrorKind::InvalidInput, "to_auth: password_to and keyfile_to are mutually exclusive"));
            }
            if has_keyfile_pass_to && !has_keyfile_to {
                return Err(Error::new(ErrorKind::InvalidInput, "to_auth: keyfile_pass_to requires keyfile_to"));
            }
        } else {
            if !has_password_to {
                return Err(Error::new(ErrorKind::InvalidInput, "password_to is required for FTP/FTPS"));
            }
        }

        if self.path_from.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "path_from cannot be empty"));
        }
        if self.path_to.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "path_to cannot be empty"));
        }
        if let Err(e) = Regex::new(&self.filename_regexp) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid filename_regexp pattern '{}': {}", self.filename_regexp, e),
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

/// A config entry with its associated comment block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// Comment text (without # prefixes), lines separated by \n
    pub comment: String,
    /// The config itself
    pub config: Config,
}

/// Parses configuration file into a vector of ConfigEntry structs (with comments preserved)
///
/// Comment lines and empty lines before a JSONL record are captured as the entry's comment.
/// Trailing comments after the last record are attached to that record.
pub fn parse_config_entries(filename: &str) -> Result<Vec<ConfigEntry>, Error> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let mut entries = Vec::new();
    let mut comment_lines: Vec<String> = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Empty line between blocks — skip (don't add to comment)
            continue;
        }

        if trimmed.starts_with('#') {
            // Comment line — strip "# " or "#" prefix
            let text = if trimmed.len() > 1 && trimmed.chars().nth(1) == Some(' ') {
                &trimmed[2..]
            } else {
                &trimmed[1..]
            };
            comment_lines.push(text.to_string());
            continue;
        }

        // JSON line
        let config: Config = serde_json::from_str(trimmed).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid JSON on line {}: {}", line_num + 1, e),
            )
        })?;

        // Validate regex
        Regex::new(&config.filename_regexp).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid filename regex pattern on line {}: {}", line_num + 1, e),
            )
        })?;

        // Validate config values
        config.validate().map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("invalid config values on line {}: {}", line_num + 1, e),
            )
        })?;

        let comment = comment_lines.join("\n");
        comment_lines.clear();

        entries.push(ConfigEntry { comment, config });
    }

    // Trailing comments attach to last entry
    if !comment_lines.is_empty() {
        if let Some(last) = entries.last_mut() {
            if !last.comment.is_empty() {
                last.comment.push('\n');
            }
            last.comment.push_str(&comment_lines.join("\n"));
        }
    }

    Ok(entries)
}

/// Writes configuration entries to a JSONL file atomically (temp file → rename)
///
/// Format: comment lines with "# " prefix, then JSONL line, then blank line separator
pub fn write_config_entries(filename: &str, entries: &[ConfigEntry]) -> Result<(), Error> {
    let temp_filename = format!("{}.tmp", filename);
    let mut file = File::create(&temp_filename)?;

    for (i, entry) in entries.iter().enumerate() {
        // Add blank line separator between entries (not before first)
        if i > 0 {
            writeln!(file)?;
        }

        // Write comment lines
        if !entry.comment.is_empty() {
            for line in entry.comment.split('\n') {
                writeln!(file, "# {}", line)?;
            }
        }

        // Write JSONL line
        let json = serde_json::to_string(&entry.config)
            .map_err(|e| Error::new(ErrorKind::InvalidData, format!("serialize error: {}", e)))?;
        writeln!(file, "{}", json)?;
    }

    file.flush()?;
    std::fs::rename(&temp_filename, filename)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use secrecy::Secret;

    /// Helper to build a Config with sensible defaults for validation tests.
    /// Only specify fields that differ from defaults.
    fn make_config<F>(
        mut overrides: F,
    ) -> Config
    where
        F: FnMut(&mut Config),
    {
        let mut config = Config {
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
            tz_from: TzOffset::Utc,
            tz_to: TzOffset::Utc,
            bind_from: None,
            bind_to: None,
        };
        overrides(&mut config);
        config
    }

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
        let config = make_config(|c| c.ip_address_from = "".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_zero_port_from() {
        let config = make_config(|c| c.port_from = 0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_login() {
        let config = make_config(|c| c.login_from = "".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_password() {
        let config = make_config(|c| c.password_from = Some(Secret::new("".to_string())));
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_path() {
        let config = make_config(|c| c.path_from = "".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_zero_age() {
        let config = make_config(|c| c.age = 0);
        // age 0 is valid and disables age filtering
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_regex() {
        let config = make_config(|c| c.filename_regexp = "(invalid[".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_valid() {
        let config = make_config(|_| {});
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_host_characters() {
        let config = make_config(|c| c.ip_address_from = "192.168.1.1/invalid".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_host_characters_backslash() {
        let config = make_config(|c| c.ip_address_to = "192.168\\1.2".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_host_characters_space() {
        let config = make_config(|c| c.ip_address_to = "192.168.1.2 invalid".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_no_auth() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.password_from = None;
            c.proto_from = Protocol::Sftp;
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_both_auth_methods() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.password_from = Some(Secret::new("pass".to_string()));
            c.keyfile_from = Some("/path/to/key".to_string());
            c.proto_from = Protocol::Sftp;
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_nonexistent_keyfile() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.password_from = None;
            c.keyfile_from = Some("/nonexistent/keyfile".to_string());
            c.proto_from = Protocol::Sftp;
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_password_auth_valid() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.proto_from = Protocol::Sftp;
        });
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_ftp_requires_password() {
        let config = make_config(|c| c.password_from = None);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_keyfile_passphrase_requires_keyfile() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.password_from = None;
            c.keyfile_pass_from = Some(Secret::new("passphrase".to_string()));
            c.proto_from = Protocol::Sftp;
            c.port_to = 22;
            c.password_to = Some(Secret::new("pass2".to_string()));
            c.proto_to = Protocol::Sftp;
        });
        // Passphrase without keyfile should fail validation
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_sftp_keyfile_with_passphrase_valid() {
        let config = make_config(|c| {
            c.port_from = 22;
            c.password_from = None;
            c.keyfile_from = Some("/nonexistent/keyfile".to_string());
            c.keyfile_pass_from = Some(Secret::new("passphrase".to_string()));
            c.proto_from = Protocol::Sftp;
            c.port_to = 22;
            c.password_to = None;
            c.keyfile_to = Some("/nonexistent/keyfile2".to_string());
            c.keyfile_pass_to = Some(Secret::new("passphrase2".to_string()));
            c.proto_to = Protocol::Sftp;
        });
        // Passphrase with keyfile should pass structural validation
        // (will fail on nonexistent file, but that's a different error)
        let result = config.validate();
        // Should fail due to nonexistent keyfile, NOT due to passphrase validation
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("does not exist"));
        assert!(!err_msg.contains("passphrase"));
    }

    // ===== TzOffset tests =====

    #[test]
    fn test_parse_tz_offset_utc() {
        assert_eq!(parse_tz_offset("utc").unwrap(), TzOffset::Utc);
        assert_eq!(parse_tz_offset("UTC").unwrap(), TzOffset::Utc);
        assert_eq!(parse_tz_offset(" utc ").unwrap(), TzOffset::Utc);
    }

    #[test]
    fn test_parse_tz_offset_positive_with_colon() {
        assert_eq!(parse_tz_offset("+03:00").unwrap(), TzOffset::Fixed(10800));
    }

    #[test]
    fn test_parse_tz_offset_negative_with_colon() {
        assert_eq!(parse_tz_offset("-05:30").unwrap(), TzOffset::Fixed(-19800));
    }

    #[test]
    fn test_parse_tz_offset_compact_format() {
        assert_eq!(parse_tz_offset("+0300").unwrap(), TzOffset::Fixed(10800));
        assert_eq!(parse_tz_offset("-0530").unwrap(), TzOffset::Fixed(-19800));
    }

    #[test]
    fn test_parse_tz_offset_hours_only() {
        assert_eq!(parse_tz_offset("+3").unwrap(), TzOffset::Fixed(10800));
        assert_eq!(parse_tz_offset("+03").unwrap(), TzOffset::Fixed(10800));
        assert_eq!(parse_tz_offset("-5").unwrap(), TzOffset::Fixed(-18000));
    }

    #[test]
    fn test_parse_tz_offset_errors() {
        // Empty string
        assert!(parse_tz_offset("").is_err());
        // Invalid string
        assert!(parse_tz_offset("invalid").is_err());
        // No sign
        assert!(parse_tz_offset("03:00").is_err());
        // Out of range hours
        assert!(parse_tz_offset("+25:00").is_err());
        // Out of range minutes
        assert!(parse_tz_offset("+12:60").is_err());
        // Garbage values
        assert!(parse_tz_offset("garbage").is_err());
        assert!(parse_tz_offset("lol").is_err());
        assert!(parse_tz_offset("123").is_err());
    }

    #[test]
    fn test_tz_offset_serde_with_value() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","tz_from":"+03:00"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.tz_from, TzOffset::Fixed(10800));
        assert_eq!(config.tz_to, TzOffset::Utc); // default
    }

    #[test]
    fn test_tz_offset_serde_default() {
        // JSON without tz_from/tz_to fields → defaults to Utc
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.tz_from, TzOffset::Utc);
        assert_eq!(config.tz_to, TzOffset::Utc);
    }

    #[test]
    fn test_tz_offset_serde_invalid() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","tz_from":"garbage"}"#;
        let result = serde_json::from_str::<Config>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_tz_offset_serde_invalid_via_parse_config() {
        // Garbage in tz_from should produce error with line number
        let config_string = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","tz_from":"garbage"}"#;
        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let result = parse_config(config_path.to_str().unwrap());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid JSON on line 1"), "Expected line number in error: {}", err_msg);
        assert!(err_msg.contains("garbage"), "Expected original value in error: {}", err_msg);
    }

    #[test]
    fn test_tz_offset_serde_invalid_tz_to_via_parse_config() {
        // Garbage in tz_to should also produce error with line number
        let config_string = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","tz_to":"lol"}"#;
        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");

        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let result = parse_config(config_path.to_str().unwrap());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid JSON on line 1"), "Expected line number in error: {}", err_msg);
        assert!(err_msg.contains("lol"), "Expected original value in error: {}", err_msg);
    }

    #[test]
    fn test_tz_offset_display() {
        assert_eq!(format!("{}", TzOffset::Utc), "utc");
        assert_eq!(format!("{}", TzOffset::Fixed(10800)), "+03:00");
        assert_eq!(format!("{}", TzOffset::Fixed(-18000)), "-05:00");
        assert_eq!(format!("{}", TzOffset::Fixed(19800)), "+05:30");
    }

    // ===== bind_from/bind_to deserialization tests =====

    #[test]
    fn test_bind_from_ipv4() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_from":"1.2.3.4"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.bind_from, Some("1.2.3.4".parse().unwrap()));
        assert_eq!(config.bind_to, None); // default
    }

    #[test]
    fn test_bind_to_ipv4() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_to":"10.0.0.1"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.bind_from, None); // default
        assert_eq!(config.bind_to, Some("10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_bind_ipv6() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_from":"::1","bind_to":"fe80::1"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.bind_from, Some("::1".parse().unwrap()));
        assert_eq!(config.bind_to, Some("fe80::1".parse().unwrap()));
    }

    #[test]
    fn test_bind_null_explicit() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_from":null,"bind_to":null}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.bind_from, None);
        assert_eq!(config.bind_to, None);
    }

    #[test]
    fn test_bind_missing_defaults_to_none() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.bind_from, None);
        assert_eq!(config.bind_to, None);
    }

    #[test]
    fn test_bind_invalid_ip_rejected() {
        let json = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_from":"not-an-ip"}"#;
        let result = serde_json::from_str::<Config>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_bind_invalid_ip_via_parse_config() {
        let config_string = r#"{"host_from":"192.168.0.1","port_from":21,"login_from":"user1","password_from":"password1","path_from":"/path/","host_to":"192.168.0.2","port_to":21,"login_to":"user2","password_to":"password2","path_to":"/path2","age":100,"filename_regexp":".*","bind_to":"999.999.999.999"}"#;
        let dir = tempdir().unwrap();
        let mut config_path = PathBuf::from(dir.path());
        config_path.push("config.jsonl");
        let mut file = File::create(&config_path).unwrap();
        file.write_all(config_string.as_bytes()).unwrap();

        let result = parse_config(config_path.to_str().unwrap());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid JSON on line 1"), "Expected line number in error: {}", err_msg);
    }
}
