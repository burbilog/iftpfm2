//! SFTP (SSH File Transfer Protocol) client implementation
//!
//! This module provides the `SftpClient` which implements the `FileTransferClient`
//! trait for SFTP connections using the ssh2 crate.

use std::io::Read;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::time::Duration;
use ssh2::{Session, Sftp};
use crate::protocols::{FileTransferClient, ProtocolConfig, TransferMode, FtpError};

/// Authentication method for SFTP connections
///
/// Stores the authentication credentials to be used during login.
/// This allows us to defer authentication from connect() to login()
/// for consistency with FTP/FTPS protocols.
enum AuthMethod {
    /// Password authentication
    Password(String),
    /// Keyfile authentication with optional passphrase
    Keyfile { path: String, passphrase: Option<String> },
}

/// SFTP client for SSH File Transfer Protocol connections
pub struct SftpClient {
    _session: Session,
    sftp: Sftp,
    current_dir: String,
    /// Authentication method to use during login
    auth_method: AuthMethod,
}

impl SftpClient {
    /// Helper function to build full path from current directory and filename
    fn full_path(&self, filename: &str) -> String {
        format!("{}/{}", self.current_dir.trim_end_matches('/'), filename)
    }
}

impl FileTransferClient for SftpClient {
    fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        _config: &ProtocolConfig,
        _user: &str,
        password: Option<&str>,
        keyfile_path: Option<&str>,
        keyfile_passphrase: Option<&str>,
    ) -> Result<Self, FtpError>
    where
        Self: Sized,
    {
        // Determine authentication method (validation should have happened during config parsing)
        let auth_method = match (password, keyfile_path) {
            (Some(pwd), _) => AuthMethod::Password(pwd.to_string()),
            (None, Some(keyfile)) => AuthMethod::Keyfile {
                path: keyfile.to_string(),
                passphrase: keyfile_passphrase.map(|s| s.to_string()),
            },
            (None, None) => {
                // This should have been validated during config parsing
                return Err(FtpError::ConnectionError(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "SFTP requires either password or keyfile",
                )));
            }
        };

        // Resolve host to all possible addresses
        let addrs: Vec<std::net::SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(FtpError::ConnectionError)?
            .collect();

        if addrs.is_empty() {
            return Err(FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No addresses found",
            )));
        }

        // Try each address until one succeeds
        let mut last_error = None;
        for addr in addrs {
            // TCP connect with timeout
            let stream = match TcpStream::connect_timeout(&addr, timeout) {
                Ok(s) => s,
                Err(e) => {
                    last_error = Some(FtpError::ConnectionError(e));
                    continue;
                }
            };

            // Set read/write timeout for the stream
            stream
                .set_read_timeout(Some(timeout))
                .map_err(FtpError::ConnectionError)?;
            stream
                .set_write_timeout(Some(timeout))
                .map_err(FtpError::ConnectionError)?;

            // Create SSH session
            let mut session = Session::new().map_err(|e| {
                FtpError::ConnectionError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("[SFTP] Failed to create SSH session: {}", e),
                ))
            })?;

            // Perform SSH handshake
            session.set_tcp_stream(stream);
            session.handshake().map_err(|e| {
                FtpError::ConnectionError(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    format!("[SFTP] SSH handshake failed: {}", e),
                ))
            })?;

            // Set timeout for SSH session operations (blocks operations if no data received)
            session.set_timeout(timeout.as_millis() as u32);

            // Create SFTP channel
            let sftp = session.sftp().map_err(|e| {
                FtpError::ConnectionError(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    format!("[SFTP] Failed to create SFTP channel: {}", e),
                ))
            })?;

            return Ok(SftpClient {
                _session: session,
                sftp,
                current_dir: String::from("/"),
                auth_method,
            });
        }

        Err(last_error.unwrap_or_else(|| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No addresses available",
            ))
        }))
    }

    fn login(&mut self, user: &str, _password: &str) -> Result<(), FtpError> {
        // Perform authentication using the stored auth_method
        // Note: _password parameter is ignored for SFTP since we store the credentials
        // in auth_method during connect(). This maintains API compatibility with FTP/FTPS.
        let auth_result = match &self.auth_method {
            AuthMethod::Password(pwd) => {
                // Password authentication
                self._session.userauth_password(user, pwd)
            }
            AuthMethod::Keyfile { path, passphrase } => {
                // Keyfile authentication (with optional passphrase)
                self._session.userauth_pubkey_file(
                    user,
                    None,
                    Path::new(path),
                    passphrase.as_deref(),
                )
            }
        };

        auth_result.map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("SFTP authentication failed for user '{}': {}", user, e),
            ))
        })
    }

    fn cwd(&mut self, path: &str) -> Result<(), FtpError> {
        // SFTP doesn't have a concept of "current working directory" in the same way as FTP
        // We verify that the path exists and is accessible, then store it for use in nlst
        self.sftp.stat(Path::new(path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("[SFTP] Failed to access path '{}': {}", path, e),
            ))
        })?;
        self.current_dir = path.to_string();
        Ok(())
    }

    fn transfer_type(&mut self, _mode: TransferMode) -> Result<(), FtpError> {
        // No-op: SFTP always uses binary mode
        Ok(())
    }

    fn nlst(&mut self, path: Option<&str>) -> Result<Vec<String>, FtpError> {
        let dir = path.unwrap_or_else(|| self.current_dir.as_str());

        let entries: Vec<(std::path::PathBuf, ssh2::FileStat)> = self.sftp.readdir(Path::new(dir)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("[SFTP] Failed to list directory '{}': {}", dir, e),
            ))
        })?;

        // Filter only regular files (not directories, not . and ..)
        Ok(entries
            .into_iter()
            .filter(|(path, stat)| {
                // Check if it's not a directory and not . or ..
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                !stat.is_dir() && name != "." && name != ".."
            })
            .filter_map(|(path, _)| {
                // Extract the file name as String
                path.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
            })
            .collect())
    }

    fn mdtm(&mut self, filename: &str) -> Result<chrono::NaiveDateTime, FtpError> {
        let full_path = self.full_path(filename);
        let stat = self.sftp.stat(Path::new(&full_path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("[SFTP] Failed to stat file '{}': {}", filename, e),
            ))
        })?;

        // Convert mtime (unix timestamp) to NaiveDateTime
        // stat.mtime is Option<u64>, we need to convert it to i64
        let secs = stat.mtime.ok_or_else(|| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("[SFTP] File '{}' has no modification time", filename),
            ))
        })? as i64;

        chrono::DateTime::from_timestamp(secs, 0)
            .map(|dt| dt.naive_utc())
            .ok_or_else(|| {
                FtpError::ConnectionError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("[SFTP] Invalid modification time: {}", secs),
                ))
            })
    }

    fn size(&mut self, filename: &str) -> Result<usize, FtpError> {
        let full_path = self.full_path(filename);
        let stat = self.sftp.stat(Path::new(&full_path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("[SFTP] Failed to stat file '{}': {}", filename, e),
            ))
        })?;
        Ok(stat.size.unwrap_or(0) as usize)
    }

    fn retr<F, D>(&mut self, filename: &str, mut callback: F) -> Result<D, FtpError>
    where
        F: FnMut(&mut dyn Read) -> Result<D, FtpError>,
    {
        let full_path = self.full_path(filename);
        let mut file = self.sftp.open(Path::new(&full_path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("[SFTP] Failed to open file '{}': {}", filename, e),
            ))
        })?;

        callback(&mut file)
    }

    fn put_file<R: Read>(
        &mut self,
        filename: &str,
        reader: &mut R,
    ) -> Result<u64, FtpError> {
        let full_path = self.full_path(filename);
        let mut file = self.sftp.create(Path::new(&full_path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("[SFTP] Failed to create file '{}': {}", filename, e),
            ))
        })?;

        std::io::copy(reader, &mut file).map_err(FtpError::ConnectionError)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError> {
        let from_path = self.full_path(from);
        let to_path = self.full_path(to);
        self.sftp.rename(Path::new(&from_path), Path::new(&to_path), None).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("[SFTP] Failed to rename '{}' to '{}': {}", from, to, e),
            ))
        })
    }

    fn rm(&mut self, filename: &str) -> Result<(), FtpError> {
        let full_path = self.full_path(filename);
        self.sftp.unlink(Path::new(&full_path)).map_err(|e| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("[SFTP] Failed to delete file '{}': {}", filename, e),
            ))
        })
    }

    fn quit(self) -> Result<(), FtpError> {
        // Session and SFTP channel are dropped automatically
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sftp_client_send() {
        // Verify that SftpClient implements Send
        fn assert_send<T: Send>() {}
        assert_send::<SftpClient>();
    }
}
