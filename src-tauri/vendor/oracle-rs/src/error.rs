//! Error types for the Oracle driver
//!
//! This module defines all error types that can occur during Oracle database
//! operations, from low-level protocol errors to high-level connection errors.

use std::io;
use thiserror::Error;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the Oracle driver
#[derive(Error, Debug)]
#[allow(missing_docs)]
pub enum Error {
    // =========================================================================
    // Protocol Errors
    // =========================================================================
    /// Invalid packet type received
    #[error("invalid packet type: {0}")]
    InvalidPacketType(u8),

    /// Invalid message type received
    #[error("invalid message type: {0}")]
    InvalidMessageType(u8),

    /// Packet too short to contain valid header
    #[error("packet too short: expected at least {expected} bytes, got {actual}")]
    PacketTooShort { expected: usize, actual: usize },

    /// Unexpected packet type received
    #[error("unexpected packet type: expected {expected:?}, got {actual:?}")]
    UnexpectedPacketType {
        expected: crate::constants::PacketType,
        actual: crate::constants::PacketType,
    },

    /// Protocol version not supported
    #[error("server protocol version {0} not supported (minimum: {1})")]
    ProtocolVersionNotSupported(u16, u16),

    /// General protocol error
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Protocol error (alternate form)
    #[error("protocol error: {0}")]
    ProtocolError(String),

    // =========================================================================
    // Buffer Errors
    // =========================================================================
    /// Buffer underflow - not enough data to read
    #[error("buffer underflow: need {needed} bytes but only {available} available")]
    BufferUnderflow { needed: usize, available: usize },

    /// Buffer overflow - not enough space to write
    #[error("buffer overflow: need {needed} bytes but only {available} available")]
    BufferOverflow { needed: usize, available: usize },

    /// Invalid length indicator
    #[error("invalid length indicator: {0}")]
    InvalidLengthIndicator(u8),

    // =========================================================================
    // Connection Errors
    // =========================================================================
    /// Connection refused by server
    #[error("connection refused{}: {}",
        error_code.map(|c| format!(" (error {})", c)).unwrap_or_default(),
        message.as_deref().unwrap_or("unknown reason"))]
    ConnectionRefused {
        error_code: Option<u32>,
        message: Option<String>,
    },

    /// Connection redirected
    #[error("connection redirected to: {address}")]
    ConnectionRedirected { address: String },

    /// Connection redirect (simple form)
    #[error("connection redirected to: {0}")]
    ConnectionRedirect(String),

    /// Connection closed unexpectedly
    #[error("connection closed unexpectedly")]
    ConnectionClosed,

    /// Connection closed by server with reason
    #[error("{0}")]
    ConnectionClosedByServer(String),

    /// Connection not ready for operations
    #[error("connection not ready")]
    ConnectionNotReady,

    /// Cursor is closed
    #[error("cursor is closed")]
    CursorClosed,

    /// Invalid cursor state
    #[error("invalid cursor: {0}")]
    InvalidCursor(String),

    /// Connection timeout
    #[error("connection timeout after {0:?}")]
    ConnectionTimeout(std::time::Duration),

    /// Invalid connection string
    #[error("invalid connection string: {0}")]
    InvalidConnectionString(String),

    /// Invalid service name (ORA-12514)
    #[error("invalid service name{}: {}",
        service_name.as_ref().map(|s| format!(": {}", s)).unwrap_or_default(),
        message.as_deref().unwrap_or("service not found"))]
    InvalidServiceName {
        service_name: Option<String>,
        message: Option<String>,
    },

    /// Invalid SID (ORA-12505)
    #[error("invalid SID{}: {}",
        sid.as_ref().map(|s| format!(": {}", s)).unwrap_or_default(),
        message.as_deref().unwrap_or("SID not found"))]
    InvalidSid {
        sid: Option<String>,
        message: Option<String>,
    },

    // =========================================================================
    // Authentication Errors
    // =========================================================================
    /// Authentication failed
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Invalid credentials
    #[error("invalid username or password")]
    InvalidCredentials,

    /// Unsupported verifier type
    #[error("unsupported verifier type: {0:#x}")]
    UnsupportedVerifierType(u32),

    // =========================================================================
    // Database Errors
    // =========================================================================
    /// Oracle database error with error code
    #[error("ORA-{code:05}: {message}")]
    OracleError { code: u32, message: String },

    /// SQL execution error
    #[error("SQL execution error: {0}")]
    SqlError(String),

    /// No data found
    #[error("no data found")]
    NoDataFound,

    /// Server error with code and message
    #[error("server error ({code}): {message}")]
    ServerError { code: u32, message: String },

    // =========================================================================
    // Data Type Errors
    // =========================================================================
    /// Invalid data type
    #[error("invalid data type: {0}")]
    InvalidDataType(u16),

    /// Invalid Oracle type number
    #[error("invalid Oracle type: {0}")]
    InvalidOracleType(u8),

    /// Data conversion error
    #[error("data conversion error: {0}")]
    DataConversionError(String),

    /// NULL value encountered where not expected
    #[error("unexpected NULL value")]
    UnexpectedNull,

    // =========================================================================
    // I/O Errors
    // =========================================================================
    /// Underlying I/O error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    // =========================================================================
    // Feature Errors
    // =========================================================================
    /// Feature not supported
    #[error("feature not supported: {0}")]
    FeatureNotSupported(String),

    /// Native network encryption required but not supported
    #[error("native network encryption and data integrity is required but not supported")]
    NativeNetworkEncryptionRequired,

    // =========================================================================
    // Internal Errors
    // =========================================================================
    /// Internal error (should not happen)
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create a new Oracle database error
    pub fn oracle(code: u32, message: impl Into<String>) -> Self {
        Error::OracleError {
            code,
            message: message.into(),
        }
    }

    /// Check if this is a "no data found" error
    pub fn is_no_data_found(&self) -> bool {
        matches!(self, Error::NoDataFound)
            || matches!(self, Error::OracleError { code, .. } if *code == crate::constants::error_code::NO_DATA_FOUND)
    }

    /// Check if this is a connection-related error
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            Error::ConnectionRefused { .. }
                | Error::ConnectionClosed
                | Error::ConnectionClosedByServer(_)
                | Error::ConnectionTimeout(_)
                | Error::Io(_)
        )
    }

    /// Check if this error is recoverable (can retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Error::ConnectionTimeout(_) | Error::ConnectionClosed)
    }
}

