//! Protocol implementations for file transfer clients
//!
//! This module provides a trait-based abstraction for different file transfer
//! protocols (FTP, FTPS, etc.). Each protocol implements the `FileTransferClient`
//! trait, allowing easy extension with new protocols.

pub mod ftp;
pub mod ftps;
pub mod sftp;

// Re-export protocol clients for convenience
pub use ftp::FtpClient;
pub use ftps::FtpsClient;
pub use sftp::SftpClient;

use crate::config::Protocol;
use std::io::Read;
use std::time::Duration;

/// Configuration for protocol connections
#[derive(Debug, Clone, Copy)]
pub struct ProtocolConfig {
    /// Skip TLS certificate verification (for FTPS with self-signed certs)
    pub insecure_skip_verify: bool,
}

/// Error type for protocol operations
pub type FtpError = suppaftp::FtpError;

/// Unified trait for file transfer client operations
///
/// This trait provides a common interface for different file transfer protocols
/// (FTP, FTPS, SFTP, etc.). All methods return `FtpError` for consistency.
pub trait FileTransferClient {
    /// Connect to a server
    ///
    /// # Arguments
    /// * `host` - Server hostname or IP address
    /// * `port` - Server port number
    /// * `timeout` - Connection timeout
    /// * `config` - Protocol-specific configuration
    /// * `user` - Username for authentication (for SFTP)
    /// * `password` - Optional password for authentication
    /// * `keyfile_path` - Optional path to SSH private key (for SFTP)
    /// * `keyfile_passphrase` - Optional passphrase for SSH private key (for SFTP)
    fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        config: &ProtocolConfig,
        user: &str,
        password: Option<&str>,
        keyfile_path: Option<&str>,
        keyfile_passphrase: Option<&str>,
    ) -> Result<Self, FtpError>
    where
        Self: Sized;

    /// Authenticate with username and password
    fn login(&mut self, user: &str, password: &str) -> Result<(), FtpError>;

    /// Change working directory
    fn cwd(&mut self, path: &str) -> Result<(), FtpError>;

    /// Set transfer type (binary or ASCII)
    fn transfer_type(&mut self, mode: TransferMode) -> Result<(), FtpError>;

    /// Get list of files in directory (NLST command)
    fn nlst(&mut self, path: Option<&str>) -> Result<Vec<String>, FtpError>;

    /// Get file modification time (MDTM command)
    fn mdtm(&mut self, filename: &str) -> Result<chrono::NaiveDateTime, FtpError>;

    /// Get file size (SIZE command)
    fn size(&mut self, filename: &str) -> Result<usize, FtpError>;

    /// Retrieve file contents
    ///
    /// This is a callback-based API to handle streaming data.
    /// The callback receives a reader and must return the desired result.
    fn retr<F, D>(&mut self, filename: &str, callback: F) -> Result<D, FtpError>
    where
        F: FnMut(&mut dyn Read) -> Result<D, FtpError>;

    /// Upload file contents
    fn put_file<R: Read>(
        &mut self,
        filename: &str,
        reader: &mut R,
    ) -> Result<u64, FtpError>;

    /// Rename a file
    fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError>;

    /// Remove/delete a file
    fn rm(&mut self, filename: &str) -> Result<(), FtpError>;

    /// Quit/disconnect from the server
    fn quit(self) -> Result<(), FtpError>;
}

/// Transfer mode for file operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    /// Binary mode (untranslated)
    Binary,
    /// ASCII/text mode (with line ending conversion)
    ASCII,
}

impl From<TransferMode> for suppaftp::types::FileType {
    fn from(mode: TransferMode) -> Self {
        match mode {
            TransferMode::Binary => suppaftp::types::FileType::Binary,
            TransferMode::ASCII => suppaftp::types::FileType::Ascii(suppaftp::types::FormatControl::NonPrint),
        }
    }
}

