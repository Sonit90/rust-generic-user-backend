//! Pure domain types for price-merger.
//!
//! This crate is intentionally free of HTTP, DB, and IO concerns so it can be
//! reused from the `api`, `db`, `jobs`, and `file-processor` crates without
//! pulling in a large dependency graph.

pub mod error;
pub mod models;

pub use error::{AppError, AppResult};
