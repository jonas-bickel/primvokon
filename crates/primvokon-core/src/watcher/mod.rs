//! USP 1 – watch the host screen for changes and push ntfy notifications.

pub mod detectors;
pub mod service;

pub use detectors::{build_detector, Change, ChangeDetector, Sample};
pub use service::{WatcherEvent, WatcherHandle, WatcherService};
