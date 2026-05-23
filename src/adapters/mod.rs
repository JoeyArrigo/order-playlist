//! Impure adapter shell — file IO, HTTP, JSON cache.
//!
//! Adapter trait definitions and feature-gated implementations live here.
//! The default v1 implementations (`musicbrainz`, `reccobeats`) are gated
//! by Cargo features of the same name. Adding a new provider means adding
//! a new feature flag and a new impl; nothing in `algo/` or `domain/`
//! should change.

pub mod cache;
pub mod csv_io;

// Re-exports of core adapter functions (Task 8 re-export level; traits land in Phase 5/6).
pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};

// Submodules added in later phases: musicbrainz (Phase 5), reccobeats (Phase 6).
