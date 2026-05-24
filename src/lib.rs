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

/// Load cache or return the appropriate exit code and error message.
///
/// This helper is used by main.rs to map Cache::load errors to exit codes
/// and is also exposed for testing purposes. Returns Ok with the loaded cache,
/// or Err with (exit_code, error_message) for error cases.
pub fn load_cache_or_exit_code(
    path: &std::path::Path,
) -> Result<adapters::Cache, (ExitCode, String)> {
    adapters::Cache::load(path).map_err(|e| {
        let message = format!("{}", e);
        (ExitCode::CacheError, message)
    })
}
