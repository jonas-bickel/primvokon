//! HTTP client with authentication, error mapping and the kvmd `{ok, result}` envelope.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use url::Url;

use crate::auth::{AuthHeaders, AuthMethod, Credentials};
use crate::error::{PikvmError, Result};

/// Everything needed to reach one PiKVM.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub base_url: Url,
    pub method: AuthMethod,
    pub credentials: Credentials,
    /// PiKVM ships with a self-signed certificate; on by default.
    pub accept_invalid_certs: bool,
    pub timeout: Duration,
}

impl ConnectionConfig {
    /// Parse and normalise a user-entered base URL (`pikvm.local`, `https://10.0.0.5:8443/kvm`).
    pub fn parse_base_url(input: &str) -> Result<Url> {
        let trimmed = input.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(PikvmError::InvalidUrl("empty URL".into()));
        }
        let with_scheme = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };
        let url = Url::parse(&with_scheme)?;
        match url.scheme() {
            "http" | "https" => {}
            other => return Err(PikvmError::InvalidUrl(format!("unsupported scheme {other}"))),
        }
        if url.host_str().is_none() {
            return Err(PikvmError::InvalidUrl("missing host".into()));
        }
        Ok(url)
    }

    pub fn new(base_url: Url, method: AuthMethod, credentials: Credentials) -> Self {
        Self {
            base_url,
            method,
            credentials,
            accept_invalid_certs: true,
            timeout: Duration::from_secs(20),
        }
    }
}

/// Raw HTTP response, used by the API explorer and for non-JSON endpoints.
#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Bytes,
}

impl RawResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Result<Value> {
        Ok(serde_json::from_slice(&self.body)?)
    }
}

/// Request body variants used by the API.
#[derive(Debug, Clone)]
pub enum Body {
    Empty,
    Text(String),
    Bytes(Bytes),
    Json(Value),
    Form(Vec<(String, String)>),
}

