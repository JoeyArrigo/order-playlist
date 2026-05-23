//! Impure adapter shell — file IO, HTTP, JSON cache.
//!
//! Adapter trait definitions and feature-gated implementations live here.
//! The default v1 implementations (`musicbrainz`, `reccobeats`) are gated
//! by Cargo features of the same name. Adding a new provider means adding
//! a new feature flag and a new impl; nothing in `algo/` or `domain/`
//! should change.

pub mod cache;
pub mod csv_io;

#[cfg(feature = "musicbrainz")]
pub mod musicbrainz;

use crate::domain::{TrackId, TrackQuery};

// Re-exports of core adapter functions (Task 8 re-export level; traits land in Phase 5/6).
pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};

#[cfg(feature = "musicbrainz")]
pub use musicbrainz::MusicBrainzIsrcResolver;

/// Outcome of resolving a single `TrackQuery`.
///
/// Each query is mapped to either a successful resolution with an ID, or an
/// unresolved state with a human-readable reason.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// The query was successfully resolved to a track ID.
    Resolved { query: TrackQuery, id: TrackId },
    /// The query could not be resolved.
    Unresolved { query: TrackQuery, reason: String },
}

/// Resolves a batch of `TrackQuery`s into IDs (ISRC for v1's
/// `MusicBrainzIsrcResolver`).
///
/// Implementations must:
/// - Honor cache read-through (skip the network when the query is in cache).
/// - Cache both successes AND explicit failures (so unresolvable queries
///   aren't re-attempted across runs).
/// - Emit `tracing::info!` on every network call (AC9.2).
/// - Emit `tracing::warn!` for each unresolved query (AC4.3).
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve multiple queries and return their outcomes.
    ///
    /// The order of results MUST match the order of input queries.
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution>;
}
