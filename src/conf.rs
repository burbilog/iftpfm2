use std::fs::File;
use std::io::{BufReader, BufRead, Error, ErrorKind};
use std::str::FromStr;

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

    use super::*;

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

        let configs = parse_config(config_path.to_str().unwrap()).unwrap();
        assert_eq!(configs, expected);
    }
}

