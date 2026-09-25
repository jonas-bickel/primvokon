//! Suspend/resume detection through systemd-logind.
//!
//! logind emits `PrepareForSleep(true)` right before the machine suspends or hibernates and
//! `PrepareForSleep(false)` right after it wakes up. The recall scheduler uses the latter to run
//! summaries that were due while the machine was off. Systems without logind (or without a
//! reachable system bus, e.g. some sandboxes) get `None` and fall back to a wall-clock heuristic.

use futures_util::StreamExt;
use tokio::sync::mpsc;
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    /// `start` is `true` before sleeping and `false` after waking up.
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

/// Subscribes to logind and yields one `()` per resume from suspend/hibernate.
///
/// Returns `None` when logind cannot be reached; the caller then relies on its own fallback.
/// The receiver ends (yields `None`) if the bus connection is lost.
pub async fn resume_events() -> Option<mpsc::Receiver<()>> {
    let connection = match Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            tracing::info!("logind not reachable ({e}); using wall-clock resume detection");
            return None;
        }
    };
    let proxy = match LoginManagerProxy::new(&connection).await {
        Ok(p) => p,
        Err(e) => {
            tracing::info!("logind proxy failed ({e}); using wall-clock resume detection");
            return None;
        }
    };
    let mut signals = match proxy.receive_prepare_for_sleep().await {
        Ok(s) => s,
        Err(e) => {
            tracing::info!("PrepareForSleep subscription failed ({e}); using wall-clock resume detection");
            return None;
        }
    };
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(async move {
        // Keep the connection alive for as long as we listen.
        let _connection = connection;
        while let Some(signal) = signals.next().await {
            match signal.args() {
                Ok(args) if !args.start => {
                    tracing::info!("logind: system resumed from sleep");
                    if tx.send(()).await.is_err() {
                        break;
                    }
                }
                Ok(_) => tracing::debug!("logind: system is going to sleep"),
                Err(e) => tracing::warn!("PrepareForSleep arguments: {e}"),
            }
        }
        tracing::info!("logind signal stream ended");
    });
    Some(rx)
}
