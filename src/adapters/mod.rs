// pattern: Imperative Shell — file IO / HTTP / cache persistence

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

#[cfg(feature = "reccobeats")]
pub mod reccobeats;

use crate::domain::{TrackFeatures, TrackId, TrackQuery};

// Re-exports of core adapter functions (Task 8 re-export level; traits land in Phase 5/6).
pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};

#[cfg(feature = "musicbrainz")]
pub use musicbrainz::MusicBrainzIsrcResolver;

#[cfg(feature = "reccobeats")]
pub use reccobeats::ReccoBeatsFeatures;

/// Outcome of fetching audio features for a single `TrackId`.
///
/// `ExhaustedRetries` is distinct from `NotFound` so orchestration can map
/// adapter rate-limit exhaustion to `ExitCode::NetworkExhausted` rather than
/// silently treating it as a missing feature.
#[derive(Debug, Clone)]
pub enum FeatureOutcome {
    /// Features were fetched (or cache-hit) successfully.
    Found(TrackFeatures),
    /// The ID was not found by the provider (semantic miss, do not retry).
    NotFound,
    /// Retries were exhausted (e.g., persistent HTTP 429). Transient failure.
    ExhaustedRetries,
}

/// Resolves track IDs to their audio features.
///
/// Implementations must:
/// - Honor cache read-through per ID; only un-cached IDs hit the network.
/// - Cache successes; do NOT cache transient failures (re-attempt on next run).
/// - Emit `tracing::info!` on every network call (AC9.2).
#[async_trait::async_trait]
pub trait FeatureSource: Send + Sync {
    /// Resolve multiple track IDs to their audio features.
    ///
    /// Returns a vec of `(id, outcome)` tuples. Order must match input order.
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, FeatureOutcome)>;
}

/// Outcome of resolving a single `TrackQuery`.
///
/// `ExhaustedRetries` is distinct from `Unresolved` so orchestration can map
/// adapter rate-limit exhaustion to `ExitCode::NetworkExhausted` rather than
/// silently treating it as a semantic miss.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// The query was successfully resolved to a track ID.
    Resolved { query: TrackQuery, id: TrackId },
    /// The query could not be resolved for a semantic reason (no candidates,
    /// cached prior failure). Do not retry.
    Unresolved { query: TrackQuery, reason: String },
    /// Adapter exhausted retries on this query (e.g., persistent HTTP 429/503).
    /// Transient failure; safe to retry on a future run.
    ExhaustedRetries { query: TrackQuery },
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
