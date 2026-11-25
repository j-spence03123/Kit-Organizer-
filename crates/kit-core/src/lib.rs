//! Kit Organizer core engine.
//!
//! This crate contains everything that does not depend on the desktop shell:
//! the category model, the rule-based classifier, the recursive scanner,
//! hashing/duplicate detection, the organisation planner/executor with undo,
//! audio metadata extraction and the SQLite persistence layer.
//!
//! Keeping it free of Tauri means it can be unit-tested with plain
//! `cargo test`, and a future ML classifier can be added as another
//! [`classifier::EvidenceSource`] without touching the UI.

pub mod audio;
pub mod benchmark;
pub mod classifier;
pub mod config;
pub mod db;
pub mod error;
pub mod filetypes;
pub mod hashing;
pub mod models;
pub mod organiser;
pub mod pipeline;
pub mod scanner;

pub use error::{CoreError, CoreResult};
