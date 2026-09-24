//! `/api/auth/*`

use reqwest::Method;
use serde_json::Value;

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;

pub struct AuthApi<'a>(pub(crate) &'a PikvmClient);

impl AuthApi<'_> {
    /// `POST /api/auth/login` – obtain a session token (also called automatically).
    pub async fn login(&self) -> Result<String> {
        self.0.login().await
    }

    /// `GET /api/auth/check` – `Ok(())` when the current credentials are accepted.
    pub async fn check(&self) -> Result<()> {
        self.0.call_ok(Method::GET, "/api/auth/check", &Query::new(), Body::Empty).await?;
        Ok(())
    }

    /// `POST /api/auth/logout` – invalidate the session token.
    pub async fn logout(&self) -> Result<()> {
        let _: Value = self.0.post_result("/api/auth/logout", Query::new(), Body::Empty).await?;
        self.0.clear_token().await;
        Ok(())
    }
}
