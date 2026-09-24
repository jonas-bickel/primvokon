//! Authentication strategies supported by kvmd.
//!
//! PiKVM accepts three single-request schemes (`X-KVMD-*` headers, HTTP Basic) and a
//! session cookie obtained from `POST /api/auth/login`. With 2FA enabled the current TOTP
//! code is appended to the password.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Builder, Secret};

use crate::error::{PikvmError, Result};

/// Which HTTP authentication scheme to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Log in once, then send the `auth_token` cookie.
    #[default]
    Session,
    /// Send `X-KVMD-User` / `X-KVMD-Passwd` on every request.
    Headers,
    /// HTTP Basic (`Authorization: Basic …`) on every request.
    Basic,
    /// Use a pre-existing `auth_token` cookie value, never log in.
    Token,
}

impl AuthMethod {
    pub const ALL: [AuthMethod; 4] = [
        AuthMethod::Session,
        AuthMethod::Headers,
        AuthMethod::Basic,
        AuthMethod::Token,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AuthMethod::Session => "Session token (login)",
            AuthMethod::Headers => "Header auth (X-KVMD-User/Passwd)",
            AuthMethod::Basic => "HTTP Basic",
            AuthMethod::Token => "Existing auth_token",
        }
    }
}

/// User credentials, including an optional TOTP secret for 2FA.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    pub user: String,
    pub password: String,
    /// Base32 TOTP secret from `/etc/kvmd/totp.secret`.
    pub totp_secret: Option<String>,
    /// A one-time code entered manually for this login (used when no secret is stored).
    pub totp_code: Option<String>,
    /// Pre-existing session token (only for [`AuthMethod::Token`]).
    pub token: Option<String>,
}

impl Credentials {
    /// Password with the current TOTP code appended when 2FA is configured.
    pub fn effective_password(&self) -> Result<String> {
        self.effective_password_at(now_unix())
    }

    fn effective_password_at(&self, now: u64) -> Result<String> {
        if let Some(code) = self.totp_code.as_deref().filter(|c| !c.is_empty()) {
            return Ok(format!("{}{}", self.password, code.trim()));
        }
        match self.totp_secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(secret) => {
                let code = totp_code(secret, now)?;
                Ok(format!("{}{}", self.password, code))
            }
            None => Ok(self.password.clone()),
        }
    }

    /// Seconds remaining in the current TOTP window, or `None` without 2FA.
    pub fn totp_seconds_remaining(&self) -> Option<u64> {
        self.totp_secret.as_ref()?;
        let now = now_unix();
        Some(30 - (now % 30))
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Compute the 6 digit TOTP for a base32 secret at `now` (unix seconds).
pub fn totp_code(secret_base32: &str, now: u64) -> Result<String> {
    let secret = Secret::try_from_base32(secret_base32.replace(' ', "").to_uppercase())
        .map_err(|e| PikvmError::Totp(e.to_string()))?;
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_step_duration(30)
        .with_secret(secret)
        .build_noncompliant();
    Ok(totp.generate(now).to_string())
}

/// Headers to attach for single-request auth schemes.
#[derive(Debug, Clone, Default)]
pub struct AuthHeaders {
    pub headers: Vec<(&'static str, String)>,
}

impl AuthHeaders {
    /// Build the headers for a request under `method`, using `token` for cookie based auth.
    pub fn build(method: AuthMethod, creds: &Credentials, token: Option<&str>) -> Result<Self> {
        let mut headers = Vec::new();
        match method {
            AuthMethod::Headers => {
                headers.push(("X-KVMD-User", creds.user.clone()));
                headers.push(("X-KVMD-Passwd", creds.effective_password()?));
            }
            AuthMethod::Basic => {
                let raw = format!("{}:{}", creds.user, creds.effective_password()?);
                let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
                headers.push(("Authorization", format!("Basic {encoded}")));
            }
            AuthMethod::Session | AuthMethod::Token => {
                if let Some(t) = token {
                    headers.push(("Cookie", format!("auth_token={t}")));
                }
            }
        }
        Ok(Self { headers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_matches_rfc_vector() {
        // RFC 6238 test secret "12345678901234567890" (base32 GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ)
        let code = totp_code("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", 59).unwrap();
        assert_eq!(code, "287082");
    }

    #[test]
    fn appends_totp_to_password() {
        let creds = Credentials {
            user: "admin".into(),
            password: "foobar".into(),
            totp_secret: Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()),
            ..Default::default()
        };
        assert_eq!(creds.effective_password_at(59).unwrap(), "foobar287082");
    }

    #[test]
    fn manual_code_wins_over_secret() {
        let creds = Credentials {
            password: "pw".into(),
            totp_secret: Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()),
            totp_code: Some("123456".into()),
            ..Default::default()
        };
        assert_eq!(creds.effective_password_at(59).unwrap(), "pw123456");
    }

    #[test]
    fn header_auth_sets_kvmd_headers() {
        let creds = Credentials {
            user: "admin".into(),
            password: "admin".into(),
            ..Default::default()
        };
        let h = AuthHeaders::build(AuthMethod::Headers, &creds, None).unwrap();
        assert_eq!(h.headers[0], ("X-KVMD-User", "admin".to_string()));
        assert_eq!(h.headers[1], ("X-KVMD-Passwd", "admin".to_string()));
    }

    #[test]
    fn basic_auth_is_base64() {
        let creds = Credentials {
            user: "admin".into(),
            password: "admin".into(),
            ..Default::default()
        };
        let h = AuthHeaders::build(AuthMethod::Basic, &creds, None).unwrap();
        assert_eq!(h.headers[0], ("Authorization", "Basic YWRtaW46YWRtaW4=".to_string()));
    }

    #[test]
    fn session_uses_cookie() {
        let creds = Credentials::default();
        let h = AuthHeaders::build(AuthMethod::Session, &creds, Some("abc")).unwrap();
        assert_eq!(h.headers[0], ("Cookie", "auth_token=abc".to_string()));
        let none = AuthHeaders::build(AuthMethod::Session, &creds, None).unwrap();
        assert!(none.headers.is_empty());
    }
}