/// Query string builder that encodes booleans the way kvmd expects (`1`/`0`).
#[derive(Debug, Clone, Default)]
pub struct Query(pub Vec<(&'static str, String)>);

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, key: &'static str, value: impl ToString) -> Self {
        self.0.push((key, value.to_string()));
        self
    }

    pub fn flag(mut self, key: &'static str, value: bool) -> Self {
        self.0.push((key, if value { "1" } else { "0" }.to_string()));
        self
    }

    pub fn opt(self, key: &'static str, value: Option<impl ToString>) -> Self {
        match value {
            Some(v) => self.push(key, v),
            None => self,
        }
    }

    pub fn opt_flag(self, key: &'static str, value: Option<bool>) -> Self {
        match value {
            Some(v) => self.flag(key, v),
            None => self,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: T,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_msg: String,
}

struct Inner {
    cfg: ConnectionConfig,
    http: reqwest::Client,
    token: RwLock<Option<String>>,
}

/// Cloneable handle to one PiKVM.
#[derive(Clone)]
pub struct PikvmClient {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for PikvmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PikvmClient")
            .field("base_url", &self.inner.cfg.base_url.as_str())
            .field("method", &self.inner.cfg.method)
            .finish()
    }
}

impl PikvmClient {
    pub fn new(cfg: ConnectionConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(cfg.accept_invalid_certs)
            .danger_accept_invalid_hostnames(cfg.accept_invalid_certs)
            .timeout(cfg.timeout)
            .user_agent(concat!("primvokon/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let token = match cfg.method {
            AuthMethod::Token => cfg.credentials.token.clone(),
            _ => None,
        };
        Ok(Self {
            inner: Arc::new(Inner {
                cfg,
                http,
                token: RwLock::new(token),
            }),
        })
    }

    pub fn config(&self) -> &ConnectionConfig {
        &self.inner.cfg
    }

    pub fn base_url(&self) -> &Url {
        &self.inner.cfg.base_url
    }

    /// Session token currently in use (cookie auth only).
    pub async fn token(&self) -> Option<String> {
        self.inner.token.read().await.clone()
    }

    /// Absolute URL for an API path (`/api/info`).
    pub fn url(&self, path: &str) -> Url {
        let mut url = self.inner.cfg.base_url.clone();
        let base_path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{base_path}{path}"));
        url.set_query(None);
        url
    }

    /// `ws(s)://…/api/ws?stream=…` for [`crate::ws::WsClient`].
    pub fn ws_url(&self, stream: bool) -> Url {
        let mut url = self.url("/api/ws");
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        let _ = url.set_scheme(scheme);
        url.set_query(Some(if stream { "stream=1" } else { "stream=0" }));
        url
    }

    /// MJPEG stream URL.
    pub fn stream_url(&self) -> Url {
        let mut url = self.url("/streamer/stream");
        url.set_query(Some("dual_final_frames=1"));
        url
    }

    /// Headers that authenticate a request (also used for websocket/stream handshakes).
    pub async fn auth_headers(&self) -> Result<AuthHeaders> {
        let token = self.inner.token.read().await.clone();
        AuthHeaders::build(self.inner.cfg.method, &self.inner.cfg.credentials, token.as_deref())
    }

    /// Log in with cookie auth when necessary. No-op for header/basic/token modes.
    pub async fn ensure_authenticated(&self) -> Result<()> {
        if self.inner.cfg.method == AuthMethod::Session && self.inner.token.read().await.is_none() {
            self.login().await?;
        }
        Ok(())
    }

    /// `POST /api/auth/login` and remember the `auth_token` cookie.
    pub async fn login(&self) -> Result<String> {
        let creds = &self.inner.cfg.credentials;
        let form = [
            ("user", creds.user.clone()),
            ("passwd", creds.effective_password()?),
        ];
        let resp = self
            .inner
            .http
            .post(self.url("/api/auth/login"))
            .form(&form)
            .send()
            .await?;
        let status = resp.status();
        let token = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(parse_auth_token_cookie);
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status(status, body));
        }
        let token = token.ok_or_else(|| PikvmError::Decode("login response had no auth_token cookie".into()))?;
        *self.inner.token.write().await = Some(token.clone());
        Ok(token)
    }

    /// Forget the session token locally (call [`crate::api::auth`] logout to invalidate it).
    pub async fn clear_token(&self) {
        *self.inner.token.write().await = None;
    }

    /// Execute a request and return the raw response, retrying once after a fresh login
    /// when a cookie session expired.
    pub async fn call(&self, method: Method, path: &str, query: &Query, body: Body) -> Result<RawResponse> {
        self.ensure_authenticated().await?;
        let resp = self.send(method.clone(), path, query, body.clone()).await?;
        if resp.status == StatusCode::UNAUTHORIZED.as_u16() && self.inner.cfg.method == AuthMethod::Session {
            self.clear_token().await;
            self.login().await?;
            return self.send(method, path, query, body).await;
        }
        Ok(resp)
    }

    async fn send(&self, method: Method, path: &str, query: &Query, body: Body) -> Result<RawResponse> {
        let mut rb = self.inner.http.request(method, self.url(path));
        if !query.is_empty() {
            rb = rb.query(&query.0);
        }
        for (name, value) in self.auth_headers().await?.headers {
            rb = rb.header(name, value);
        }
        rb = match body {
            Body::Empty => rb,
            Body::Text(t) => rb.body(t),
            Body::Bytes(b) => rb.body(b),
            Body::Json(v) => rb.json(&v),
            Body::Form(f) => rb.form(&f),
        };
        let resp = rb.send().await?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = resp.bytes().await?;
        Ok(RawResponse { status, content_type, body })
    }

    /// Execute and return the raw response only when the status is 2xx.
    pub async fn call_ok(&self, method: Method, path: &str, query: &Query, body: Body) -> Result<RawResponse> {
        let resp = self.call(method, path, query, body).await?;
        check_status(resp)
    }

    /// GET an endpoint and decode the `result` field of the envelope.
    pub async fn get_result<T: DeserializeOwned>(&self, path: &str, query: Query) -> Result<T> {
        let resp = self.call(Method::GET, path, &query, Body::Empty).await?;
        decode_envelope(resp)
    }

    /// POST to an endpoint and decode the `result` field.
    pub async fn post_result<T: DeserializeOwned>(&self, path: &str, query: Query, body: Body) -> Result<T> {
        let resp = self.call(Method::POST, path, &query, body).await?;
        decode_envelope(resp)
    }

    /// POST to an endpoint whose result is an empty object.
    pub async fn post_ok(&self, path: &str, query: Query) -> Result<()> {
        let _: Value = self.post_result(path, query, Body::Empty).await?;
        Ok(())
    }

    /// Build a stream-capable request (used by the MJPEG reader and log follow).
    pub async fn stream_request(&self, url: Url) -> Result<reqwest::RequestBuilder> {
        self.ensure_authenticated().await?;
        let mut rb = self.inner.http.get(url).timeout(Duration::from_secs(60 * 60 * 24));
        for (name, value) in self.auth_headers().await?.headers {
            rb = rb.header(name, value);
        }
        Ok(rb)
    }

    /// Native TLS connector matching this client's certificate policy (for websockets).
    pub fn tls_connector(&self) -> Result<native_tls::TlsConnector> {
        native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(self.inner.cfg.accept_invalid_certs)
            .danger_accept_invalid_hostnames(self.inner.cfg.accept_invalid_certs)
            .build()
            .map_err(|e| PikvmError::WebSocket(e.to_string()))
    }
}

fn parse_auth_token_cookie(header: &str) -> Option<String> {
    header
        .split(';')
        .next()?
        .trim()
        .strip_prefix("auth_token=")
        .map(|t| t.to_string())
}

