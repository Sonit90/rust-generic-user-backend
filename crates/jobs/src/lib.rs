//! Background jobs.
//!
//! A `Worker` polls the Postgres queue, claims a batch of due jobs with
//! `SKIP LOCKED`, and dispatches each to a typed handler. Failures are
//! retried with exponential backoff up to `max_attempts`.

pub mod handlers;
pub mod storage;
pub mod worker;

pub use storage::ObjectStore;
pub use worker::{Worker, WorkerConfig};
