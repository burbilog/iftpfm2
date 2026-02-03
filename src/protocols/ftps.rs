//! FTPS (FTP over TLS/SSL) client implementation
//!
//! This module provides the `FtpsClient` which implements the `FileTransferClient`
//! trait for FTPS connections using rustls for TLS.

use std::io::Read;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;
use suppaftp::{RustlsConnector, RustlsFtpStream, types::Mode};

use crate::logging::log_with_thread;
use crate::protocols::{FileTransferClient, ProtocolConfig, TransferMode, FtpError};

// Module for insecure certificate verification (for self-signed certs)
mod danger {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::DigitallySignedStruct;

    /// Certificate verifier that accepts any certificate
    ///
    /// WARNING: This should only be used for testing or with trusted
    /// self-signed certificates. Using this in production is a security risk.
    #[derive(Debug)]
    pub struct NoCertificateVerification;

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}

/// FTPS client for encrypted FTP over TLS/SSL connections
pub struct FtpsClient {
    stream: RustlsFtpStream,
}

impl FileTransferClient for FtpsClient {
    fn connect(
        host: &str,
        port: u16,
        timeout: Duration,
        config: &ProtocolConfig,
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

        // Build TLS configuration
        let provider = rustls::crypto::ring::default_provider();
        let builder = rustls::ClientConfig::builder_with_provider(Arc::new(provider));

        let tls_config = if config.insecure_skip_verify {
            builder
                .with_safe_default_protocol_versions()
                .map_err(|e| FtpError::SecureError(e.to_string()))?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(danger::NoCertificateVerification))
                .with_no_client_auth()
        } else {
            let mut root_store = rustls::RootCertStore::empty();
            let certs_result = rustls_native_certs::load_native_certs();
            for cert in certs_result.certs {
                root_store.add(cert).ok();
            }
            if !certs_result.errors.is_empty() {
                let _ = log_with_thread(
                    format!(
                        "Warning: failed to load some native certificates: {:?}",
                        certs_result.errors
                    ),
                    None,
                );
            }
            builder
                .with_safe_default_protocol_versions()
                .map_err(|e| FtpError::SecureError(e.to_string()))?
                .with_root_certificates(root_store)
                .with_no_client_auth()
        };

        // Wrap the config in Arc so we can create multiple connectors from it
        let tls_config = Arc::new(tls_config);

        // Try each address until one succeeds
        let mut last_error = None;
        for addr in addrs {
            match RustlsFtpStream::connect_timeout(addr, timeout) {
                Ok(secure_stream) => {
                    // Create a new connector from the shared Arc for this attempt
                    let connector = RustlsConnector::from(tls_config.clone());
                    match secure_stream.into_secure(connector, host) {
                        Ok(mut stream) => {
                            // Enable data channel protection (PROT P) for secure data transfer
                            let _ = stream.custom_command("PROT P", &[suppaftp::Status::CommandOk])?;
                            stream.set_mode(Mode::Passive);
                            stream.set_passive_nat_workaround(true);
                            return Ok(FtpsClient { stream });
                        }
                        Err(e) => last_error = Some(e),
                    }
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
        self.stream.nlst(path)
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
    fn test_ftps_client_send() {
        // Verify that FtpsClient implements Send
        fn assert_send<T: Send>() {}
        assert_send::<FtpsClient>();
        // Note: RustlsFtpStream is not Sync, so FtpsClient won't be either
    }
}