/// Error for a listener that closed the socket before a complete TNS packet arrived.
///
/// Any Oracle text already read (`(DESCRIPTION=…)`, `ORA-…`, `TNS-…`) is kept in full.
pub(crate) fn closed_response(bytes: &[u8]) -> Error {
    if let Some(text) = server_error_text(bytes) {
        return Error::ConnectionClosedByServer(text);
    }
    if bytes.is_empty() {
        return Error::ConnectionClosedByServer(
            "I/O error: early eof. The listener closed the connection without sending an error packet.".into(),
        );
    }
    Error::ConnectionClosedByServer(format!(
        "I/O error: early eof. The listener closed the connection after {} bytes: {}",
        bytes.len(),
        bytes_preview(bytes),
    ))
}

/// Oracle listener or server text embedded in a raw response.
pub(crate) fn server_error_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let upper = text.to_ascii_uppercase();
    let start = [
        "(DESCRIPTION",
        "(ERR=",
        "ORA-",
        "TNS-",
        "ERROR_STACK",
        "(ERROR=",
    ]
    .iter()
    .filter_map(|marker| upper.find(marker))
    .min()?;
    let message = trim_server_text(&text[start..]);
    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

fn trim_server_text(text: &str) -> String {
    text.trim_matches(|c: char| {
        c == '\0' || (c.is_control() && c != '\n' && c != '\r' && c != '\t')
    })
    .trim()
    .to_string()
}

fn bytes_preview(bytes: &[u8]) -> String {
    let shown = bytes.len().min(512);
    let lossy = String::from_utf8_lossy(&bytes[..shown]);
    let printable = lossy
        .chars()
        .filter(|c| c.is_ascii_graphic() || c.is_whitespace())
        .count();
    let total = lossy.chars().count().max(1);
    if printable * 4 >= total * 3 {
        let mut text = trim_server_text(&lossy);
        if bytes.len() > shown {
            text.push_str(" …");
        }
        return text;
    }
    let hex = bytes[..shown]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > shown {
        format!("{hex} …")
    } else {
        hex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_error_display() {
        let err = Error::oracle(1017, "invalid username/password");
        assert_eq!(err.to_string(), "ORA-01017: invalid username/password");
    }

    #[test]
    fn test_is_no_data_found() {
        assert!(Error::NoDataFound.is_no_data_found());
        assert!(Error::oracle(1403, "no data found").is_no_data_found());
        assert!(!Error::oracle(1017, "test").is_no_data_found());
    }

    #[test]
    fn test_is_connection_error() {
        assert!(Error::ConnectionClosed.is_connection_error());
        assert!(Error::ConnectionRefused {
            error_code: Some(12514),
            message: Some("test".to_string()),
        }
        .is_connection_error());
        assert!(!Error::NoDataFound.is_connection_error());
    }

    #[test]
    fn closed_response_keeps_listener_text() {
        let packet = b"\x00\x2a\x00\x00\x04\x00\x00\x00(DESCRIPTION=(ERR=12514)(ERROR_STACK=(ERROR=(CODE=12514)(EMFI=4))))";
        let error = closed_response(packet);
        let message = error.to_string();
        assert!(message.starts_with("(DESCRIPTION=(ERR=12514)"));
        assert!(message.contains("(ERROR=(CODE=12514)(EMFI=4))"));
        assert!(!message.contains("early eof"));
    }

    #[test]
    fn closed_response_without_bytes_says_no_packet() {
        let message = closed_response(&[]).to_string();
        assert!(message.contains("early eof"));
        assert!(message.contains("without sending an error packet"));
    }

    #[test]
    fn server_error_text_finds_ora_code() {
        let bytes = b"\x00\x01junkORA-12505: TNS:listener does not currently know of SID given in connect descriptor\0";
        assert_eq!(
            server_error_text(bytes).as_deref(),
            Some("ORA-12505: TNS:listener does not currently know of SID given in connect descriptor")
        );
    }
}
