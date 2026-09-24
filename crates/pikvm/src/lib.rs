//! Async client for the PiKVM HTTP and WebSocket API.
//!
//! The crate is GTK-free and organised by API category. [`PikvmClient`] holds the base URL,
//! TLS policy and authentication; the [`api`] modules add one method per documented endpoint.
//! [`ws::WsClient`] streams state events and forwards keyboard/mouse input, and
//! [`stream`] reads the MJPEG video.

pub mod api;
pub mod auth;
pub mod catalog;
pub mod client;
pub mod error;
pub mod events;
pub mod keycodes;
pub mod models;
pub mod stream;
pub mod ws;

pub use auth::{AuthMethod, Credentials};
pub use client::{PikvmClient, RawResponse};
pub use error::{PikvmError, Result};
pub use events::{KvmEvent, KvmState};
