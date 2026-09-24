//! PRIMVOKON domain layer. Everything here is independent of GTK so it can be unit-tested
//! and reused. The GTK crate wires these services to widgets.

pub mod agent;
pub mod ai;
pub mod ntfy;
pub mod paths;
pub mod recall;
pub mod runtime;
pub mod screen;
pub mod secrets;
pub mod settings;
pub mod storage;
pub mod watcher;

pub use pikvm;
