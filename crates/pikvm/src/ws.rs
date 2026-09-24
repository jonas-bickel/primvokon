//! WebSocket client for `/api/ws`: receives state events, sends input events, keeps the
//! session alive with pings and reconnects with exponential backoff.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};
use tokio_util::sync::CancellationToken;

use crate::client::PikvmClient;
use crate::error::{PikvmError, Result};
use crate::events::{KvmEvent, OutEvent};

/// Connection status reported alongside events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsStatus {
    Connecting,
    Connected,
    /// Disconnected with the reason; a reconnect is scheduled after `retry_in`.
    Disconnected { reason: String, retry_in: Duration },
    /// The session was cancelled and will not reconnect.
    Closed,
}

/// Messages delivered to the consumer.
#[derive(Debug, Clone, PartialEq)]
pub enum WsMessage {
    Status(WsStatus),
    Event(KvmEvent),
}

/// Reconnect policy.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub ping_interval: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
            ping_interval: Duration::from_secs(15),
        }
    }
}

/// Handle to a running websocket session.
#[derive(Debug, Clone)]
pub struct WsClient {
    out_tx: mpsc::Sender<OutEvent>,
    cancel: CancellationToken,
}

impl WsClient {
    /// Start a supervised session. `stream=true` tells kvmd to run the video streamer for this
    /// client; background services use `false`.
    pub fn connect(client: PikvmClient, stream: bool, policy: ReconnectPolicy) -> (WsClient, mpsc::Receiver<WsMessage>) {
        let (msg_tx, msg_rx) = mpsc::channel(256);
        let (out_tx, out_rx) = mpsc::channel(512);
        let cancel = CancellationToken::new();
        let handle = WsClient {
            out_tx,
            cancel: cancel.clone(),
        };
        tokio::spawn(supervise(client, stream, policy, msg_tx, out_rx, cancel));
        (handle, msg_rx)
    }

    /// Queue an input event (key, mouse, ping). Drops silently when the session is closed.
    pub fn send(&self, ev: OutEvent) {
        let _ = self.out_tx.try_send(ev);
    }

    pub async fn send_async(&self, ev: OutEvent) -> Result<()> {
        self.out_tx
            .send(ev)
            .await
            .map_err(|_| PikvmError::WebSocket("session closed".into()))
    }

    /// Stop the session; no further reconnects.
    pub fn close(&self) {
        self.cancel.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for WsClient {
    fn drop(&mut self) {
        // Only the last clone closes the session.
        if self.out_tx.strong_count() == 1 {
            self.cancel.cancel();
        }
    }
}

async fn supervise(
    client: PikvmClient,
    stream: bool,
    policy: ReconnectPolicy,
    msg_tx: mpsc::Sender<WsMessage>,
    mut out_rx: mpsc::Receiver<OutEvent>,
    cancel: CancellationToken,
) {
    let mut backoff = policy.initial;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        let _ = msg_tx.send(WsMessage::Status(WsStatus::Connecting)).await;
        let result = tokio::select! {
            _ = cancel.cancelled() => break,
            r = run_session(&client, stream, &policy, &msg_tx, &mut out_rx, &cancel) => r,
        };
        match &result {
            Ok(()) => {
                backoff = policy.initial;
                if cancel.is_cancelled() {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!("websocket session ended: {e}");
                if e.is_auth() {
                    // Force a fresh login on the next attempt.
                    client.clear_token().await;
                }
            }
        }
        if cancel.is_cancelled() {
            break;
        }
        let reason = match &result {
            Ok(()) => "connection closed".to_string(),
            Err(e) => e.to_string(),
        };
        let _ = msg_tx
            .send(WsMessage::Status(WsStatus::Disconnected {
                reason,
                retry_in: backoff,
            }))
            .await;
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(policy.max);
    }
    let _ = msg_tx.send(WsMessage::Status(WsStatus::Closed)).await;
}

async fn run_session(
    client: &PikvmClient,
    stream: bool,
    policy: &ReconnectPolicy,
    msg_tx: &mpsc::Sender<WsMessage>,
    out_rx: &mut mpsc::Receiver<OutEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    client.ensure_authenticated().await?;
    let url = client.ws_url(stream);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| PikvmError::WebSocket(e.to_string()))?;
    for (name, value) in client.auth_headers().await?.headers {
        let value = value.parse().map_err(|_| PikvmError::WebSocket("bad header".into()))?;
        request.headers_mut().insert(name, value);
    }
    let connector = Connector::NativeTls(client.tls_connector()?);
    let (ws, _) = connect_async_tls_with_config(request, None, false, Some(connector))
        .await
        .map_err(map_ws_err)?;
    let (mut sink, mut source) = ws.split();
    let _ = msg_tx.send(WsMessage::Status(WsStatus::Connected)).await;
    let mut ping = tokio::time::interval(policy.ping_interval);
    ping.tick().await; // first tick fires immediately
    // Drop stale input queued while offline.
    while out_rx.try_recv().is_ok() {}
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = sink.close().await;
                return Ok(());
            }
            _ = ping.tick() => {
                let text = serde_json::to_string(&OutEvent::Ping {})?;
                sink.send(Message::text(text)).await.map_err(map_ws_err)?;
            }
            out = out_rx.recv() => {
                match out {
                    Some(ev) => {
                        let text = serde_json::to_string(&ev)?;
                        sink.send(Message::text(text)).await.map_err(map_ws_err)?;
                    }
                    None => return Ok(()),
                }
            }
            incoming = source.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match KvmEvent::parse(text.as_str()) {
                            Ok(ev) => {
                                if msg_tx.send(WsMessage::Event(ev)).await.is_err() {
                                    return Ok(());
                                }
                            }
                            Err(e) => tracing::debug!("ignoring unparsable ws message: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err(PikvmError::WebSocket("closed by server".into()));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(map_ws_err(e)),
                }
            }
        }
    }
}

fn map_ws_err(e: tokio_tungstenite::tungstenite::Error) -> PikvmError {
    use tokio_tungstenite::tungstenite::Error as E;
    match e {
        E::Http(resp) => match resp.status().as_u16() {
            401 => PikvmError::Unauthenticated,
            403 => PikvmError::Forbidden("websocket handshake rejected".into()),
            s => PikvmError::Http {
                status: s,
                body: "websocket handshake failed".into(),
            },
        },
        other => PikvmError::WebSocket(other.to_string()),
    }
}