pub(crate) fn map_status(status: StatusCode, body: String) -> PikvmError {
    let api_msg = serde_json::from_str::<Envelope<ApiErrorBody>>(&body)
        .ok()
        .map(|e| e.result)
        .filter(|e| !e.error.is_empty() || !e.error_msg.is_empty());
    match status {
        StatusCode::UNAUTHORIZED => PikvmError::Unauthenticated,
        StatusCode::FORBIDDEN => PikvmError::Forbidden(api_msg.map(|e| e.error_msg).unwrap_or(body)),
        StatusCode::SERVICE_UNAVAILABLE => PikvmError::Unavailable(api_msg.map(|e| e.error_msg).unwrap_or(body)),
        _ => match api_msg {
            Some(e) => PikvmError::Api {
                error: e.error,
                message: e.error_msg,
            },
            None => PikvmError::Http {
                status: status.as_u16(),
                body,
            },
        },
    }
}

pub(crate) fn check_status(resp: RawResponse) -> Result<RawResponse> {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if status.is_success() {
        Ok(resp)
    } else {
        Err(map_status(status, resp.text()))
    }
}

pub(crate) fn decode_envelope<T: DeserializeOwned>(resp: RawResponse) -> Result<T> {
    let resp = check_status(resp)?;
    let env: Envelope<Value> = serde_json::from_slice(&resp.body)?;
    if !env.ok {
        let e: ApiErrorBody = serde_json::from_value(env.result).unwrap_or(ApiErrorBody {
            error: "unknown".into(),
            error_msg: String::new(),
        });
        return Err(PikvmError::Api {
            error: e.error,
            message: e.error_msg,
        });
    }
    Ok(serde_json::from_value(env.result)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(url: &str) -> PikvmClient {
        let cfg = ConnectionConfig::new(
            ConnectionConfig::parse_base_url(url).unwrap(),
            AuthMethod::Headers,
            Credentials::default(),
        );
        PikvmClient::new(cfg).unwrap()
    }

    #[test]
    fn normalises_base_url() {
        assert_eq!(ConnectionConfig::parse_base_url("pikvm.local").unwrap().as_str(), "https://pikvm.local/");
        assert_eq!(
            ConnectionConfig::parse_base_url("http://10.0.0.5:8080/kvm/").unwrap().as_str(),
            "http://10.0.0.5:8080/kvm"
        );
        assert!(ConnectionConfig::parse_base_url("ftp://x").is_err());
        assert!(ConnectionConfig::parse_base_url("").is_err());
    }

    #[test]
    fn builds_api_and_ws_urls() {
        let c = client("https://pikvm.tail.ts.net/prefix");
        assert_eq!(c.url("/api/info").as_str(), "https://pikvm.tail.ts.net/prefix/api/info");
        assert_eq!(c.ws_url(false).as_str(), "wss://pikvm.tail.ts.net/prefix/api/ws?stream=0");
        assert_eq!(c.ws_url(true).as_str(), "wss://pikvm.tail.ts.net/prefix/api/ws?stream=1");
        let plain = client("http://192.168.1.2");
        assert_eq!(plain.ws_url(true).as_str(), "ws://192.168.1.2/api/ws?stream=1");
        assert_eq!(plain.stream_url().as_str(), "http://192.168.1.2/streamer/stream?dual_final_frames=1");
    }

    #[test]
    fn query_encodes_flags() {
        let q = Query::new().push("action", "on").flag("wait", true).opt("x", None::<u8>).opt_flag("y", Some(false));
        assert_eq!(q.0, vec![("action", "on".to_string()), ("wait", "1".to_string()), ("y", "0".to_string())]);
    }

    #[test]
    fn parses_cookie() {
        assert_eq!(parse_auth_token_cookie("auth_token=abc; Path=/"), Some("abc".into()));
        assert_eq!(parse_auth_token_cookie("other=1"), None);
    }

    #[test]
    fn decodes_envelopes_and_errors() {
        let ok = RawResponse {
            status: 200,
            content_type: "application/json".into(),
            body: Bytes::from(r#"{"ok": true, "result": {"a": 1}}"#),
        };
        let v: Value = decode_envelope(ok).unwrap();
        assert_eq!(v["a"], 1);

        let bad = RawResponse {
            status: 400,
            content_type: "application/json".into(),
            body: Bytes::from(r#"{"ok": false, "result": {"error": "ValidatorError", "error_msg": "bad"}}"#),
        };
        match decode_envelope::<Value>(bad) {
            Err(PikvmError::Api { error, message }) => {
                assert_eq!(error, "ValidatorError");
                assert_eq!(message, "bad");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(map_status(StatusCode::UNAUTHORIZED, String::new()), PikvmError::Unauthenticated));
    }
}
