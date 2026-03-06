//! Plain FTP client implementation
//!
//! This module provides the `FtpClient` which implements the `FileTransferClient`
//! trait for standard (unencrypted) FTP connections.

use std::io::Read;
use std::net::ToSocketAddrs;
use std::time::Duration;
use suppaftp::FtpStream;

use crate::protocols::{FileTransferClient, ProtocolConfig, TransferMode, FtpError};

/// Default timeout for read/write operations on control connection
const DEFAULT_RW_TIMEOUT: Duration = Duration::from_secs(60);

/// FTP client for plain (unencrypted) FTP connections
pub struct FtpClient {
    stream: FtpStream,
}

impl FileTransferClient for FtpClient {
    fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        _config: &ProtocolConfig,
        _user: &str,
        _password: Option<&str>,
        _keyfile_path: Option<&str>,
        _keyfile_passphrase: Option<&str>,
    ) -> Result<Self, FtpError>
    where
        Self: Sized,
    {
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
            match FtpStream::connect_timeout(addr, timeout) {
                Ok(stream) => {
                    // Set read/write timeout on the control connection
                    // This prevents hanging on commands like QUIT, CWD, etc.
                    let tcp_stream = stream.get_ref();
                    tcp_stream.set_read_timeout(Some(DEFAULT_RW_TIMEOUT))
                        .map_err(FtpError::ConnectionError)?;
                    tcp_stream.set_write_timeout(Some(DEFAULT_RW_TIMEOUT))
                        .map_err(FtpError::ConnectionError)?;
                    return Ok(FtpClient { stream });
                }
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| {
            FtpError::ConnectionError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No addresses available",
            ))
        }))
    }

    fn login(&mut self, user: &str, password: &str) -> Result<(), FtpError> {
        self.stream.login(user, password)
    }

    fn cwd(&mut self, path: &str) -> Result<(), FtpError> {
        self.stream.cwd(path)
    }

    fn transfer_type(&mut self, mode: TransferMode) -> Result<(), FtpError> {
        self.stream.transfer_type(mode.into())
    }

    fn nlst(&mut self, path: Option<&str>) -> Result<Vec<String>, FtpError> {
        // Get all names from NLST
        let _ = crate::logging::log_with_thread("FTP: Calling nlst() on stream...".to_string(), None);
        let all_names = self.stream.nlst(path)?;
        let _ = crate::logging::log_with_thread(format!("FTP: nlst() returned {} entries", all_names.len()), None);

        // Filter out . and .. only (no SIZE check - too slow for large directories)
        // Directories will fail during transfer with appropriate error
        let files_only: Vec<String> = all_names
            .into_iter()
            .filter(|name| name != "." && name != "..")
            .collect();

        Ok(files_only)
    }

    fn mdtm(&mut self, filename: &str) -> Result<chrono::NaiveDateTime, FtpError> {
        self.stream.mdtm(filename)
    }

    fn size(&mut self, filename: &str) -> Result<usize, FtpError> {
        self.stream.size(filename)
    }

    fn retr<F, D>(&mut self, filename: &str, callback: F) -> Result<D, FtpError>
    where
        F: FnMut(&mut dyn Read) -> Result<D, FtpError>,
    {
        self.stream.retr(filename, callback)
    }

    fn put_file<R: Read>(
        &mut self,
        filename: &str,
        reader: &mut R,
    ) -> Result<u64, FtpError> {
        self.stream.put_file(filename, reader)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError> {
        self.stream.rename(from, to)
    }

    fn rm(&mut self, filename: &str) -> Result<(), FtpError> {
        self.stream.rm(filename)
    }

    fn quit(mut self) -> Result<(), FtpError> {
        self.stream.quit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ftp_client_send() {
        // Verify that FtpClient implements Send
        fn assert_send<T: Send>() {}
        assert_send::<FtpClient>();
        // Note: FtpStream is not Sync, so FtpClient won't be either
    }
}
