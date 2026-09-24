//! Error type shared by every PiKVM call.

use thiserror::Error;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, PikvmError>;

/// Everything that can go wrong when talking to a PiKVM.
#[derive(Debug, Error)]
pub enum PikvmError {
    /// The base URL could not be parsed or is not http(s).
    #[error("invalid PiKVM URL: {0}")]
    InvalidUrl(String),

    /// Network level failure (DNS, TLS, connection refused, timeout).
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// No credentials or token were presented (HTTP 401).
    #[error("not authenticated (401)")]
    Unauthenticated,

    /// Credentials or token rejected (HTTP 403).
    #[error("access denied (403): {0}")]
    Forbidden(String),

    /// The subsystem is unavailable, e.g. snapshot without video (HTTP 503).
    #[error("unavailable (503): {0}")]
    Unavailable(String),

    /// Any other non-success HTTP status. `body` is the raw response text.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    /// The `ok: false` envelope returned by kvmd with the error it reported.
    #[error("PiKVM error {error}: {message}")]
    Api { error: String, message: String },

    /// Response could not be decoded.
    #[error("invalid response: {0}")]
    Decode(String),

    /// WebSocket failure.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// The MJPEG stream ended or is malformed.
    #[error("video stream error: {0}")]
    Stream(String),

    /// TOTP secret could not be parsed.
    #[error("invalid TOTP secret: {0}")]
    Totp(String),
}

impl From<serde_json::Error> for PikvmError {
    fn from(e: serde_json::Error) -> Self {
        PikvmError::Decode(e.to_string())
    }
}

impl From<url::ParseError> for PikvmError {
    fn from(e: url::ParseError) -> Self {
        PikvmError::InvalidUrl(e.to_string())
    }
}

impl PikvmError {
    /// `true` for errors that a reconnect could fix.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            PikvmError::Transport(_) | PikvmError::WebSocket(_) | PikvmError::Stream(_) | PikvmError::Unavailable(_)
        )
    }

    /// `true` when the credentials should be re-entered.
    pub fn is_auth(&self) -> bool {
        matches!(self, PikvmError::Unauthenticated | PikvmError::Forbidden(_))
    }
}