/// Macro to delegate method calls to enum variants
///
/// This reduces boilerplate by generating match expressions that delegate
/// to the appropriate client type (Ftp, Ftps, or Sftp).
macro_rules! delegate {
    ($self:expr, $method:ident $(, $arg:expr)* $(,)?) => {
        match $self {
            Client::Ftp(client) => client.$method($($arg),*),
            Client::Ftps(client) => client.$method($($arg),*),
            Client::Sftp(client) => client.$method($($arg),*),
        }
    };
}

/// Enum wrapper for FTP/FTPS/SFTP clients
///
/// Since `FileTransferClient` has generic methods, it cannot be used as
/// `dyn FileTransferClient`. This enum provides a concrete type that can
/// be used to hold FTP, FTPS, or SFTP clients.
pub enum Client {
    Ftp(FtpClient),
    Ftps(FtpsClient),
    Sftp(SftpClient),
}

impl Client {
    /// Connect to a server and create a client of the appropriate type
    pub fn connect(
        proto: &Protocol,
        host: &str,
        port: u16,
        timeout: Duration,
        insecure_skip_verify: bool,
        user: &str,
        password: Option<&str>,
        keyfile_path: Option<&str>,
        keyfile_passphrase: Option<&str>,
    ) -> Result<Self, FtpError> {
        let config = ProtocolConfig {
            insecure_skip_verify,
        };

        match proto {
            Protocol::Ftp => Ok(Client::Ftp(FtpClient::connect(host, port, timeout, &config, user, password, keyfile_path, keyfile_passphrase)?)),
            Protocol::Ftps => Ok(Client::Ftps(FtpsClient::connect(host, port, timeout, &config, user, password, keyfile_path, keyfile_passphrase)?)),
            Protocol::Sftp => Ok(Client::Sftp(SftpClient::connect(host, port, timeout, &config, user, password, keyfile_path, keyfile_passphrase)?)),
        }
    }

    /// Authenticate with username and password
    pub fn login(&mut self, user: &str, password: &str) -> Result<(), FtpError> {
        delegate!(self, login, user, password)
    }

    /// Change working directory
    pub fn cwd(&mut self, path: &str) -> Result<(), FtpError> {
        delegate!(self, cwd, path)
    }

    /// Set transfer type (binary or ASCII)
    pub fn transfer_type(&mut self, mode: TransferMode) -> Result<(), FtpError> {
        delegate!(self, transfer_type, mode)
    }

    /// Get list of files in directory (NLST command)
    pub fn nlst(&mut self, path: Option<&str>) -> Result<Vec<String>, FtpError> {
        delegate!(self, nlst, path)
    }

    /// Get file modification time (MDTM command)
    pub fn mdtm(&mut self, filename: &str) -> Result<chrono::NaiveDateTime, FtpError> {
        delegate!(self, mdtm, filename)
    }

    /// Get file size (SIZE command)
    pub fn size(&mut self, filename: &str) -> Result<usize, FtpError> {
        delegate!(self, size, filename)
    }

    /// Retrieve file contents
    pub fn retr<F, D>(&mut self, filename: &str, callback: F) -> Result<D, FtpError>
    where
        F: FnMut(&mut dyn Read) -> Result<D, FtpError>,
    {
        delegate!(self, retr, filename, callback)
    }

    /// Upload file contents
    pub fn put_file<R: Read>(
        &mut self,
        filename: &str,
        reader: &mut R,
    ) -> Result<u64, FtpError> {
        delegate!(self, put_file, filename, reader)
    }

    /// Rename a file
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError> {
        delegate!(self, rename, from, to)
    }

    /// Remove/delete a file
    pub fn rm(&mut self, filename: &str) -> Result<(), FtpError> {
        delegate!(self, rm, filename)
    }

    /// Quit/disconnect from the server
    pub fn quit(self) -> Result<(), FtpError> {
        delegate!(self, quit)
    }
}
