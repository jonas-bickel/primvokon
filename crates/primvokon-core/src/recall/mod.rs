//! USP 2 – screen recall: periodic text captures and daily AI summaries.

pub mod capture;
pub mod export;
pub mod summariser;

pub use capture::{CaptureHandle, CaptureService, RecallEvent};
pub use summariser::{Scheduler, SchedulerEvent, Summariser};
