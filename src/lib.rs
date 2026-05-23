//! `playlistize` library crate.
//!
//! Modules are added as later phases fill them in. Re-exports here let
//! integration tests under `tests/` reach internal types without bypassing
//! visibility rules.

pub mod adapters;
pub mod algo;
pub mod cli;
pub mod domain;
pub mod errors;
pub mod run;

pub use run::{run, ExitCode, RunDeps, RunReport};
